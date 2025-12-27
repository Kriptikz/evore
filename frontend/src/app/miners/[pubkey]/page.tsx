"use client";

import { useState, useEffect, useCallback, useMemo } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { api, MinerStats, HistoricalDeployment, CursorResponse } from "@/lib/api";
import { Header } from "@/components/Header";

const LAMPORTS_PER_SOL = 1_000_000_000;
const ORE_DECIMALS = 11;

// Grouped deployment for a single round
interface RoundDeploymentGroup {
  round_id: number;
  amounts: number[];  // 25 squares
  total_amount: number;
  sol_earned: number;
  ore_earned: number;
  is_winner: boolean;
  is_top_miner: boolean;
  deployed_slot: number;  // First slot for this round
}

function formatSol(lamports: number): string {
  const sol = lamports / LAMPORTS_PER_SOL;
  if (Math.abs(sol) >= 1000) {
    return sol.toLocaleString(undefined, { maximumFractionDigits: 2 }) + " SOL";
  }
  return sol.toFixed(6) + " SOL";
}

function formatOre(atomic: number): string {
  const ore = atomic / Math.pow(10, ORE_DECIMALS);
  return ore.toFixed(4) + " ORE";
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return addr.slice(0, 6) + "..." + addr.slice(-4);
}

type TabType = "overview" | "deployments" | "wins";

function MinerDeploymentsGrouped({
  deployments,
  loadingDeployments,
  hasMore,
  onLoadMore,
}: {
  deployments: HistoricalDeployment[];
  loadingDeployments: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  // Group deployments by round
  const groupedByRound = useMemo(() => {
    const groups: Map<number, RoundDeploymentGroup> = new Map();
    
    for (const d of deployments) {
      if (!groups.has(d.round_id)) {
        groups.set(d.round_id, {
          round_id: d.round_id,
          amounts: new Array(25).fill(0),
          total_amount: 0,
          sol_earned: 0,
          ore_earned: 0,
          is_winner: false,
          is_top_miner: false,
          deployed_slot: d.deployed_slot,
        });
      }
      
      const group = groups.get(d.round_id)!;
      group.amounts[d.square_id] += d.amount;
      group.total_amount += d.amount;
      group.sol_earned += d.sol_earned;
      group.ore_earned += d.ore_earned;
      if (d.is_winner) group.is_winner = true;
      if (d.is_top_miner) group.is_top_miner = true;
      // Use earliest slot
      if (d.deployed_slot < group.deployed_slot && d.deployed_slot > 0) {
        group.deployed_slot = d.deployed_slot;
      }
    }
    
    // Sort by round_id descending (newest first)
    return Array.from(groups.values()).sort((a, b) => b.round_id - a.round_id);
  }, [deployments]);

  if (deployments.length === 0 && !loadingDeployments) {
    return (
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-8 text-center text-slate-400">
        No deployment history found
      </div>
    );
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white">
          Deployment History ({groupedByRound.length} rounds)
        </h3>
      </div>
      
      <div className="max-h-[600px] overflow-y-auto divide-y divide-slate-700/30">
        {groupedByRound.map((group) => {
          const deployedSquares = group.amounts
            .map((amt, idx) => amt > 0 ? idx : -1)
            .filter(idx => idx >= 0);
          
          return (
            <div
              key={group.round_id}
              className="px-6 py-4 hover:bg-slate-700/20 transition-colors"
            >
              {/* Round Header */}
              <div className="flex items-center justify-between mb-3">
                <div className="flex items-center gap-3">
                  <Link
                    href={`/?round=${group.round_id}`}
                    className="text-lg font-bold text-amber-400 hover:text-amber-300 font-mono"
                  >
                    Round #{group.round_id}
                  </Link>
                  {group.is_top_miner && (
                    <span className="px-2 py-1 bg-yellow-500/20 text-yellow-400 text-xs rounded-full">
                      👑 Top Miner
                    </span>
                  )}
                  {group.is_winner && !group.is_top_miner && (
                    <span className="px-2 py-1 bg-green-500/20 text-green-400 text-xs rounded-full">
                      Winner
                    </span>
                  )}
                </div>
                <div className="flex items-center gap-4 text-sm">
                  <span className="font-mono text-white">{formatSol(group.total_amount)}</span>
                  {group.sol_earned > 0 && (
                    <span className="text-green-400 font-mono">+{formatSol(group.sol_earned)}</span>
                  )}
                  {group.ore_earned > 0 && (
                    <span className="text-amber-400 font-mono">+{formatOre(group.ore_earned)}</span>
                  )}
                </div>
              </div>
              
              {/* Squares Grid */}
              <div className="flex flex-wrap gap-1.5">
                {deployedSquares.map((squareId) => (
                  <span
                    key={squareId}
                    className={`px-2 py-1 rounded text-xs ${
                      group.is_winner && group.amounts[squareId] > 0
                        ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/50"
                        : "bg-slate-700 text-slate-400"
                    }`}
                    title={`Square ${squareId + 1}: ${formatSol(group.amounts[squareId])}`}
                  >
                    ◼{squareId + 1}: {formatSol(group.amounts[squareId])}
                  </span>
                ))}
              </div>
            </div>
          );
        })}
      </div>

      {/* Load More */}
      {hasMore && (
        <div className="p-4 border-t border-slate-700/50 text-center">
          <button
            onClick={onLoadMore}
            disabled={loadingDeployments}
            className={`px-6 py-2 rounded-lg transition-colors ${
              loadingDeployments
                ? "bg-slate-700 text-slate-500 cursor-not-allowed"
                : "bg-amber-500 hover:bg-amber-600 text-black font-medium"
            }`}
          >
            {loadingDeployments ? "Loading..." : "Load More Rounds"}
          </button>
        </div>
      )}
      
      {loadingDeployments && groupedByRound.length === 0 && (
        <div className="flex items-center justify-center p-8">
          <div className="w-6 h-6 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      )}
    </div>
  );
}

export default function MinerProfilePage() {
  const params = useParams();
  const pubkey = params.pubkey as string;

  const [stats, setStats] = useState<MinerStats | null>(null);
  const [deployments, setDeployments] = useState<HistoricalDeployment[]>([]);
  const [deploymentsResponse, setDeploymentsResponse] = useState<CursorResponse<HistoricalDeployment> | null>(null);
  const [loadingStats, setLoadingStats] = useState(true);
  const [loadingDeployments, setLoadingDeployments] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<TabType>("overview");
  const [copiedAddress, setCopiedAddress] = useState(false);

  const fetchStats = useCallback(async () => {
    setLoadingStats(true);
    setError(null);
    try {
      const data = await api.getMinerStats(pubkey);
      setStats(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load miner stats");
    } finally {
      setLoadingStats(false);
    }
  }, [pubkey]);

  const fetchDeployments = useCallback(async (cursor?: string, winnerOnly?: boolean) => {
    setLoadingDeployments(true);
    try {
      const data = await api.getMinerDeployments(pubkey, {
        cursor,
        limit: 50,
        winnerOnly,
      });
      
      if (cursor) {
        setDeployments(prev => [...prev, ...data.data]);
      } else {
        setDeployments(data.data);
      }
      setDeploymentsResponse(data);
    } catch (err) {
      console.error("Failed to load deployments:", err);
    } finally {
      setLoadingDeployments(false);
    }
  }, [pubkey]);

  useEffect(() => {
    fetchStats();
  }, [fetchStats]);

  useEffect(() => {
    if (activeTab === "deployments") {
      fetchDeployments(undefined, false);
    } else if (activeTab === "wins") {
      fetchDeployments(undefined, true);
    }
  }, [activeTab, fetchDeployments]);

  const handleCopyAddress = () => {
    navigator.clipboard.writeText(pubkey);
    setCopiedAddress(true);
    setTimeout(() => setCopiedAddress(false), 2000);
  };

  const loadMoreDeployments = () => {
    if (deploymentsResponse?.cursor) {
      fetchDeployments(deploymentsResponse.cursor, activeTab === "wins");
    }
  };

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />

      <main className="max-w-7xl mx-auto px-4 py-8">
        {/* Address Header */}
        <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8">
          <div className="flex items-center justify-between">
            <div>
              <div className="text-sm text-slate-400 mb-1">Miner Address</div>
              <div className="flex items-center gap-3">
                <code className="text-xl font-mono text-white break-all">{pubkey}</code>
                <button
                  onClick={handleCopyAddress}
                  className="px-3 py-1 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                >
                  {copiedAddress ? "Copied!" : "Copy"}
                </button>
              </div>
            </div>
            <a
              href={`https://solscan.io/account/${pubkey}`}
              target="_blank"
              rel="noopener noreferrer"
              className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
            >
              View on Solscan ↗
            </a>
          </div>
        </div>

        {/* Loading State */}
        {loadingStats && (
          <div className="flex items-center justify-center h-64">
            <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
          </div>
        )}

        {/* Error State */}
        {error && (
          <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-6 text-center">
            <div className="text-red-400 mb-2">Error loading miner profile</div>
            <div className="text-slate-400">{error}</div>
            <button
              onClick={fetchStats}
              className="mt-4 px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition-colors"
            >
              Retry
            </button>
          </div>
        )}

        {/* Stats Grid */}
        {!loadingStats && !error && stats && (
          <>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Net SOL</div>
                <div className={`text-2xl font-bold ${stats.net_sol_change >= 0 ? "text-green-400" : "text-red-400"}`}>
                  {stats.net_sol_change >= 0 ? "+" : ""}{formatSol(stats.net_sol_change)}
                </div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">SOL Earned</div>
                <div className="text-2xl font-bold text-white">{formatSol(stats.total_sol_earned)}</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">ORE Earned</div>
                <div className="text-2xl font-bold text-amber-400">{formatOre(stats.total_ore_earned)}</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Total Deployed</div>
                <div className="text-2xl font-bold text-white">{formatSol(stats.total_deployed)}</div>
              </div>
            </div>

            <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Rounds Played</div>
                <div className="text-2xl font-bold text-white">{stats.rounds_played.toLocaleString()}</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Rounds Won</div>
                <div className="text-2xl font-bold text-green-400">{stats.rounds_won.toLocaleString()}</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Win Rate</div>
                <div className="text-2xl font-bold text-white">{stats.win_rate.toFixed(1)}%</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Avg Deployment</div>
                <div className="text-2xl font-bold text-white">{formatSol(stats.avg_deployment)}</div>
              </div>
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <div className="text-slate-400 text-sm mb-1">Avg Slots Left</div>
                <div className="text-2xl font-bold text-cyan-400" title="Average slots remaining when deploying">
                  {stats.avg_slots_left.toFixed(1)}
                </div>
              </div>
            </div>

            {/* Tabs */}
            <div className="flex gap-2 mb-6">
              {(["overview", "deployments", "wins"] as TabType[]).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setActiveTab(tab)}
                  className={`px-4 py-2 rounded-lg transition-colors capitalize ${
                    activeTab === tab
                      ? "bg-amber-500 text-black font-medium"
                      : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                  }`}
                >
                  {tab === "wins" ? "Winning Rounds" : tab}
                </button>
              ))}
            </div>

            {/* Tab Content */}
            {activeTab === "overview" && (
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-8">
                <h3 className="text-lg font-semibold text-white mb-4">Performance Summary</h3>
                <div className="grid md:grid-cols-2 gap-6">
                  <div>
                    <h4 className="text-slate-400 text-sm mb-2">Profitability</h4>
                    <div className="space-y-2">
                      <div className="flex justify-between">
                        <span className="text-slate-400">Total Deployed:</span>
                        <span className="text-white font-mono">{formatSol(stats.total_deployed)}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-slate-400">Total SOL Earned:</span>
                        <span className="text-green-400 font-mono">{formatSol(stats.total_sol_earned)}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-slate-400">Net P/L:</span>
                        <span className={`font-mono font-bold ${stats.net_sol_change >= 0 ? "text-green-400" : "text-red-400"}`}>
                          {stats.net_sol_change >= 0 ? "+" : ""}{formatSol(stats.net_sol_change)}
                        </span>
                      </div>
                    </div>
                  </div>
                  <div>
                    <h4 className="text-slate-400 text-sm mb-2">Activity</h4>
                    <div className="space-y-2">
                      <div className="flex justify-between">
                        <span className="text-slate-400">Rounds Played:</span>
                        <span className="text-white font-mono">{stats.rounds_played.toLocaleString()}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-slate-400">Rounds Won:</span>
                        <span className="text-green-400 font-mono">{stats.rounds_won.toLocaleString()}</span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-slate-400">Win Rate:</span>
                        <span className="text-white font-mono">{stats.win_rate.toFixed(2)}%</span>
                      </div>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {(activeTab === "deployments" || activeTab === "wins") && (
              <MinerDeploymentsGrouped 
                deployments={deployments}
                loadingDeployments={loadingDeployments}
                hasMore={deploymentsResponse?.has_more || false}
                onLoadMore={loadMoreDeployments}
              />
            )}
          </>
        )}
      </main>
    </div>
  );
}

