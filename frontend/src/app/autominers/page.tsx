"use client";

import { useState } from "react";
import Link from "next/link";
import { useWallet } from "@solana/wallet-adapter-react";
import { PublicKey } from "@solana/web3.js";
import { AutoMinerCard } from "@/components/AutoMinerCard";
import { BulkActionBar } from "@/components/BulkActionBar";
import { Header } from "@/components/Header";
import { useEvore } from "@/hooks/useEvore";
import { formatSol, formatOre } from "@/lib/accounts";
import { DEFAULT_DEPLOYER_PUBKEY, DEFAULT_DEPLOYER_BPS_FEE, DEFAULT_DEPLOYER_FLAT_FEE } from "@/lib/constants";

export default function AutoMinersPage() {
  const { connected } = useWallet();
  const {
    managers,
    deployers,
    miners,
    board,
    walletBalance,
    loading,
    createAutoMiner,
    bulkCreateAutoMiners,
    depositAutodeployBalance,
    bulkDepositAutodeployBalance,
    claimSol,
    bulkClaimSol,
    checkpoint,
    bulkCheckpoint,
    claimOre,
    bulkClaimOre,
    transferManager,
  } = useEvore();

  const [creating, setCreating] = useState(false);
  const [createCount, setCreateCount] = useState(1);
  const [createProgress, setCreateProgress] = useState<{ completed: number; total: number } | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  const [selectedManagers, setSelectedManagers] = useState<Set<string>>(new Set());

  // Find deployer for a manager
  const getDeployerForManager = (managerAddress: PublicKey) => {
    return deployers.find(
      (d) => d.data.managerKey.toBase58() === managerAddress.toBase58()
    );
  };

  // Find the first miner for a manager (miners are keyed by "manager-authId")
  const getMinerForManager = (managerAddress: PublicKey) => {
    const prefix = managerAddress.toBase58();
    // Find any miner key that starts with this manager address
    for (const [key, miner] of Array.from(miners.entries())) {
      if (key.startsWith(prefix + "-")) {
        return miner;
      }
    }
    return undefined;
  };

  // Get all miners for a manager
  const getAllMinersForManager = (managerAddress: PublicKey) => {
    const prefix = managerAddress.toBase58();
    const result: typeof miners extends Map<string, infer V> ? V[] : never[] = [];
    for (const [key, miner] of Array.from(miners.entries())) {
      if (key.startsWith(prefix + "-")) {
        result.push(miner);
      }
    }
    return result;
  };

  // Calculate totals
  const totalAutodeployBalance = deployers.reduce((sum, d) => sum + d.autodeployBalance, BigInt(0));
  const totalClaimableOre = Array.from(miners.values()).reduce(
    (sum, m) => sum + m.rewardsOre + m.refinedOre,
    BigInt(0)
  );

  // Get managers with deployers (for selection), sorted by claimable ORE (descending)
  const managersWithDeployers = managers
    .filter(m => deployers.some(d => d.data.managerKey.toBase58() === m.address.toBase58()))
    .sort((a, b) => {
      const minerA = getMinerForManager(a.address);
      const minerB = getMinerForManager(b.address);
      const oreA = (minerA?.rewardsOre || BigInt(0)) + (minerA?.refinedOre || BigInt(0));
      const oreB = (minerB?.rewardsOre || BigInt(0)) + (minerB?.refinedOre || BigInt(0));
      if (oreB > oreA) return 1;
      if (oreB < oreA) return -1;
      return 0;
    });

  const MIN_AUTODEPLOY_BALANCE = BigInt(100_000);

  const managersWithLowBalance = managersWithDeployers.filter(m => {
    const deployer = getDeployerForManager(m.address);
    return deployer && deployer.autodeployBalance < MIN_AUTODEPLOY_BALANCE;
  });

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

  const selectAll = () => {
    setSelectedManagers(new Set(managersWithDeployers.map(m => m.address.toBase58())));
  };

  const deselectAll = () => {
    setSelectedManagers(new Set());
  };

  const selectLowBalance = () => {
    setSelectedManagers(new Set(managersWithLowBalance.map(m => m.address.toBase58())));
  };

  const handleBulkDeposit = async (authId: bigint, amount: bigint) => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    if (selected.length === 0) return;
    
    await bulkDepositAutodeployBalance(
      selected.map(m => m.address),
      authId,
      amount
    );
  };

  const handleBulkClaimSol = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    const claims: { managerAccount: PublicKey; authId: bigint }[] = [];
    
    for (const manager of selected) {
      const deployer = getDeployerForManager(manager.address);
      const miner = getMinerForManager(manager.address);
      const hasBalance = deployer && deployer.autodeployBalance > BigInt(0);
      const hasRewards = miner && miner.rewardsSol > BigInt(0);
      if (hasBalance || hasRewards) {
        claims.push({
          managerAccount: manager.address,
          authId: BigInt(0),
        });
      }
    }
    
    if (claims.length === 0) return;
    await bulkClaimSol(claims);
  };

  const handleBulkCheckpoint = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
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

  const handleBulkClaimOre = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    const claims: { managerAccount: PublicKey; authId: bigint }[] = [];
    
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && (miner.rewardsOre > BigInt(0) || miner.refinedOre > BigInt(0))) {
        claims.push({
          managerAccount: manager.address,
          authId: BigInt(0),
        });
      }
    }
    
    if (claims.length === 0) return;
    await bulkClaimOre(claims);
  };

  const handleCreateAutoMiner = async () => {
    if (!DEFAULT_DEPLOYER_PUBKEY) {
      setCreateError("Deployer pubkey not configured. Set NEXT_PUBLIC_DEPLOYER_PUBKEY in .env");
      return;
    }

    try {
      setCreating(true);
      setCreateError(null);
      setCreateProgress(null);
      const deployAuthority = new PublicKey(DEFAULT_DEPLOYER_PUBKEY);
      
      if (createCount === 1) {
        await createAutoMiner(deployAuthority, BigInt(DEFAULT_DEPLOYER_BPS_FEE), BigInt(DEFAULT_DEPLOYER_FLAT_FEE));
      } else {
        await bulkCreateAutoMiners(
          createCount,
          deployAuthority,
          BigInt(DEFAULT_DEPLOYER_BPS_FEE),
          BigInt(DEFAULT_DEPLOYER_FLAT_FEE),
          BigInt(1_000_000_000),
          (completed, total) => setCreateProgress({ completed, total })
        );
      }
    } catch (err: any) {
      setCreateError(err.message);
    } finally {
      setCreating(false);
      setCreateProgress(null);
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />

      <main className="max-w-5xl mx-auto px-4 py-8">
        {!connected ? (
          <div className="text-center py-20">
            <h1 className="text-4xl font-bold mb-4 bg-gradient-to-r from-amber-400 to-orange-500 bg-clip-text text-transparent">
              Evore AutoMiner
            </h1>
            <p className="text-slate-400 mb-8 max-w-md mx-auto">
              Automated ORE mining with managed deployments. 
              Connect your wallet to get started.
            </p>
            <div className="inline-block">
              <p className="text-sm text-slate-500">
                Click the wallet button to connect
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-6">
            {/* Page Header with Sub-navigation */}
            <div className="flex items-center justify-between">
              <h1 className="text-2xl font-bold text-white">AutoMiners</h1>
              <Link
                href="/manage"
                className="px-3 py-1.5 text-sm bg-slate-800 hover:bg-slate-700 text-slate-300 hover:text-white rounded-lg transition-colors border border-slate-700 flex items-center gap-2"
              >
                <span>⚙️</span>
                Advanced Management
              </Link>
            </div>

            {/* Wallet Stats */}
            <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
                <p className="text-xs text-slate-400">Current Round</p>
                <p className="text-xl font-bold font-mono text-white">{board?.roundId?.toString() || "-"}</p>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
                <p className="text-xs text-slate-400">Wallet Balance</p>
                <p className="text-xl font-bold text-white">{formatSol(walletBalance)} SOL</p>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
                <p className="text-xs text-slate-400">AutoMiners</p>
                <p className="text-xl font-bold text-white">{managers.length}</p>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
                <p className="text-xs text-slate-400">Total Autodeploy</p>
                <p className="text-xl font-bold text-amber-400">{formatSol(totalAutodeployBalance)} SOL</p>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
                <p className="text-xs text-slate-400">Total Claimable ORE</p>
                <p className="text-xl font-bold text-orange-400">{formatOre(totalClaimableOre)}</p>
              </div>
            </div>

            {/* Loading */}
            {loading && (
              <div className="flex items-center justify-center py-4">
                <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
              </div>
            )}

            {/* AutoMiner Cards */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold text-white">Your AutoMiners</h2>
                <Link 
                  href="/manage" 
                  className="text-sm text-amber-400 hover:text-amber-300 transition-colors"
                >
                  Advanced Management →
                </Link>
              </div>

              {/* Bulk Action Bar */}
              {managersWithDeployers.length > 0 && (
                <BulkActionBar
                  selectedCount={selectedManagers.size}
                  totalCount={managersWithDeployers.length}
                  lowBalanceCount={managersWithLowBalance.length}
                  onSelectAll={selectAll}
                  onDeselectAll={deselectAll}
                  onSelectLowBalance={selectLowBalance}
                  onBulkDeposit={handleBulkDeposit}
                  onBulkWithdraw={handleBulkClaimSol}
                  onBulkCheckpoint={handleBulkCheckpoint}
                  onBulkClaimOre={handleBulkClaimOre}
                  totalAutodeployBalance={totalAutodeployBalance}
                />
              )}

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {managersWithDeployers.map((manager) => {
                  const deployer = getDeployerForManager(manager.address);
                  const miner = getMinerForManager(manager.address);
                  const managerKey = manager.address.toBase58();
                  
                  if (!deployer) return null;

                  return (
                    <AutoMinerCard
                      key={managerKey}
                      managerAddress={manager.address}
                      deployer={{
                        deployAuthority: deployer.data.deployAuthority,
                        bpsFee: deployer.data.bpsFee,
                        flatFee: deployer.data.flatFee,
                        maxPerRound: deployer.data.maxPerRound,
                        autodeployBalance: deployer.autodeployBalance,
                      }}
                      miner={miner}
                      currentBoardRoundId={board?.roundId}
                      isSelected={selectedManagers.has(managerKey)}
                      onToggleSelect={() => toggleSelection(managerKey)}
                      onDeposit={(authId, amount) => depositAutodeployBalance(manager.address, authId, amount)}
                      onClaimSol={() => claimSol(manager.address)}
                      onCheckpoint={(roundId) => checkpoint(manager.address, roundId)}
                      onClaimOre={() => claimOre(manager.address)}
                      onTransfer={(newAuthority) => transferManager(manager.address, newAuthority)}
                    />
                  );
                })}

                {/* Create AutoMiner Card */}
                <div className="bg-slate-800/30 border border-dashed border-slate-600 rounded-xl p-6 flex flex-col items-center justify-center min-h-[200px]">
                  <p className="text-slate-400 mb-4 text-center">
                    Create AutoMiners to start automated ORE mining
                  </p>
                  {createError && (
                    <p className="text-red-400 text-sm mb-4 text-center">{createError}</p>
                  )}
                  {createProgress && (
                    <p className="text-amber-400 text-sm mb-4 text-center">
                      Creating {createProgress.completed} of {createProgress.total}...
                    </p>
                  )}
                  <div className="flex items-center gap-3 mb-4">
                    <label className="text-sm text-slate-400">Count:</label>
                    <input
                      type="number"
                      min="1"
                      max="50"
                      value={createCount}
                      onChange={(e) => setCreateCount(Math.max(1, Math.min(50, parseInt(e.target.value) || 1)))}
                      disabled={creating}
                      className="w-20 px-2 py-1 bg-slate-900 border border-slate-700 rounded-lg text-center text-sm text-white"
                    />
                  </div>
                  <button
                    onClick={handleCreateAutoMiner}
                    disabled={creating || !DEFAULT_DEPLOYER_PUBKEY}
                    className={`px-6 py-3 rounded-lg font-medium transition-colors ${
                      creating || !DEFAULT_DEPLOYER_PUBKEY
                        ? 'bg-slate-700 text-slate-500 cursor-not-allowed'
                        : 'bg-amber-500 hover:bg-amber-400 text-black'
                    }`}
                  >
                    {creating 
                      ? (createProgress ? `Creating ${createProgress.completed}/${createProgress.total}...` : "Creating...") 
                      : `+ Create ${createCount > 1 ? `${createCount} AutoMiners` : "AutoMiner"}`}
                  </button>
                  {!DEFAULT_DEPLOYER_PUBKEY && (
                    <p className="text-xs text-slate-500 mt-2">
                      Configure NEXT_PUBLIC_DEPLOYER_PUBKEY to enable
                    </p>
                  )}
                </div>
              </div>
            </div>

            {/* Info */}
            <div className="bg-slate-800/30 border border-slate-700/50 rounded-xl p-6 text-sm text-slate-400">
              <p className="font-medium text-white mb-3">How it works:</p>
              <ol className="list-decimal list-inside space-y-2">
                <li>Create an AutoMiner (one-click setup)</li>
                <li>Deposit SOL to fund automated deployments</li>
                <li>The crank service automatically deploys each round</li>
                <li>Claim your ORE rewards or withdraw your SOL anytime</li>
              </ol>
            </div>
          </div>
        )}
      </main>
    </div>
  );
}

