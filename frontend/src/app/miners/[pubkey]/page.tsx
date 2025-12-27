"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { api, MinerStats, HistoricalDeployment, CursorResponse } from "@/lib/api";

const LAMPORTS_PER_SOL = 1_000_000_000;
const ORE_DECIMALS = 11;

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
      {/* Header */}
      <header className="border-b border-slate-800/50 bg-slate-900/50 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-7xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link href="/" className="text-2xl font-bold bg-gradient-to-r from-amber-400 to-orange-500 bg-clip-text text-transparent">
              ORE Stats
            </Link>
            <span className="text-slate-500">/</span>
            <Link href="/miners" className="text-slate-400 hover:text-white transition-colors">
              Miners
            </Link>
            <span className="text-slate-500">/</span>
            <h1 className="text-xl text-white font-mono">{truncateAddress(pubkey)}</h1>
          </div>
          <nav className="flex gap-4">
            <Link href="/rounds" className="text-slate-400 hover:text-white transition-colors">
              Rounds
            </Link>
            <Link href="/miners" className="text-amber-400 font-medium">
              Miners
            </Link>
          </nav>
        </div>
      </header>

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

            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
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
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-slate-700/50 bg-slate-800/80">
                      <th className="text-left px-6 py-4 text-sm font-medium text-slate-400">Round</th>
                      <th className="text-center px-6 py-4 text-sm font-medium text-slate-400">Square</th>
                      <th className="text-right px-6 py-4 text-sm font-medium text-slate-400">Deployed</th>
                      <th className="text-right px-6 py-4 text-sm font-medium text-slate-400">SOL Earned</th>
                      <th className="text-right px-6 py-4 text-sm font-medium text-slate-400">ORE Earned</th>
                      <th className="text-center px-6 py-4 text-sm font-medium text-slate-400">Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {deployments.length === 0 && !loadingDeployments && (
                      <tr>
                        <td colSpan={6} className="text-center py-8 text-slate-400">
                          No deployment history found
                        </td>
                      </tr>
                    )}
                    {deployments.map((d, idx) => (
                      <tr
                        key={`${d.round_id}-${d.square_id}-${idx}`}
                        className="border-b border-slate-700/30 hover:bg-slate-700/30 transition-colors"
                      >
                        <td className="px-6 py-4">
                          <Link
                            href={`/rounds?round=${d.round_id}`}
                            className="text-amber-400 hover:text-amber-300 font-mono"
                          >
                            #{d.round_id}
                          </Link>
                        </td>
                        <td className="px-6 py-4 text-center">
                          <span className={`inline-flex items-center justify-center w-8 h-8 rounded-lg text-sm font-bold ${
                            d.is_winner ? "bg-yellow-500/20 text-yellow-400 ring-2 ring-yellow-500/50" : "bg-slate-700 text-slate-300"
                          }`}>
                            {d.square_id}
                          </span>
                        </td>
                        <td className="px-6 py-4 text-right font-mono text-white">
                          {formatSol(d.amount)}
                        </td>
                        <td className="px-6 py-4 text-right font-mono text-green-400">
                          {d.sol_earned > 0 ? formatSol(d.sol_earned) : "-"}
                        </td>
                        <td className="px-6 py-4 text-right font-mono text-amber-400">
                          {d.ore_earned > 0 ? formatOre(d.ore_earned) : "-"}
                        </td>
                        <td className="px-6 py-4 text-center">
                          <div className="flex items-center justify-center gap-2">
                            {d.is_winner && (
                              <span className="px-2 py-1 bg-green-500/20 text-green-400 text-xs rounded-full">
                                Winner
                              </span>
                            )}
                            {d.is_top_miner && (
                              <span className="px-2 py-1 bg-yellow-500/20 text-yellow-400 text-xs rounded-full">
                                👑 Top
                              </span>
                            )}
                            {!d.is_winner && !d.is_top_miner && (
                              <span className="text-slate-500 text-sm">—</span>
                            )}
                          </div>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>

                {/* Load More */}
                {deploymentsResponse?.has_more && (
                  <div className="p-4 border-t border-slate-700/50 text-center">
                    <button
                      onClick={loadMoreDeployments}
                      disabled={loadingDeployments}
                      className={`px-6 py-2 rounded-lg transition-colors ${
                        loadingDeployments
                          ? "bg-slate-700 text-slate-500 cursor-not-allowed"
                          : "bg-amber-500 hover:bg-amber-600 text-black font-medium"
                      }`}
                    >
                      {loadingDeployments ? "Loading..." : "Load More"}
                    </button>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </main>
    </div>
  );
}

