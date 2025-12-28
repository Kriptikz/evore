"use client";

import { useCallback, useEffect, useState, useRef, useMemo } from "react";
import { useConnection, useWallet } from "@solana/wallet-adapter-react";
import { Keypair, PublicKey, Transaction, TransactionInstruction } from "@solana/web3.js";
import { getDeployerPda, getManagedMinerAuthPda, getOreMinerPda, getOreBoardPda } from "@/lib/pda";
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
  mmCreateMinerInstruction,
} from "@/lib/instructions";

// API base URL
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";

// Confirmation polling settings
const CONFIRMATION_POLL_INTERVAL = 1000; // 1 second
const CONFIRMATION_TIMEOUT = 60000; // 60 seconds

// Confirm transaction via ore-stats API
async function confirmTransactionViaApi(signature: string): Promise<void> {
  const startTime = Date.now();
  
  while (Date.now() - startTime < CONFIRMATION_TIMEOUT) {
    try {
      const res = await fetch(`${API_BASE}/signature/${signature}`);
      if (!res.ok) {
        throw new Error(`API error: ${res.status}`);
      }
      
      const data = await res.json();
      
      // Check if confirmed or finalized
      if (data.status === "confirmed" || data.status === "finalized") {
        return;
      }
      
      // Check for error
      if (data.err) {
        throw new Error(`Transaction failed: ${data.err}`);
      }
      
      // Still pending, wait and retry
      await new Promise(resolve => setTimeout(resolve, CONFIRMATION_POLL_INTERVAL));
    } catch (err) {
      // If fetch failed, wait and retry
      await new Promise(resolve => setTimeout(resolve, CONFIRMATION_POLL_INTERVAL));
    }
  }
  
  throw new Error("Transaction confirmation timeout");
}

// Max instructions per transaction to avoid transaction size limits
const MAX_INSTRUCTIONS_PER_TX = 6;

interface ManagerAccount {
  address: PublicKey;
  data: {
    authority: PublicKey;
  };
}

interface DeployerAccount {
  address: PublicKey;
  data: {
    managerKey: PublicKey;
    deployAuthority: PublicKey;
    bpsFee: bigint;
    flatFee: bigint;
    maxPerRound: bigint;
  };
  autodeployBalance: bigint;
  authPdaAddress: PublicKey;
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

  // Manual SOL balance fetching for auth PDAs (not cached on backend)
  const [authBalances, setAuthBalances] = useState<Map<string, bigint>>(new Map());
  const balanceFetchQueueRef = useRef<string[]>([]);
  const balanceFetchingRef = useRef<boolean>(false);

  // Fetch wallet SOL balance via ore-stats API (proxied RPC)
  const fetchWalletBalance = useCallback(async () => {
    if (!publicKey) {
      setWalletBalance(BigInt(0));
      return;
    }
    try {
      const res = await fetch(`${API_BASE}/balance/${publicKey.toBase58()}`);
      if (!res.ok) {
        console.error("Error fetching wallet balance from API");
        return;
      }
      const data = await res.json();
      setWalletBalance(BigInt(data.lamports || 0));
    } catch (err) {
      console.error("Error fetching wallet balance:", err);
    }
  }, [publicKey]);
  
  // Fetch a single auth PDA balance (rate limited - 1 per second)
  const fetchAuthBalance = useCallback(async (authPda: string) => {
    try {
      console.log(`[useEvore] Fetching balance for auth PDA: ${authPda}`);
      const res = await fetch(`${API_BASE}/balance/${authPda}`);
      if (!res.ok) {
        console.error(`[useEvore] Failed to fetch balance for ${authPda}: ${res.status}`);
      return;
    }
      const data = await res.json();
      console.log(`[useEvore] Got balance for ${authPda}: ${data.lamports} lamports`);
      setAuthBalances(prev => {
        const next = new Map(prev);
        next.set(authPda, BigInt(data.lamports || 0));
        return next;
      });
    } catch (err) {
      console.error(`Error fetching auth balance for ${authPda}:`, err);
    }
  }, []);
  
  // Process balance fetch queue with 1s rate limiting
  const processBalanceQueue = useCallback(async () => {
    if (balanceFetchingRef.current) return;
    if (balanceFetchQueueRef.current.length === 0) return;
    
    balanceFetchingRef.current = true;
    
    while (balanceFetchQueueRef.current.length > 0) {
      const authPda = balanceFetchQueueRef.current.shift();
      if (authPda) {
        await fetchAuthBalance(authPda);
        // Wait 1 second between fetches (rate limit)
        await new Promise(resolve => setTimeout(resolve, 1000));
      }
    }
    
    balanceFetchingRef.current = false;
  }, [fetchAuthBalance]);
  
  // Queue auth PDAs for balance fetching
  const queueAuthBalances = useCallback((authPdas: string[]) => {
    // Only queue PDAs we haven't fetched yet
    const newPdas = authPdas.filter(pda => !authBalances.has(pda));
    balanceFetchQueueRef.current = Array.from(new Set([...balanceFetchQueueRef.current, ...newPdas]));
    processBalanceQueue();
  }, [authBalances, processBalanceQueue]);

  // Fetch board data from API
  const fetchBoard = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/round`);
      if (!res.ok) {
        console.error("Error fetching board from API");
        return;
      }
      const data = await res.json();
      setBoard({
        roundId: BigInt(data.round_id),
        endSlot: BigInt(data.end_slot),
      });
    } catch (err) {
      console.error("Error fetching board:", err);
    }
  }, []);

  // Fetch all user data from API (managers, deployers, miners)
  const fetchMyMiners = useCallback(async () => {
    if (!publicKey) {
      setManagers([]);
      setDeployers([]);
      setMiners(new Map());
      return;
    }

    try {
      setLoading(true);
      const res = await fetch(`${API_BASE}/evore/my-miners/${publicKey.toBase58()}`);
      if (!res.ok) {
        console.error("Error fetching my miners from API");
        return;
      }
      
      const data = await res.json();
      
      // Debug: log raw API response to see what we're getting
      console.log("[useEvore] API response:", JSON.stringify(data, null, 2));
      
      // Transform API response to hook state format
      const newManagers: ManagerAccount[] = [];
      const newDeployers: DeployerAccount[] = [];
      const newMiners = new Map<string, MinerAccount>();
      const authPdasToFetch: string[] = [];
      
      for (const autominer of data.autominers || []) {
        const managerAddr = new PublicKey(autominer.manager.address);
        
        // Manager
        newManagers.push({
          address: managerAddr,
          data: {
            authority: new PublicKey(autominer.manager.authority),
          },
        });
        
        // Process all miners for this manager
        // API now returns `miners` array instead of single `miner`
        const miners = autominer.miners || [];
        
        // Find the first miner to use for deployer's auth PDA (usually auth_id 0)
        const firstMiner = miners.length > 0 ? miners[0] : null;
        const firstAuthId = firstMiner ? BigInt(firstMiner.auth_id || 0) : BigInt(0);
        
        // Deployer (if exists)
        if (autominer.deployer) {
          const [deployerPda] = getDeployerPda(managerAddr);
          const [authPda] = getManagedMinerAuthPda(managerAddr, firstAuthId);
          const authPdaStr = authPda.toBase58();
          authPdasToFetch.push(authPdaStr);
          
          newDeployers.push({
            address: deployerPda,
            data: {
              managerKey: managerAddr,
              deployAuthority: new PublicKey(autominer.deployer.deploy_authority),
              bpsFee: BigInt(autominer.deployer.bps_fee || 0),
              flatFee: BigInt(autominer.deployer.flat_fee || 0),
              maxPerRound: BigInt(autominer.deployer.max_per_round || 1_000_000_000),
            },
            // Auth balance not cached on backend - will be fetched manually
            autodeployBalance: BigInt(0),
            authPdaAddress: authPda,
          });
          
          // Queue balance fetching for ALL miners' auth PDAs
          for (const miner of miners) {
            const minerAuthId = BigInt(miner.auth_id || 0);
            const [minerAuthPda] = getManagedMinerAuthPda(managerAddr, minerAuthId);
            const minerAuthPdaStr = minerAuthPda.toBase58();
            if (!authPdasToFetch.includes(minerAuthPdaStr)) {
              authPdasToFetch.push(minerAuthPdaStr);
            }
          }
        }
        
        // Process all miners
        for (const miner of miners) {
          const authId = BigInt(miner.auth_id || 0);
          const [authPda] = getManagedMinerAuthPda(managerAddr, authId);
          // Use manager address + auth_id as the key to support multiple miners per manager
          const minerKey = `${managerAddr.toBase58()}-${authId}`;
          
          newMiners.set(minerKey, {
            address: new PublicKey(miner.address),
            authority: authPda, // The miner's authority is the ManagedMinerAuth PDA
            roundId: BigInt(miner.round_id || 0),
            checkpointId: BigInt(miner.checkpoint_id || 0),
            deployed: (miner.deployed || new Array(25).fill(0)).map((v: number) => BigInt(v)),
            rewardsSol: BigInt(miner.rewards_sol || 0),
            rewardsOre: BigInt(miner.rewards_ore || 0),
            refinedOre: BigInt(miner.refined_ore || 0),
          });
        }
      }
      
      setManagers(newManagers);
      setDeployers(newDeployers);
      setMiners(newMiners);
      
      // Queue auth PDAs for manual balance fetching
      if (authPdasToFetch.length > 0) {
        console.log(`[useEvore] Queuing ${authPdasToFetch.length} auth PDAs for balance fetching:`, authPdasToFetch);
        queueAuthBalances(authPdasToFetch);
      }
    } catch (err) {
      console.error("Error fetching my miners:", err);
      setError("Failed to fetch data from API");
    } finally {
      setLoading(false);
    }
  }, [publicKey, queueAuthBalances]);

  // Deployers with updated auth balances from manual fetching
  const deployersWithBalances = useMemo(() => {
    return deployers.map(d => ({
      ...d,
      // Use manually fetched balance if available, otherwise use cached
      autodeployBalance: authBalances.get(d.authPdaAddress.toBase58()) ?? d.autodeployBalance,
    }));
  }, [deployers, authBalances]);
  
  // Legacy wrappers for compatibility
  const fetchManagers = fetchMyMiners;
  const fetchDeployers = fetchMyMiners;
  const fetchMiners = fetchMyMiners;

  // Create a new manager account
  const createManager = useCallback(async (managerKeypair: Keypair) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const ix = createManagerInstruction(publicKey, managerKeypair.publicKey);
    const tx = new Transaction().add(ix);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    tx.partialSign(managerKeypair);

    const signature = await sendTransaction(tx, connection);
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Create an AutoMiner (manager + deployer + miner in one transaction)
  const createAutoMiner = useCallback(async (
    deployAuthority: PublicKey,
    bpsFee: bigint,
    flatFee: bigint = BigInt(0),
    maxPerRound: bigint = BigInt(1_000_000_000)
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const managerKeypair = Keypair.generate();

    const createManagerIx = createManagerInstruction(publicKey, managerKeypair.publicKey);
    const createDeployerIx = createDeployerInstruction(publicKey, managerKeypair.publicKey, deployAuthority, bpsFee, flatFee, maxPerRound);
    const createMinerIx = mmCreateMinerInstruction(publicKey, managerKeypair.publicKey, BigInt(0));

    const tx = new Transaction()
      .add(createManagerIx)
      .add(createDeployerIx)
      .add(createMinerIx);
    
    const { blockhash } = await connection.getLatestBlockhash();
    tx.recentBlockhash = blockhash;
    tx.feePayer = publicKey;

    tx.partialSign(managerKeypair);

    const signature = await sendTransaction(tx, connection);
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Bulk create multiple AutoMiners
  const AUTOMINERS_PER_TX = 3;
  
  const bulkCreateAutoMiners = useCallback(async (
    count: number,
    deployAuthority: PublicKey,
    bpsFee: bigint,
    flatFee: bigint = BigInt(0),
    maxPerRound: bigint = BigInt(1_000_000_000),
    onProgress?: (completed: number, total: number) => void
  ): Promise<string[]> => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (count <= 0) throw new Error("Count must be positive");

    const signatures: string[] = [];
    let created = 0;
    
    while (created < count) {
      const batchSize = Math.min(AUTOMINERS_PER_TX, count - created);
      const keypairs: Keypair[] = [];
      
      const tx = new Transaction();
      
      for (let i = 0; i < batchSize; i++) {
        const managerKeypair = Keypair.generate();
        keypairs.push(managerKeypair);
        
        tx.add(createManagerInstruction(publicKey, managerKeypair.publicKey));
        tx.add(createDeployerInstruction(publicKey, managerKeypair.publicKey, deployAuthority, bpsFee, flatFee, maxPerRound));
        tx.add(mmCreateMinerInstruction(publicKey, managerKeypair.publicKey, BigInt(0)));
      }
      
      const { blockhash } = await connection.getLatestBlockhash();
      tx.recentBlockhash = blockhash;
      tx.feePayer = publicKey;

      for (const keypair of keypairs) {
        tx.partialSign(keypair);
      }

      const signature = await sendTransaction(tx, connection);
      await confirmTransactionViaApi(signature);
      signatures.push(signature);
      
      created += batchSize;
      
      if (onProgress) {
        onProgress(created, count);
      }
    }
    
    await fetchMyMiners();
    return signatures;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Helper to send batched transactions
  const sendBatchedTransactions = useCallback(async (
    instructions: TransactionInstruction[],
    onProgress?: (completed: number, total: number) => void
  ): Promise<string[]> => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (instructions.length === 0) throw new Error("No instructions to send");

    const signatures: string[] = [];
    const batches: TransactionInstruction[][] = [];
    
    for (let i = 0; i < instructions.length; i += MAX_INSTRUCTIONS_PER_TX) {
      batches.push(instructions.slice(i, i + MAX_INSTRUCTIONS_PER_TX));
    }

    for (let i = 0; i < batches.length; i++) {
      const batch = batches[i];
      const tx = new Transaction();
      
      for (const ix of batch) {
        tx.add(ix);
      }
      
      const { blockhash } = await connection.getLatestBlockhash();
      tx.recentBlockhash = blockhash;
      tx.feePayer = publicKey;

      const signature = await sendTransaction(tx, connection);
      await confirmTransactionViaApi(signature);
      signatures.push(signature);
      
      if (onProgress) {
        onProgress(i + 1, batches.length);
      }
    }

    return signatures;
  }, [connection, publicKey, sendTransaction]);

  // Bulk update multiple deployers
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

    const instructions: TransactionInstruction[] = managerAccounts.map(managerAccount =>
      updateDeployerInstruction(publicKey, managerAccount, newDeployAuthority, newBpsFee, newFlatFee, newExpectedBpsFee, newExpectedFlatFee, newMaxPerRound)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Bulk deposit to multiple managers
  const bulkDepositAutodeployBalance = useCallback(async (
    managerAccounts: PublicKey[],
    authId: bigint,
    amount: bigint
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (managerAccounts.length === 0) throw new Error("No managers to deposit to");

    const instructions: TransactionInstruction[] = managerAccounts.map(managerAccount =>
      depositAutodeployBalanceInstruction(publicKey, managerAccount, authId, amount)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Bulk withdraw from multiple managers
  const bulkWithdrawAutodeployBalance = useCallback(async (
    withdrawals: { managerAccount: PublicKey; authId: bigint; amount: bigint }[]
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (withdrawals.length === 0) throw new Error("No managers to withdraw from");

    const instructions: TransactionInstruction[] = withdrawals.map(({ managerAccount, authId, amount }) =>
      withdrawAutodeployBalanceInstruction(publicKey, managerAccount, authId, amount)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Bulk checkpoint multiple miners
  const bulkCheckpoint = useCallback(async (
    checkpoints: { managerAccount: PublicKey; roundId: bigint; authId: bigint }[]
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (checkpoints.length === 0) throw new Error("No miners to checkpoint");

    const instructions: TransactionInstruction[] = checkpoints.map(({ managerAccount, roundId, authId }) =>
      mmCheckpointInstruction(publicKey, managerAccount, roundId, authId)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Bulk claim SOL from multiple miners
  const bulkClaimSol = useCallback(async (
    claims: { managerAccount: PublicKey; authId: bigint }[]
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (claims.length === 0) throw new Error("No miners to claim SOL from");

    const instructions: TransactionInstruction[] = claims.map(({ managerAccount, authId }) =>
      mmClaimSolInstruction(publicKey, managerAccount, authId)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Bulk claim ORE from multiple miners
  const bulkClaimOre = useCallback(async (
    claims: { managerAccount: PublicKey; authId: bigint }[]
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");
    if (claims.length === 0) throw new Error("No miners to claim ORE from");

    const instructions: TransactionInstruction[] = claims.map(({ managerAccount, authId }) =>
      mmClaimOreInstruction(publicKey, managerAccount, authId)
    );
    
    const signatures = await sendBatchedTransactions(instructions);
    
    await fetchMyMiners();
    return signatures;
  }, [publicKey, sendBatchedTransactions, fetchMyMiners]);

  // Deposit to autodeploy balance
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Withdraw from autodeploy balance
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Withdraw all
  const withdrawAll = useCallback(async (
    managerAccount: PublicKey,
    authId: bigint,
    rewardsSol: bigint,
    autodeployBalance: bigint
  ) => {
    if (!publicKey) throw new Error("Wallet not connected");

    const tx = new Transaction();
    
    if (rewardsSol > BigInt(0)) {
      const claimSolIx = mmClaimSolInstruction(publicKey, managerAccount, authId);
      tx.add(claimSolIx);
    }
    
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Checkpoint a miner
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Claim SOL rewards
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Claim ORE rewards
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Transfer manager authority
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
    await confirmTransactionViaApi(signature);
    
    await fetchMyMiners();
    return signature;
  }, [connection, publicKey, sendTransaction, fetchMyMiners]);

  // Refresh all data
  const refreshAll = useCallback(async () => {
    await fetchMyMiners();
    await fetchBoard();
    await fetchWalletBalance();
  }, [fetchMyMiners, fetchBoard, fetchWalletBalance]);

  // Auto-fetch on wallet change
  useEffect(() => {
    fetchMyMiners();
    fetchBoard();
    fetchWalletBalance();
  }, [fetchMyMiners, fetchBoard, fetchWalletBalance]);

  // Auto-refresh interval
  useEffect(() => {
    if (refreshIntervalRef.current) {
      clearInterval(refreshIntervalRef.current);
    }

    if (publicKey) {
      refreshIntervalRef.current = setInterval(() => {
        fetchMyMiners();
        fetchBoard();
        fetchWalletBalance();
      }, REFRESH_INTERVAL);
    }

    return () => {
      if (refreshIntervalRef.current) {
        clearInterval(refreshIntervalRef.current);
      }
    };
  }, [publicKey, fetchMyMiners, fetchBoard, fetchWalletBalance]);

  return {
    managers,
    deployers: deployersWithBalances,
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
    bulkCreateAutoMiners,
    updateDeployer,
    bulkUpdateDeployers,
    depositAutodeployBalance,
    withdrawAutodeployBalance,
    bulkDepositAutodeployBalance,
    bulkWithdrawAutodeployBalance,
    withdrawAll,
    checkpoint,
    bulkCheckpoint,
    claimSol,
    bulkClaimSol,
    claimOre,
    bulkClaimOre,
    transferManager,
  };
}
