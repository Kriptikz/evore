"use client";

import { useState } from "react";
import { useWallet } from "@solana/wallet-adapter-react";
import { Keypair, PublicKey } from "@solana/web3.js";
import { Header } from "@/components/Header";
import { ManagerCard } from "@/components/ManagerCard";
import { CreateManagerForm } from "@/components/CreateManagerForm";
import { BulkActionBar } from "@/components/BulkActionBar";
import { useEvore } from "@/hooks/useEvore";

export default function ManagePage() {
  const { publicKey, connected } = useWallet();
  const {
    managers,
    deployers,
    miners,
    board,
    loading,
    createManager,
    createDeployer,
    updateDeployer,
    bulkUpdateDeployers,
    depositAutodeployBalance,
    withdrawAutodeployBalance,
    bulkDepositAutodeployBalance,
    bulkWithdrawAutodeployBalance,
    checkpoint,
    bulkCheckpoint,
    claimSol,
    bulkClaimSol,
    claimOre,
    bulkClaimOre,
    transferManager,
  } = useEvore();

  const [selectedManagers, setSelectedManagers] = useState<Set<string>>(new Set());

  // Find deployer for a manager
  const getDeployerForManager = (managerAddress: PublicKey) => {
    return deployers.find(
      (d) => d.data.managerKey.toBase58() === managerAddress.toBase58()
    );
  };

  // Find miner for a manager
  const getMinerForManager = (managerAddress: PublicKey) => {
    return miners.get(managerAddress.toBase58());
  };

  // Selection helpers
  const toggleSelection = (managerKey: string) => {
    setSelectedManagers(prev => {
      const next = new Set(prev);
      if (next.has(managerKey)) {
        next.delete(managerKey);
      } else {
        next.add(managerKey);
      }
      return next;
    });
  };

  // Sort managers by claimable ORE (descending - most ORE first)
  const sortedManagers = [...managers].sort((a, b) => {
    const minerA = miners.get(a.address.toBase58());
    const minerB = miners.get(b.address.toBase58());
    const oreA = (minerA?.rewardsOre || BigInt(0)) + (minerA?.refinedOre || BigInt(0));
    const oreB = (minerB?.rewardsOre || BigInt(0)) + (minerB?.refinedOre || BigInt(0));
    // Sort descending (most ORE first)
    if (oreB > oreA) return 1;
    if (oreB < oreA) return -1;
    return 0;
  });

  const selectAll = () => {
    setSelectedManagers(new Set(sortedManagers.map(m => m.address.toBase58())));
  };

  const deselectAll = () => {
    setSelectedManagers(new Set());
  };

  // Bulk action handlers - all now use batched transactions that auto-split if needed
  const handleBulkDeposit = async (authId: bigint, amount: bigint) => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    if (selected.length === 0) return;
    
    await bulkDepositAutodeployBalance(
      selected.map(m => m.address),
      authId,
      amount
    );
  };

  const handleBulkWithdraw = async () => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    const withdrawals: { managerAccount: PublicKey; authId: bigint; amount: bigint }[] = [];
    
    for (const manager of selected) {
      const deployer = getDeployerForManager(manager.address);
      if (deployer && deployer.autodeployBalance > BigInt(0)) {
        withdrawals.push({
          managerAccount: manager.address,
          authId: BigInt(0),
          amount: deployer.autodeployBalance,
        });
      }
    }
    
    if (withdrawals.length === 0) return;
    await bulkWithdrawAutodeployBalance(withdrawals);
  };

  const handleBulkCheckpoint = async () => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    const checkpoints: { managerAccount: PublicKey; roundId: bigint; authId: bigint }[] = [];
    
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && board?.roundId && miner.checkpointId < miner.roundId && miner.roundId < board.roundId) {
        checkpoints.push({
          managerAccount: manager.address,
          roundId: miner.roundId,
          authId: BigInt(0),
        });
      }
    }
    
    if (checkpoints.length === 0) return;
    await bulkCheckpoint(checkpoints);
  };

  const handleBulkClaimSol = async () => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    const claims: { managerAccount: PublicKey; authId: bigint }[] = [];
    
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && miner.rewardsSol > BigInt(0)) {
        claims.push({
          managerAccount: manager.address,
          authId: BigInt(0),
        });
      }
    }
    
    if (claims.length === 0) return;
    await bulkClaimSol(claims);
  };

  const handleBulkClaimOre = async () => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    const claims: { managerAccount: PublicKey; authId: bigint }[] = [];
    
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && miner.rewardsOre > BigInt(0)) {
        claims.push({
          managerAccount: manager.address,
          authId: BigInt(0),
        });
      }
    }
    
    if (claims.length === 0) return;
    await bulkClaimOre(claims);
  };

  const handleBulkUpdate = async (deployAuthority: PublicKey, bpsFee: bigint, flatFee: bigint, maxPerRound: bigint) => {
    const selected = managers.filter(m => selectedManagers.has(m.address.toBase58()));
    // Filter to only managers that have deployers
    const managersWithDeployers = selected.filter(m => getDeployerForManager(m.address));
    if (managersWithDeployers.length === 0) return;
    
    // Batch updates - will auto-split into multiple transactions if needed
    await bulkUpdateDeployers(
      managersWithDeployers.map(m => m.address),
      deployAuthority,
      bpsFee,
      flatFee,
      BigInt(0), // expected_bps_fee
      BigInt(0), // expected_flat_fee
      maxPerRound
    );
  };

  const handleCreateManager = async (keypair: Keypair): Promise<string> => {
    // Pass the full keypair so it can sign the transaction
    return await createManager(keypair);
  };

  return (
    <div className="min-h-screen bg-zinc-950">
      <Header />

      <main className="max-w-6xl mx-auto px-4 py-8">
        <div className="mb-6">
          <h1 className="text-2xl font-bold">Advanced Management</h1>
          <p className="text-zinc-400 text-sm">Full control over managers, deployers, and miners</p>
        </div>

        {!connected ? (
          <div className="text-center py-20">
            <h2 className="text-2xl font-bold mb-4">Welcome to Evore</h2>
            <p className="text-zinc-400 mb-8">
              Connect your wallet to manage autodeploys
            </p>
            <div className="inline-block">
              <p className="text-sm text-zinc-500">
                Click the button in the header to connect
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-8">
            {/* Stats Overview */}
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Manager Accounts</p>
                <p className="text-2xl font-bold">{managers.length}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Active Deployers</p>
                <p className="text-2xl font-bold">{deployers.length}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-sm text-zinc-400">Total Autodeploy Balance</p>
                <p className="text-2xl font-bold text-yellow-400">
                  {(
                    Number(
                      deployers.reduce((sum, d) => sum + d.autodeployBalance, BigInt(0))
                    ) / 1_000_000_000
                  ).toFixed(4)}{" "}
                  SOL
                </p>
              </div>
            </div>

            {/* Loading State */}
            {loading && (
              <div className="text-center py-8">
                <p className="text-zinc-400">Loading...</p>
              </div>
            )}

            {/* Manager Cards */}
            <div className="space-y-4">
              <h2 className="text-xl font-semibold">Your Managers</h2>
              
              {/* Bulk Action Bar */}
              {sortedManagers.length > 0 && (
                <BulkActionBar
                  selectedCount={selectedManagers.size}
                  totalCount={sortedManagers.length}
                  onSelectAll={selectAll}
                  onDeselectAll={deselectAll}
                  onBulkDeposit={handleBulkDeposit}
                  onBulkWithdraw={handleBulkWithdraw}
                  onBulkCheckpoint={handleBulkCheckpoint}
                  onBulkClaimSol={handleBulkClaimSol}
                  onBulkClaimOre={handleBulkClaimOre}
                  onBulkUpdate={handleBulkUpdate}
                />
              )}
              
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                {sortedManagers.map((manager) => {
                  const deployer = getDeployerForManager(manager.address);
                  const miner = getMinerForManager(manager.address);
                  const managerKey = manager.address.toBase58();
                  
                  return (
                    <ManagerCard
                      key={managerKey}
                      managerAddress={manager.address}
                      deployer={
                        deployer
                          ? {
                              address: deployer.address,
                              deployAuthority: deployer.data.deployAuthority,
                              bpsFee: deployer.data.bpsFee,
                              flatFee: deployer.data.flatFee,
                              maxPerRound: deployer.data.maxPerRound,
                              autodeployBalance: deployer.autodeployBalance,
                              authPdaAddress: deployer.authPdaAddress,
                            }
                          : undefined
                      }
                      miner={miner}
                      currentBoardRoundId={board?.roundId}
                      isSelected={selectedManagers.has(managerKey)}
                      onToggleSelect={() => toggleSelection(managerKey)}
                      onCreateDeployer={(deployAuthority, bpsFee, flatFee, maxPerRound) =>
                        createDeployer(manager.address, deployAuthority, bpsFee, flatFee, maxPerRound)
                      }
                      onUpdateDeployer={(newDeployAuthority, newBpsFee, newFlatFee, newMaxPerRound) =>
                        updateDeployer(manager.address, newDeployAuthority, newBpsFee, newFlatFee, BigInt(0), BigInt(0), newMaxPerRound)
                      }
                      onDeposit={(authId, amount) =>
                        depositAutodeployBalance(manager.address, authId, amount)
                      }
                      onWithdraw={(authId, amount) =>
                        withdrawAutodeployBalance(manager.address, authId, amount)
                      }
                      onCheckpoint={(roundId) =>
                        checkpoint(manager.address, roundId)
                      }
                      onClaimSol={() =>
                        claimSol(manager.address)
                      }
                      onClaimOre={() =>
                        claimOre(manager.address)
                      }
                      onTransfer={(newAuthority) =>
                        transferManager(manager.address, newAuthority)
                      }
                    />
                  );
                })}

                {/* Create New Manager */}
                <CreateManagerForm onCreateManager={handleCreateManager} />
              </div>
            </div>

            {/* Info Section */}
            <div className="bg-zinc-900/50 border border-zinc-800 rounded-lg p-6">
              <h3 className="text-lg font-semibold mb-4">How Autodeploy Works</h3>
              <div className="grid grid-cols-1 md:grid-cols-3 gap-6 text-sm">
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">1. Create Manager</h4>
                  <p className="text-zinc-400">
                    A manager account holds your authority over managed miners. 
                    Create one to get started.
                  </p>
                </div>
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">2. Set Up Deployer</h4>
                  <p className="text-zinc-400">
                    Configure a deployer with a deploy authority (can be a crank service) 
                    and fee percentage.
                  </p>
                </div>
                <div>
                  <h4 className="font-medium text-purple-400 mb-2">3. Fund & Deploy</h4>
                  <p className="text-zinc-400">
                    Deposit SOL to your autodeploy balance. The crank will automatically 
                    deploy when configured.
                  </p>
                </div>
              </div>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

