"use client";

import { useState } from "react";
import Link from "next/link";
import { useWallet } from "@solana/wallet-adapter-react";
import { PublicKey } from "@solana/web3.js";
import { Header } from "@/components/Header";
import { AutoMinerCard } from "@/components/AutoMinerCard";
import { BulkActionBar } from "@/components/BulkActionBar";
import { useEvore } from "@/hooks/useEvore";
import { formatSol, formatOre } from "@/lib/accounts";
import { DEFAULT_DEPLOYER_PUBKEY, DEFAULT_DEPLOYER_BPS_FEE, DEFAULT_DEPLOYER_FLAT_FEE } from "@/lib/constants";

export default function Home() {
  const { connected } = useWallet();
  const {
    managers,
    deployers,
    miners,
    board,
    walletBalance,
    loading,
    createAutoMiner,
    depositAutodeployBalance,
    withdrawAll,
    checkpoint,
    claimOre,
  } = useEvore();

  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
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

  // Calculate totals
  const totalAutodeployBalance = deployers.reduce((sum, d) => sum + d.autodeployBalance, BigInt(0));
  const totalClaimableOre = Array.from(miners.values()).reduce(
    (sum, m) => sum + m.rewardsOre + m.refinedOre,
    BigInt(0)
  );

  // Get managers with deployers (for selection)
  const managersWithDeployers = managers.filter(m => 
    deployers.some(d => d.data.managerKey.toBase58() === m.address.toBase58())
  );

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

  const selectAll = () => {
    setSelectedManagers(new Set(managersWithDeployers.map(m => m.address.toBase58())));
  };

  const deselectAll = () => {
    setSelectedManagers(new Set());
  };

  // Bulk action handlers
  const handleBulkDeposit = async (authId: bigint, amount: bigint) => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    for (const manager of selected) {
      await depositAutodeployBalance(manager.address, authId, amount);
    }
  };

  const handleBulkWithdraw = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    for (const manager of selected) {
      const deployer = getDeployerForManager(manager.address);
      const miner = getMinerForManager(manager.address);
      if (deployer) {
        await withdrawAll(
          manager.address, 
          BigInt(0), 
          miner?.rewardsSol || BigInt(0), 
          deployer.autodeployBalance
        );
      }
    }
  };

  const handleBulkCheckpoint = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && board?.roundId && miner.checkpointId < miner.roundId && miner.roundId < board.roundId) {
        await checkpoint(manager.address, miner.roundId);
      }
    }
  };

  const handleBulkClaimOre = async () => {
    const selected = managersWithDeployers.filter(m => selectedManagers.has(m.address.toBase58()));
    for (const manager of selected) {
      const miner = getMinerForManager(manager.address);
      if (miner && (miner.rewardsOre > BigInt(0) || miner.refinedOre > BigInt(0))) {
        await claimOre(manager.address);
      }
    }
  };

  // Handle create autominer
  const handleCreateAutoMiner = async () => {
    if (!DEFAULT_DEPLOYER_PUBKEY) {
      setCreateError("Deployer pubkey not configured. Set NEXT_PUBLIC_DEPLOYER_PUBKEY in .env");
      return;
    }

    try {
      setCreating(true);
      setCreateError(null);
      const deployAuthority = new PublicKey(DEFAULT_DEPLOYER_PUBKEY);
      await createAutoMiner(deployAuthority, BigInt(DEFAULT_DEPLOYER_BPS_FEE), BigInt(DEFAULT_DEPLOYER_FLAT_FEE));
    } catch (err: any) {
      setCreateError(err.message);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="min-h-screen bg-zinc-950">
      <Header />

      <main className="max-w-4xl mx-auto px-4 py-8">
        {!connected ? (
          <div className="text-center py-20">
            <h1 className="text-4xl font-bold mb-4">Evore AutoMiner</h1>
            <p className="text-zinc-400 mb-8 max-w-md mx-auto">
              Automated ORE mining with managed deployments. 
              Connect your wallet to get started.
            </p>
            <div className="inline-block">
              <p className="text-sm text-zinc-500">
                Click the wallet button to connect
              </p>
            </div>
          </div>
        ) : (
          <div className="space-y-6">
            {/* Wallet Stats */}
            <div className="grid grid-cols-2 md:grid-cols-5 gap-4">
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-xs text-zinc-500">Current Round</p>
                <p className="text-xl font-bold font-mono">{board?.roundId?.toString() || "-"}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-xs text-zinc-500">Wallet Balance</p>
                <p className="text-xl font-bold">{formatSol(walletBalance)} SOL</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-xs text-zinc-500">AutoMiners</p>
                <p className="text-xl font-bold">{managers.length}</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-xs text-zinc-500">Total Autodeploy</p>
                <p className="text-xl font-bold text-yellow-400">{formatSol(totalAutodeployBalance)} SOL</p>
              </div>
              <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
                <p className="text-xs text-zinc-500">Total Claimable ORE</p>
                <p className="text-xl font-bold text-orange-400">{formatOre(totalClaimableOre)}</p>
              </div>
            </div>

            {/* Loading */}
            {loading && (
              <div className="text-center py-4">
                <p className="text-zinc-400">Loading...</p>
              </div>
            )}

            {/* AutoMiner Cards */}
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <h2 className="text-lg font-semibold">Your AutoMiners</h2>
                <Link 
                  href="/manage" 
                  className="text-sm text-purple-400 hover:text-purple-300"
                >
                  Advanced Management →
                </Link>
              </div>

              {/* Bulk Action Bar */}
              {managersWithDeployers.length > 0 && (
                <BulkActionBar
                  selectedCount={selectedManagers.size}
                  totalCount={managersWithDeployers.length}
                  onSelectAll={selectAll}
                  onDeselectAll={deselectAll}
                  onBulkDeposit={handleBulkDeposit}
                  onBulkWithdraw={handleBulkWithdraw}
                  onBulkCheckpoint={handleBulkCheckpoint}
                  onBulkClaimOre={handleBulkClaimOre}
                  totalAutodeployBalance={totalAutodeployBalance}
                />
              )}

              <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
                {managers.map((manager) => {
                  const deployer = getDeployerForManager(manager.address);
                  const miner = getMinerForManager(manager.address);
                  const managerKey = manager.address.toBase58();
                  
                  // Only show managers that have deployers (fully set up)
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
                      onWithdraw={(authId, rewardsSol, autodeployBalance) => 
                        withdrawAll(manager.address, authId, rewardsSol, autodeployBalance)
                      }
                      onCheckpoint={(roundId) => checkpoint(manager.address, roundId)}
                      onClaimOre={() => claimOre(manager.address)}
                    />
                  );
                })}

                {/* Create AutoMiner Card */}
                <div className="bg-zinc-900/50 border border-dashed border-zinc-700 rounded-lg p-6 flex flex-col items-center justify-center min-h-[200px]">
                  <p className="text-zinc-500 mb-4 text-center">
                    Create a new AutoMiner to start automated ORE mining
                  </p>
                  {createError && (
                    <p className="text-red-400 text-sm mb-4 text-center">{createError}</p>
                  )}
                  <button
                    onClick={handleCreateAutoMiner}
                    disabled={creating || !DEFAULT_DEPLOYER_PUBKEY}
                    className={`px-6 py-3 rounded-lg font-medium ${
                      creating || !DEFAULT_DEPLOYER_PUBKEY
                        ? 'bg-zinc-700 text-zinc-500 cursor-not-allowed'
                        : 'bg-purple-600 hover:bg-purple-500'
                    }`}
                  >
                    {creating ? "Creating..." : "+ Create AutoMiner"}
                  </button>
                  {!DEFAULT_DEPLOYER_PUBKEY && (
                    <p className="text-xs text-zinc-500 mt-2">
                      Configure NEXT_PUBLIC_DEPLOYER_PUBKEY to enable
                    </p>
                  )}
                </div>
              </div>
            </div>

            {/* Info */}
            <div className="bg-zinc-900/30 border border-zinc-800 rounded-lg p-4 text-sm text-zinc-400">
              <p className="font-medium text-zinc-300 mb-2">How it works:</p>
              <ol className="list-decimal list-inside space-y-1">
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
