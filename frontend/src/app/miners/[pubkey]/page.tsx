"use client";

import { useState, useEffect, useCallback, useMemo, Suspense } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";
import { api, MinerStats, HistoricalDeployment, CursorResponse, MinerSquareStats } from "@/lib/api";
import { Header } from "@/components/Header";
import { RoundRangeFilter } from "@/components/RoundRangeFilter";
import { useMultiUrlState } from "@/hooks/useUrlState";
import { formatSol, formatOre } from "@/lib/format";

// Grouped deployment for a single round
interface RoundDeploymentGroup {
  round_id: number;
  amounts: number[];  // 25 squares
  total_amount: number;
  sol_earned: number;
  ore_earned: number;
  is_winner: boolean;
  is_top_miner: boolean;
  deployed_slot: number;
  winning_square: number; // The actual winning square from the round
}

type TabType = "overview" | "deployments" | "wins";

// Square Heatmap Component for favorite squares
function SquareHeatmap({
  squareStats,
  onSquareClick,
}: {
  squareStats: MinerSquareStats | null;
  onSquareClick?: (squareId: number) => void;
}) {
  if (!squareStats) {
    return (
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 text-center text-slate-400">
        Loading square stats...
      </div>
    );
  }

  const maxCount = Math.max(...squareStats.square_counts);
  const maxAmount = Math.max(...squareStats.square_amounts);

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
      <h3 className="text-lg font-semibold text-white mb-4">Favorite Squares</h3>
      <div className="grid grid-cols-5 gap-2">
        {squareStats.square_counts.map((count, idx) => {
          const amount = squareStats.square_amounts[idx];
          const wins = squareStats.square_wins[idx];
          const intensity = maxCount > 0 ? count / maxCount : 0;
          
          return (
            <button
              key={idx}
              onClick={() => onSquareClick?.(idx)}
              className={`
                relative aspect-square rounded-lg border transition-all
                ${count > 0 
                  ? "border-amber-500/50 hover:border-amber-400 cursor-pointer" 
                  : "border-slate-700 cursor-default"
                }
              `}
              style={{
                backgroundColor: count > 0 
                  ? `rgba(245, 158, 11, ${0.1 + intensity * 0.4})` 
                  : "rgba(51, 65, 85, 0.3)"
              }}
              title={`Square ${idx}: ${count} deploys, ${formatSol(amount)}, ${wins} wins`}
            >
              <div className="absolute inset-0 flex flex-col items-center justify-center text-xs">
                <span className="font-bold text-white">{idx}</span>
                {count > 0 && (
                  <span className="text-amber-400">{count}</span>
                )}
                {wins > 0 && (
                  <span className="text-green-400 text-[10px]">🏆{wins}</span>
                )}
              </div>
            </button>
          );
        })}
      </div>
      <div className="mt-4 text-xs text-slate-500">
        Total rounds: {squareStats.total_rounds.toLocaleString()}
      </div>
    </div>
  );
}

function MinerDeploymentsGrouped({
  deployments,
  loadingDeployments,
  hasMore,
  onLoadMore,
  showWinningOnly,
}: {
  deployments: HistoricalDeployment[];
  loadingDeployments: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  showWinningOnly?: boolean;
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
          winning_square: d.winning_square, // Use the winning_square from the backend
        });
      }
      
      const group = groups.get(d.round_id)!;
      group.amounts[d.square_id] += d.amount;
      group.total_amount += d.amount;
      group.sol_earned += d.sol_earned;
      group.ore_earned += d.ore_earned;
      if (d.is_winner) group.is_winner = true;
      if (d.is_top_miner) group.is_top_miner = true;
      if (d.deployed_slot < group.deployed_slot && d.deployed_slot > 0) {
        group.deployed_slot = d.deployed_slot;
      }
    }
    
    return Array.from(groups.values()).sort((a, b) => b.round_id - a.round_id);
  }, [deployments]);

  if (deployments.length === 0 && !loadingDeployments) {
    return (
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-8 text-center text-slate-400">
        {showWinningOnly ? "No winning rounds found" : "No deployment history found"}
      </div>
    );
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white">
          {showWinningOnly ? "Winning Rounds" : "Deployment History"} ({groupedByRound.length} rounds)
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
                  {group.winning_square < 25 && (
                    <span className="text-xs text-slate-500">
                      Winning square: {group.winning_square}
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
              
              {/* Squares Grid - Highlight ONLY the actual winning square */}
              <div className="flex flex-wrap gap-1.5">
                {deployedSquares.map((squareId) => {
                  const hasValidWinningSquare = group.winning_square < 25;
                  const isWinningSquare = hasValidWinningSquare && squareId === group.winning_square;
                  const minerDeployedToWinner = hasValidWinningSquare && group.amounts[group.winning_square] > 0;
                  
                  return (
                    <span
                      key={squareId}
                      className={`px-2 py-1 rounded text-xs ${
                        isWinningSquare && minerDeployedToWinner
                          ? "bg-green-500/30 text-green-400 ring-2 ring-green-500/50"
                          : group.is_winner && !isWinningSquare
                          ? "bg-amber-500/10 text-amber-400"
                          : "bg-slate-700 text-slate-400"
                      }`}
                      title={`Square ${squareId}: ${formatSol(group.amounts[squareId])}${isWinningSquare ? " (Winner!)" : ""}`}
                    >
                      {isWinningSquare ? "🏆" : "◼"}{squareId}: {formatSol(group.amounts[squareId])}
                    </span>
                  );
                })}
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

function MinerProfileContent() {
  const params = useParams();
  const pubkey = params.pubkey as string;

  const [stats, setStats] = useState<MinerStats | null>(null);
  const [squareStats, setSquareStats] = useState<MinerSquareStats | null>(null);
  const [deployments, setDeployments] = useState<HistoricalDeployment[]>([]);
  const [deploymentsResponse, setDeploymentsResponse] = useState<CursorResponse<HistoricalDeployment> | null>(null);
  const [loadingStats, setLoadingStats] = useState(true);
  const [loadingDeployments, setLoadingDeployments] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copiedAddress, setCopiedAddress] = useState(false);
  const [currentRoundId, setCurrentRoundId] = useState<number | undefined>(undefined);

  // URL state for tab and filters
  const [urlState, setUrlState] = useMultiUrlState({
    tab: "overview" as string,
    round_min: undefined as number | undefined,
    round_max: undefined as number | undefined,
  });

  const activeTab = urlState.tab as TabType;
  const roundMin = urlState.round_min;
  const roundMax = urlState.round_max;

  // Fetch current round ID
  useEffect(() => {
    api.getBoard().then(board => {
      setCurrentRoundId(board.round_id);
    }).catch(() => {
      api.getHistoricalRounds({ limit: 1 }).then(response => {
        if (response.data.length > 0) {
          setCurrentRoundId(response.data[0].round_id);
        }
      });
    });
  }, []);

  const fetchStats = useCallback(async () => {
    setLoadingStats(true);
    setError(null);
    try {
      const data = await api.getMinerStats(pubkey, {
        roundIdGte: roundMin,
        roundIdLte: roundMax,
      });
      setStats(data);
    } catch (err) {
      const message = err instanceof Error ? err.message : "Failed to load miner stats";
      // Handle "not found" error gracefully
      if (message.includes("not found") || message.includes("404")) {
        setError("This miner has no recorded stats yet. They may not have participated in any tracked rounds.");
      } else {
        setError(message);
      }
    } finally {
      setLoadingStats(false);
    }
  }, [pubkey, roundMin, roundMax]);

  const fetchSquareStats = useCallback(async () => {
    try {
      const data = await api.getMinerSquareStats(pubkey, {
        roundIdGte: roundMin,
        roundIdLte: roundMax,
      });
      setSquareStats(data);
    } catch (err) {
      console.error("Failed to load square stats:", err);
    }
  }, [pubkey, roundMin, roundMax]);

  const fetchDeployments = useCallback(async (cursor?: string, winnerOnly?: boolean) => {
    setLoadingDeployments(true);
    try {
      const data = await api.getMinerDeployments(pubkey, {
        cursor,
        limit: 50,
        winnerOnly,
        roundIdGte: roundMin,
        roundIdLte: roundMax,
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
  }, [pubkey, roundMin, roundMax]);

  useEffect(() => {
    fetchStats();
    fetchSquareStats();
  }, [fetchStats, fetchSquareStats]);

  useEffect(() => {
    if (activeTab === "deployments") {
      fetchDeployments(undefined, false);
    } else if (activeTab === "wins") {
      fetchDeployments(undefined, true);
    }
  }, [activeTab, fetchDeployments]);

  const handleTabChange = (tab: TabType) => {
    setUrlState({ tab });
    setDeployments([]);
    setDeploymentsResponse(null);
  };

  const handleRoundRangeChange = (min?: number, max?: number) => {
    setUrlState({ round_min: min, round_max: max });
  };

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
    <main className="max-w-7xl mx-auto px-4 py-8">
      {/* Address Header */}
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8">
        <div className="flex items-center justify-between flex-wrap gap-4">
          <div>
            <div className="text-sm text-slate-400 mb-1">Miner Address</div>
            <div className="flex items-center gap-3 flex-wrap">
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

      {/* Round Range Filter */}
      <div className="mb-6">
        <RoundRangeFilter
          roundMin={roundMin}
          roundMax={roundMax}
          currentRoundId={currentRoundId}
          onChange={handleRoundRangeChange}
          compact
        />
      </div>

      {/* Loading State */}
      {loadingStats && (
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      )}

      {/* Error State */}
      {error && (
        <div className="bg-amber-500/10 border border-amber-500/30 rounded-xl p-6 text-center">
          <div className="text-amber-400 mb-2">No Stats Found</div>
          <div className="text-slate-400">{error}</div>
          <div className="mt-4 flex gap-4 justify-center">
            <button
              onClick={fetchStats}
              className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black rounded-lg transition-colors"
            >
              Retry
            </button>
            <Link
              href="/leaderboard"
              className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
            >
              View Leaderboard
            </Link>
          </div>
        </div>
      )}

      {/* Stats Grid */}
      {!loadingStats && !error && stats && (
        <>
          <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
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
            <div className="bg-gradient-to-br from-cyan-500/10 to-blue-500/10 rounded-xl border border-cyan-500/30 p-6">
              <div className="text-cyan-400 text-sm mb-1">Cost per ORE</div>
              <div className="text-2xl font-bold text-cyan-300">
                {(() => {
                  // Cost per ORE = -net_sol / ore_earned (when net is negative and ore > 0)
                  const oreInFullUnits = stats.total_ore_earned / 1e11; // Convert from atomic units
                  if (stats.net_sol_change >= 0 || oreInFullUnits <= 0) {
                    return "0";
                  }
                  const costPerOre = (-stats.net_sol_change) / oreInFullUnits;
                  return formatSol(costPerOre);
                })()}
              </div>
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
                onClick={() => handleTabChange(tab)}
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
            <div className="grid md:grid-cols-2 gap-6">
              <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6">
                <h3 className="text-lg font-semibold text-white mb-4">Performance Summary</h3>
                <div className="space-y-4">
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
              
              <SquareHeatmap squareStats={squareStats} />
            </div>
          )}

          {(activeTab === "deployments" || activeTab === "wins") && (
            <MinerDeploymentsGrouped 
              deployments={deployments}
              loadingDeployments={loadingDeployments}
              hasMore={deploymentsResponse?.has_more || false}
              onLoadMore={loadMoreDeployments}
              showWinningOnly={activeTab === "wins"}
            />
          )}
        </>
      )}
    </main>
  );
}

export default function MinerProfilePage() {
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />
      <Suspense fallback={
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      }>
        <MinerProfileContent />
      </Suspense>
    </div>
  );
}
