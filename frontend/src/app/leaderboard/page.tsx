"use client";

import { useState, useEffect, useCallback, Suspense } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { api, LeaderboardEntry, OffsetResponse, CostPerOreStats } from "@/lib/api";
import { Header } from "@/components/Header";
import { RoundRangeFilter } from "@/components/RoundRangeFilter";
import { useMultiUrlState } from "@/hooks/useUrlState";
import { formatSol, formatOre, truncateAddress } from "@/lib/format";

type MetricType = "net_sol" | "sol_deployed" | "sol_earned" | "ore_earned" | "sol_cost";
type MinRoundsType = 0 | 100 | 500 | 1000 | 5000;

function LeaderboardContent() {
  const router = useRouter();
  const [leaderboard, setLeaderboard] = useState<OffsetResponse<LeaderboardEntry> | null>(null);
  const [costPerOre, setCostPerOre] = useState<CostPerOreStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [currentRoundId, setCurrentRoundId] = useState<number | undefined>(undefined);
  
  // URL state for all filters
  const [urlState, setUrlState] = useMultiUrlState({
    metric: "net_sol" as string,
    round_min: undefined as number | undefined,
    round_max: undefined as number | undefined,
    min_rounds: 0 as number,
    page: 1 as number,
    search: "" as string,
  });

  const metric = urlState.metric as MetricType;
  const roundMin = urlState.round_min;
  const roundMax = urlState.round_max;
  const minRounds = urlState.min_rounds as MinRoundsType;
  const page = urlState.page;
  const searchQuery = (urlState.search as string) || "";

  const [localSearch, setLocalSearch] = useState(searchQuery);
  const [debouncedSearch, setDebouncedSearch] = useState(searchQuery);

  // Fetch current round ID for round range calculations
  useEffect(() => {
    api.getBoard().then(board => {
      setCurrentRoundId(board.round_id);
    }).catch(() => {
      // Fallback - try to get from rounds
      api.getHistoricalRounds({ limit: 1 }).then(response => {
        if (response.data.length > 0) {
          setCurrentRoundId(response.data[0].round_id);
        }
      });
    });
  }, []);

  // Sync local search with URL search
  useEffect(() => {
    setLocalSearch(searchQuery);
  }, [searchQuery]);

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(localSearch);
      if (localSearch !== debouncedSearch) {
        setUrlState({ search: localSearch || undefined, page: 1 });
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [localSearch]);

  const fetchLeaderboard = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [leaderboardData, costData] = await Promise.all([
        api.getLeaderboard({
          metric: metric,
          roundIdGte: roundMin,
          roundIdLte: roundMax,
          page: debouncedSearch ? 1 : page,
          limit: 50,
          search: debouncedSearch || undefined,
          minRounds: minRounds > 0 ? minRounds : undefined,
        }),
        api.getCostPerOreStats({
          roundIdGte: roundMin,
          roundIdLte: roundMax,
        }),
      ]);
      setLeaderboard(leaderboardData);
      setCostPerOre(costData);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load leaderboard");
    } finally {
      setLoading(false);
    }
  }, [metric, roundMin, roundMax, page, debouncedSearch, minRounds]);

  useEffect(() => {
    fetchLeaderboard();
  }, [fetchLeaderboard]);

  const handleMetricChange = (newMetric: MetricType) => {
    setUrlState({ metric: newMetric, page: 1 });
  };

  const handleRoundRangeChange = (min?: number, max?: number) => {
    setUrlState({ round_min: min, round_max: max, page: 1 });
  };

  const handleMinRoundsChange = (newMin: MinRoundsType) => {
    setUrlState({ min_rounds: newMin, page: 1 });
  };

  const handlePageChange = (newPage: number) => {
    setUrlState({ page: newPage });
  };

  const getMinRoundsLabel = (m: MinRoundsType): string => {
    if (m === 0) return "All";
    return `${m}+`;
  };

  const getMetricLabel = (m: MetricType): string => {
    switch (m) {
      case "net_sol": return "Net SOL";
      case "sol_deployed": return "SOL Deployed";
      case "sol_earned": return "SOL Earned";
      case "ore_earned": return "ORE Earned";
      case "sol_cost": return "SOL Cost/ORE";
    }
  };

  const getRangeLabel = (): string => {
    if (roundMin === undefined && roundMax === undefined) return "All Time";
    if (roundMax === undefined && currentRoundId) {
      const diff = currentRoundId - (roundMin ?? 0);
      if (diff === 60) return "Last 60 Rounds";
      if (diff === 100) return "Last 100 Rounds";
      if (diff === 1000) return "Last 1000 Rounds";
    }
    if (roundMin !== undefined && roundMax !== undefined) {
      return `Rounds ${roundMin} - ${roundMax}`;
    }
    if (roundMin !== undefined) return `From Round ${roundMin}`;
    if (roundMax !== undefined) return `Up to Round ${roundMax}`;
    return "Custom Range";
  };

  const handleGoToMiner = () => {
    const address = localSearch.trim();
    if (!address) return;
    if (address.length >= 32 && address.length <= 44) {
      router.push(`/miners/${address}`);
    }
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleGoToMiner();
    }
  };

  return (
    <main className="max-w-7xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold text-white mb-6">Leaderboard</h1>

      {/* Filters */}
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8 space-y-6">
        {/* Round Range Filter */}
        <RoundRangeFilter
          roundMin={roundMin}
          roundMax={roundMax}
          currentRoundId={currentRoundId}
          onChange={handleRoundRangeChange}
        />

        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {/* Metric Selector */}
          <div>
            <label className="block text-sm text-slate-400 mb-2">Rank By</label>
            <div className="flex flex-wrap gap-2">
              {(["net_sol", "sol_deployed", "sol_earned", "ore_earned", "sol_cost"] as MetricType[]).map((m) => (
                <button
                  key={m}
                  onClick={() => handleMetricChange(m)}
                  className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                    metric === m
                      ? "bg-amber-500 text-black font-medium"
                      : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                  }`}
                >
                  {getMetricLabel(m)}
                </button>
              ))}
            </div>
            {metric === "sol_cost" && (
              <p className="text-xs text-slate-500 mt-1">
                Only shows miners with net loss and ORE earned. Lower cost is better.
              </p>
            )}
          </div>

          {/* Min Rounds Filter */}
          <div>
            <label className="block text-sm text-slate-400 mb-2">Min Rounds</label>
            <div className="flex flex-wrap gap-2">
              {([0, 100, 500, 1000, 5000] as MinRoundsType[]).map((m) => (
                <button
                  key={m}
                  onClick={() => handleMinRoundsChange(m)}
                  className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                    minRounds === m
                      ? "bg-amber-500 text-black font-medium"
                      : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                  }`}
                >
                  {getMinRoundsLabel(m)}
                </button>
              ))}
            </div>
          </div>

          {/* Search/Filter */}
          <div>
            <label className="block text-sm text-slate-400 mb-2">
              Filter Leaderboard
              {debouncedSearch && <span className="text-amber-400 ml-2">(ranking preserved)</span>}
            </label>
            <div className="flex gap-2">
              <input
                type="text"
                value={localSearch}
                onChange={(e) => setLocalSearch(e.target.value)}
                onKeyDown={handleSearchKeyDown}
                placeholder="Search by address..."
                className="flex-1 px-4 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
              />
              {localSearch.length >= 32 && (
                <button
                  onClick={handleGoToMiner}
                  className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black font-medium rounded-lg transition-colors whitespace-nowrap"
                >
                  View Profile ↗
                </button>
              )}
              {localSearch && (
                <button
                  onClick={() => {
                    setLocalSearch("");
                    setUrlState({ search: undefined });
                  }}
                  className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                  title="Clear search"
                >
                  ✕
                </button>
              )}
            </div>
            <p className="text-slate-500 text-xs mt-1">
              {debouncedSearch 
                ? `Showing miners matching "${debouncedSearch}" with their original ranking` 
                : "Type to filter leaderboard by address (keeps ranking position intact)"
              }
            </p>
          </div>
        </div>
      </div>

      {/* Stats Summary */}
      {leaderboard && (
        <div className="grid grid-cols-2 md:grid-cols-5 gap-4 mb-8">
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Total Miners</div>
            <div className="text-2xl font-bold text-white">{leaderboard.total_count.toLocaleString()}</div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Current Page</div>
            <div className="text-2xl font-bold text-white">{leaderboard.page} / {leaderboard.total_pages}</div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Ranking Metric</div>
            <div className="text-2xl font-bold text-amber-400">{getMetricLabel(metric)}</div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Time Period</div>
            <div className="text-2xl font-bold text-white">{getRangeLabel()}</div>
          </div>
          {costPerOre && (
            <div className="bg-gradient-to-br from-cyan-500/10 to-blue-500/10 rounded-xl border border-cyan-500/30 p-4">
              <div className="text-cyan-400 text-sm flex items-center gap-1">
                Cost per ORE
                <span className="text-xs text-cyan-400/60" title="Average SOL cost to acquire 1 ORE in the selected round range">ⓘ</span>
              </div>
              <div className="text-2xl font-bold text-cyan-300">
                {formatSol(costPerOre.cost_per_ore_lamports)} SOL
              </div>
              <div className="text-xs text-slate-500 mt-1">
                {costPerOre.total_rounds.toLocaleString()} rounds • {formatOre(costPerOre.total_ore_minted_atomic)} ORE minted
              </div>
            </div>
          )}
        </div>
      )}

      {/* Loading State */}
      {loading && (
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      )}

      {/* Error State */}
      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-6 text-center">
          <div className="text-red-400 mb-2">Error loading leaderboard</div>
          <div className="text-slate-400">{error}</div>
          <button
            onClick={fetchLeaderboard}
            className="mt-4 px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition-colors"
          >
            Retry
          </button>
        </div>
      )}

      {/* Leaderboard Table */}
      {!loading && !error && leaderboard && (
        <>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden overflow-x-auto">
            <table className="w-full min-w-[900px]">
              <thead>
                <tr className="border-b border-slate-700/50 bg-slate-800/80">
                  <th className="text-left px-4 py-4 text-sm font-medium text-slate-400 w-16">Rank</th>
                  <th className="text-left px-4 py-4 text-sm font-medium text-slate-400">Miner</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">SOL Deployed</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">SOL Earned</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">Net SOL</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">ORE Earned</th>
                  {metric === "sol_cost" && (
                    <th className="text-right px-4 py-4 text-sm font-medium text-amber-400">SOL Cost/ORE</th>
                  )}
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">Rounds</th>
                </tr>
              </thead>
              <tbody>
                {leaderboard.data.map((entry) => (
                  <tr
                    key={entry.miner_pubkey}
                    className="border-b border-slate-700/30 hover:bg-slate-700/30 transition-colors"
                  >
                    <td className="px-4 py-3">
                      <div className={`text-lg font-bold ${
                        entry.rank === 1 ? "text-yellow-400" :
                        entry.rank === 2 ? "text-slate-300" :
                        entry.rank === 3 ? "text-amber-600" :
                        "text-slate-500"
                      }`}>
                        #{entry.rank}
                      </div>
                    </td>
                    <td className="px-4 py-3">
                      <Link
                        href={`/miners/${entry.miner_pubkey}`}
                        className="font-mono text-white hover:text-amber-400 transition-colors text-sm"
                      >
                        {truncateAddress(entry.miner_pubkey)}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-slate-300 text-sm">
                      {formatSol(entry.sol_deployed)}
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-green-400 text-sm">
                      {formatSol(entry.sol_earned)}
                    </td>
                    <td className={`px-4 py-3 text-right font-mono text-sm ${
                      entry.net_sol >= 0 ? "text-green-400" : "text-red-400"
                    }`}>
                      {formatSol(entry.net_sol)}
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-cyan-400 text-sm">
                      {formatOre(entry.ore_earned)}
                    </td>
                    {metric === "sol_cost" && (
                      <td className="px-4 py-3 text-right font-mono text-amber-400 text-sm">
                        {entry.sol_cost_per_ore !== null 
                          ? formatSol(entry.sol_cost_per_ore)
                          : "N/A"
                        }
                      </td>
                    )}
                    <td className="px-4 py-3 text-right text-slate-400 text-sm">
                      {entry.rounds_played.toLocaleString()}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* Pagination */}
          <div className="flex items-center justify-between mt-6">
            <div className="text-slate-400">
              Showing {(page - 1) * leaderboard.per_page + 1} - {Math.min(page * leaderboard.per_page, leaderboard.total_count)} of {leaderboard.total_count.toLocaleString()} miners
            </div>
            <div className="flex gap-2">
              <button
                onClick={() => handlePageChange(Math.max(1, page - 1))}
                disabled={page === 1}
                className={`px-4 py-2 rounded-lg transition-colors ${
                  page === 1
                    ? "bg-slate-800 text-slate-600 cursor-not-allowed"
                    : "bg-slate-700 text-white hover:bg-slate-600"
                }`}
              >
                Previous
              </button>
              <button
                onClick={() => handlePageChange(Math.min(leaderboard.total_pages, page + 1))}
                disabled={page >= leaderboard.total_pages}
                className={`px-4 py-2 rounded-lg transition-colors ${
                  page >= leaderboard.total_pages
                    ? "bg-slate-800 text-slate-600 cursor-not-allowed"
                    : "bg-slate-700 text-white hover:bg-slate-600"
                }`}
              >
                Next
              </button>
            </div>
          </div>
        </>
      )}
    </main>
  );
}

export default function LeaderboardPage() {
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />
      <Suspense fallback={
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      }>
        <LeaderboardContent />
      </Suspense>
    </div>
  );
}
