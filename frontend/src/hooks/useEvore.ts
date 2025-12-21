"use client";

import { useCallback, useEffect, useState, useRef } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { Keypair, PublicKey, Transaction } from "@solana/web3.js";
import { getDeployerPda, getManagedMinerAuthPda, getOreMinerPda, getOreBoardPda } from "@/lib/pda";
import { Manager, Deployer, decodeManager, decodeDeployer } from "@/lib/accounts";
import {
  createManagerInstruction,
  transferManagerInstruction,
  createDeployerInstruction,
  updateDeployerInstruction,
  depositAutodeployBalanceInstruction,
  withdrawAutodeployBalanceInstruction,
  mmCheckpointInstruction,
  mmClaimSolInstruction,
  mmClaimOreInstruction,
} from "@/lib/instructions";
import { EVORE_PROGRAM_ID, MANAGER_DISCRIMINATOR } from "@/lib/constants";

interface ManagerAccount {
  address: PublicKey;
  data: Manager;
}

interface DeployerAccount {
  address: PublicKey;
  data: Deployer;
  autodeployBalance: bigint;  // Balance in managed_miner_auth PDA (auth_id 0)
  authPdaAddress: PublicKey;  // The managed_miner_auth PDA address
}

interface MinerAccount {
  address: PublicKey;
  authority: PublicKey;
  roundId: bigint;
  checkpointId: bigint;
  deployed: bigint[];
  rewardsSol: bigint;
  rewardsOre: bigint;
  refinedOre: bigint;
}

interface BoardData {
  roundId: bigint;
  endSlot: bigint;
}

// Auto-refresh interval in ms
const REFRESH_INTERVAL = 5000;

export function useEvore() {
  const { connection } = useConnection();
  const { publicKey, sendTransaction } = useWallet();

  const [managers, setManagers] = useState<ManagerAccount[]>([]);
  const [deployers, setDeployers] = useState<DeployerAccount[]>([]);
  const [miners, setMiners] = useState<Map<string, MinerAccount>>(new Map());
  const [board, setBoard] = useState<BoardData | null>(null);
  const [walletBalance, setWalletBalance] = useState<bigint>(BigInt(0));
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const refreshIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Fetch wallet SOL balance
  const fetchWalletBalance = useCallback(async () => {
    if (!publicKey) {
      setWalletBalance(BigInt(0));
      return;
    }
    try {
      const balance = await connection.getBalance(publicKey);
      setWalletBalance(BigInt(balance));
    } catch (err) {
      console.error("Error fetching wallet balance:", err);
    }
  }, [connection, publicKey]);

  // Fetch all managers owned by the connected wallet
  const fetchManagers = useCallback(async () => {
    if (!publicKey) {
      setManagers([]);
      return;
    }

    try {
      setLoading(true);
      
      // Get all manager accounts where authority matches wallet
      const accounts = await connection.getProgramAccounts(EVORE_PROGRAM_ID, {
        filters: [
          // Filter by discriminator (Manager = 100)
          {
            memcmp: {
              offset: 0,
              bytes: Buffer.from([MANAGER_DISCRIMINATOR, 0, 0, 0, 0, 0, 0, 0]).toString('base64'),
              encoding: 'base64',
            },
          },
          // Filter by authority
          {
            memcmp: {
              offset: 8,
              bytes: publicKey.toBase58(),
            },
          },
        ],
      });

      const decoded = accounts.map(({ pubkey, account }) => ({
        address: pubkey,
        data: decodeManager(Buffer.from(account.data)),
      }));

      setManagers(decoded);
    } catch (err) {
      console.error("Error fetching managers:", err);
      setError("Failed to fetch managers");
    } finally {
      setLoading(false);
    }
  }, [connection, publicKey]);

  // Fetch deployer for each manager
  const fetchDeployers = useCallback(async () => {
    if (managers.length === 0) {
      setDeployers([]);
      return;
    }

    try {
      const deployerPromises = managers.map(async (manager) => {
        const [deployerPda] = getDeployerPda(manager.address);
        // Funds are now held in managed_miner_auth PDA (auth_id 0)
        const [authPda] = getManagedMinerAuthPda(manager.address, BigInt(0));

        try {
          const accountInfo = await connection.getAccountInfo(deployerPda);
          if (!accountInfo) return null;

          const data = decodeDeployer(Buffer.from(accountInfo.data));
          
          // Get balance from managed_miner_auth PDA
          const balance = await connection.getBalance(authPda);

          return {
            address: deployerPda,
            data,
            autodeployBalance: BigInt(balance),
            authPdaAddress: authPda,
          };
        } catch {
          return null;
        }
      });

      const results = await Promise.all(deployerPromises);
      setDeployers(results.filter((d): d is DeployerAccount => d !== null));
    } catch (err) {
      console.error("Error fetching deployers:", err);
    }
  }, [connection, managers]);

  // Fetch board data (current round info)
  const fetchBoard = useCallback(async () => {
    try {
      const [boardPda] = getOreBoardPda();
      const accountInfo = await connection.getAccountInfo(boardPda);
      
      if (!accountInfo) {
        setBoard(null);
        return;
      }

      const data = Buffer.from(accountInfo.data);
      // Board layout:
      // 8 bytes discriminator
      // 8 bytes round_id
      // 8 bytes start_slot
      // 8 bytes end_slot
      const roundId = data.readBigUInt64LE(8);
      const endSlot = data.readBigUInt64LE(24);
      
      setBoard({ roundId, endSlot });
    } catch (err) {
      console.error("Error fetching board:", err);
    }
  }, [connection]);

  // Fetch miner accounts for each manager (auth_id 0)
  const fetchMiners = useCallback(async () => {
    if (managers.length === 0) {
      setMiners(new Map());
      return;
    }

    try {
      const newMiners = new Map<string, MinerAccount>();
      
      for (const manager of managers) {
        const [managedMinerAuthPda] = getManagedMinerAuthPda(manager.address, BigInt(0));
        const [oreMinerPda] = getOreMinerPda(managedMinerAuthPda);

        try {
          const accountInfo = await connection.getAccountInfo(oreMinerPda);
          if (!accountInfo) continue;

          const data = Buffer.from(accountInfo.data);
          // ORE Miner layout:
          // 8 bytes discriminator
          // 32 bytes authority
          // 25 * 8 = 200 bytes deployed array
          // 25 * 8 = 200 bytes cumulative array
          // 8 bytes checkpoint_fee
          // 8 bytes checkpoint_id
          // 8 bytes last_claim_ore_at
          // 8 bytes last_claim_sol_at
          // 16 bytes rewards_factor (Numeric)
          // 8 bytes rewards_sol
          // 8 bytes rewards_ore
          // 8 bytes refined_ore
          // 8 bytes round_id
          
          // Authority is at offset 8 (after discriminator)
          const authority = new PublicKey(data.subarray(8, 8 + 32));
          
          const deployed: bigint[] = [];
          for (let i = 0; i < 25; i++) {
            deployed.push(data.readBigUInt64LE(8 + 32 + i * 8));
          }
          
          const checkpointId = data.readBigUInt64LE(8 + 32 + 200 + 200 + 8);
          const rewardsSol = data.readBigUInt64LE(8 + 32 + 200 + 200 + 8 + 8 + 8 + 8 + 16);
          const rewardsOre = data.readBigUInt64LE(8 + 32 + 200 + 200 + 8 + 8 + 8 + 8 + 16 + 8);
          const refinedOre = data.readBigUInt64LE(8 + 32 + 200 + 200 + 8 + 8 + 8 + 8 + 16 + 8 + 8);
          const roundId = data.readBigUInt64LE(8 + 32 + 200 + 200 + 8 + 8 + 8 + 8 + 16 + 8 + 8 + 8);
          
          newMiners.set(manager.address.toBase58(), {
            address: oreMinerPda,
            authority: authority,
            roundId,
            checkpointId,
            deployed,
            rewardsSol,
            rewardsOre,
            refinedOre,
          });
        } catch (err) {
          console.error("Error fetching miner for", manager.address.toBase58(), err);
        }
      }

      setMiners(newMiners);
    } catch (err) {
      console.error("Error fetching miners:", err);
    }
  }, [connection, managers]);

  // Create a new manager account
  // Note: managerKeypair must sign the transaction
  const createManager = useCallback(async (managerKeypair: Keypair) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = createManagerInstruction(publicKey, managerKeypair.publicKey);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    // Partially sign with the manager keypair first
    tx.partialSign(managerKeypair);

    // Then send to wallet for the payer's signature
    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchManagers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchManagers]);

  // Create a deployer for a manager
  const createDeployer = useCallback(async (
    managerAccount: PublicKey,
    deployAuthority: PublicKey,
    bpsFee: bigint,
    flatFee: bigint = BigInt(0),
    maxPerRound: bigint = BigInt(1_000_000_000)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = createDeployerInstruction(publicKey, managerAccount, deployAuthority, bpsFee, flatFee, maxPerRound);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers]);

  // Create an AutoMiner (manager + deployer in one transaction)
  const createAutoMiner = useCallback(async (
    deployAuthority: PublicKey,
    bpsFee: bigint,
    flatFee: bigint = BigInt(0),
    maxPerRound: bigint = BigInt(1_000_000_000)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    // Generate new manager keypair
    const managerKeypair = Keypair.generate();

    // Create both instructions
    const createManagerIx = createManagerInstruction(publicKey, managerKeypair.publicKey);
    const createDeployerIx = createDeployerInstruction(publicKey, managerKeypair.publicKey, deployAuthority, bpsFee, flatFee, maxPerRound);

    const tx = new Transaction().add(createManagerIx).add(createDeployerIx);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    // Partially sign with the manager keypair
    tx.partialSign(managerKeypair);

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchManagers();
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchManagers, fetchDeployers]);

  // Update a deployer
  const updateDeployer = useCallback(async (
    managerAccount: PublicKey,
    newDeployAuthority: PublicKey,
    newBpsFee: bigint,
    newFlatFee: bigint = BigInt(0),
    newExpectedBpsFee: bigint = BigInt(0),
    newExpectedFlatFee: bigint = BigInt(0),
    newMaxPerRound: bigint = BigInt(1_000_000_000)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = updateDeployerInstruction(publicKey, managerAccount, newDeployAuthority, newBpsFee, newFlatFee, newExpectedBpsFee, newExpectedFlatFee, newMaxPerRound);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers]);

  // Bulk update multiple deployers in a single transaction
  const bulkUpdateDeployers = useCallback(async (
    managerAccounts: PublicKey[],
    newDeployAuthority: PublicKey,
    newBpsFee: bigint,
    newFlatFee: bigint = BigInt(0),
    newExpectedBpsFee: bigint = BigInt(0),
    newExpectedFlatFee: bigint = BigInt(0),
    newMaxPerRound: bigint = BigInt(1_000_000_000)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (managerAccounts.length === 0) throw new Error("No managers to update");

    const tx = new Transaction();
    
    for (const managerAccount of managerAccounts) {
      const ix = updateDeployerInstruction(publicKey, managerAccount, newDeployAuthority, newBpsFee, newFlatFee, newExpectedBpsFee, newExpectedFlatFee, newMaxPerRound);
      tx.add(ix);
    }
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers]);

  // Deposit to autodeploy balance (to managed_miner_auth PDA)
  const depositAutodeployBalance = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint,
    amount: bigint
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = depositAutodeployBalanceInstruction(publicKey, managerAccount, authId, amount);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers]);

  // Withdraw from autodeploy balance (from managed_miner_auth PDA)
  const withdrawAutodeployBalance = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint,
    amount: bigint
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = withdrawAutodeployBalanceInstruction(publicKey, managerAccount, authId, amount);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers]);

  // Withdraw all: claim SOL (if available) + withdraw autodeploy balance
  const withdrawAll = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint,
    rewardsSol: bigint,
    autodeployBalance: bigint
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const tx = new Transaction();
    
    // Add claim SOL instruction if there are rewards
    if (rewardsSol > BigInt(0)) {
      const claimSolIx = mmClaimSolInstruction(publicKey, managerAccount, authId);
      tx.add(claimSolIx);
    }
    
    // Add withdraw instruction if there's balance to withdraw
    if (autodeployBalance > BigInt(0)) {
      const withdrawIx = withdrawAutodeployBalanceInstruction(publicKey, managerAccount, authId, autodeployBalance);
      tx.add(withdrawIx);
    }
    
    if (tx.instructions.length === 0) {
      throw new Error("Nothing to withdraw");
    }
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchDeployers();
    await fetchMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchDeployers, fetchMiners]);

  // Checkpoint a miner to claim round winnings
  const checkpoint = useCallback(async (
    managerAccount: PublicKey,
    roundId: bigint,
    authId: bigint = BigInt(0)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = mmCheckpointInstruction(publicKey, managerAccount, roundId, authId);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMiners]);

  // Claim SOL rewards from miner to manager authority
  const claimSol = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint = BigInt(0)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = mmClaimSolInstruction(publicKey, managerAccount, authId);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMiners]);

  // Claim ORE token rewards from miner to signer
  const claimOre = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint = BigInt(0)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = mmClaimOreInstruction(publicKey, managerAccount, authId);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    await fetchMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMiners]);

  // Transfer manager authority to a new pubkey
  const transferManager = useCallback(async (
    managerAccount: PublicKey,
    newAuthority: PublicKey
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = transferManagerInstruction(publicKey, managerAccount, newAuthority);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    const signature = await sendTransaction(tx, connection);
    await connection.confirmTransaction(signature, "confirmed");
    
    // Refresh managers to remove transferred one from list
    await fetchManagers();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchManagers]);

  // Refresh all data
  const refreshAll = useCallback(async () => {
    await fetchManagers();
  }, [fetchManagers]);

  // Auto-fetch on wallet change
  useEffect(() => {
    fetchManagers();
    fetchWalletBalance();
  }, [fetchManagers, fetchWalletBalance]);

  // Auto-fetch deployers, miners, and board when managers change
  useEffect(() => {
    fetchDeployers();
    fetchMiners();
    fetchBoard();
  }, [fetchDeployers, fetchMiners, fetchBoard]);

  // Auto-refresh interval
  useEffect(() => {
    // Clear any existing interval
    if (refreshIntervalRef.current) {
      clearInterval(refreshIntervalRef.current);
    }

    // Set up auto-refresh if we have a connected wallet
    if (publicKey) {
      refreshIntervalRef.current = setInterval(() => {
        fetchDeployers();
        fetchMiners();
        fetchBoard();
        fetchWalletBalance();
      }, REFRESH_INTERVAL);
    }

    return () => {
      if (refreshIntervalRef.current) {
        clearInterval(refreshIntervalRef.current);
      }
    };
  }, [publicKey, fetchDeployers, fetchMiners, fetchBoard]);

  return {
    managers,
    deployers,
    miners,
    board,
    walletBalance,
    loading,
    error,
    fetchManagers,
    fetchDeployers,
    fetchMiners,
    fetchBoard,
    fetchWalletBalance,
    refreshAll,
    createManager,
    createDeployer,
    createAutoMiner,
    updateDeployer,
    bulkUpdateDeployers,
    depositAutodeployBalance,
    withdrawAutodeployBalance,
    withdrawAll,
    checkpoint,
    claimSol,
    claimOre,
    transferManager,
  };
}
