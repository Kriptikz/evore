"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { api, LeaderboardEntry, OffsetResponse } from "@/lib/api";

type MetricType = "net_sol" | "sol_earned" | "ore_earned" | "rounds_won";
type RangeType = "all" | "last_60" | "last_100" | "today";

const LAMPORTS_PER_SOL = 1_000_000_000;
const ORE_DECIMALS = 11;

function formatSol(lamports: number): string {
  const sol = lamports / LAMPORTS_PER_SOL;
  if (Math.abs(sol) >= 1000) {
    return sol.toLocaleString(undefined, { maximumFractionDigits: 1 }) + " SOL";
  }
  return sol.toFixed(4) + " SOL";
}

function formatOre(atomic: number): string {
  const ore = atomic / Math.pow(10, ORE_DECIMALS);
  return ore.toFixed(4) + " ORE";
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return addr.slice(0, 6) + "..." + addr.slice(-4);
}

export default function MinersPage() {
  const [leaderboard, setLeaderboard] = useState<OffsetResponse<LeaderboardEntry> | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const [metric, setMetric] = useState<MetricType>("net_sol");
  const [range, setRange] = useState<RangeType>("all");
  const [page, setPage] = useState(1);
  const [search, setSearch] = useState("");

  const fetchLeaderboard = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await api.getLeaderboard({
        metric,
        roundRange: range,
        page,
        limit: 50,
      });
      setLeaderboard(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load leaderboard");
    } finally {
      setLoading(false);
    }
  }, [metric, range, page]);

  useEffect(() => {
    fetchLeaderboard();
  }, [fetchLeaderboard]);

  const handleMetricChange = (newMetric: MetricType) => {
    setMetric(newMetric);
    setPage(1);
  };

  const handleRangeChange = (newRange: RangeType) => {
    setRange(newRange);
    setPage(1);
  };

  const getMetricLabel = (m: MetricType): string => {
    switch (m) {
      case "net_sol": return "Net SOL";
      case "sol_earned": return "SOL Earned";
      case "ore_earned": return "ORE Earned";
      case "rounds_won": return "Rounds Won";
    }
  };

  const getRangeLabel = (r: RangeType): string => {
    switch (r) {
      case "all": return "All Time";
      case "last_60": return "Last 60 Rounds";
      case "last_100": return "Last 100 Rounds";
      case "today": return "Today";
    }
  };

  const formatValue = (entry: LeaderboardEntry): string => {
    switch (metric) {
      case "net_sol":
      case "sol_earned":
        return formatSol(entry.value);
      case "ore_earned":
        return formatOre(entry.value);
      case "rounds_won":
        return entry.value.toLocaleString();
    }
  };

  const getValueClass = (entry: LeaderboardEntry): string => {
    if (metric === "net_sol") {
      return entry.value >= 0 ? "text-green-400" : "text-red-400";
    }
    return "text-white";
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
            <h1 className="text-xl text-white font-semibold">Miners Leaderboard</h1>
          </div>
          <nav className="flex gap-4">
            <Link href="/" className="text-slate-400 hover:text-white transition-colors">
              Rounds
            </Link>
            <Link href="/miners" className="text-amber-400 font-medium">
              Miners
            </Link>
            <Link href="/autominers" className="text-slate-400 hover:text-white transition-colors">
              AutoMiners
            </Link>
          </nav>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-4 py-8">
        {/* Filters */}
        <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8">
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            {/* Metric Selector */}
            <div>
              <label className="block text-sm text-slate-400 mb-2">Rank By</label>
              <div className="flex flex-wrap gap-2">
                {(["net_sol", "sol_earned", "ore_earned", "rounds_won"] as MetricType[]).map((m) => (
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
            </div>

            {/* Time Range */}
            <div>
              <label className="block text-sm text-slate-400 mb-2">Time Range</label>
              <div className="flex flex-wrap gap-2">
                {(["all", "last_100", "last_60", "today"] as RangeType[]).map((r) => (
                  <button
                    key={r}
                    onClick={() => handleRangeChange(r)}
                    className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                      range === r
                        ? "bg-amber-500 text-black font-medium"
                        : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                    }`}
                  >
                    {getRangeLabel(r)}
                  </button>
                ))}
              </div>
            </div>

            {/* Search */}
            <div>
              <label className="block text-sm text-slate-400 mb-2">Search Miner</label>
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Enter miner address..."
                className="w-full px-4 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
              />
            </div>
          </div>
        </div>

        {/* Stats Summary */}
        {leaderboard && (
          <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
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
              <div className="text-2xl font-bold text-white">{getRangeLabel(range)}</div>
            </div>
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
            <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
              <table className="w-full">
                <thead>
                  <tr className="border-b border-slate-700/50 bg-slate-800/80">
                    <th className="text-left px-6 py-4 text-sm font-medium text-slate-400 w-16">Rank</th>
                    <th className="text-left px-6 py-4 text-sm font-medium text-slate-400">Miner</th>
                    <th className="text-right px-6 py-4 text-sm font-medium text-slate-400">{getMetricLabel(metric)}</th>
                    <th className="text-right px-6 py-4 text-sm font-medium text-slate-400">Rounds</th>
                    <th className="text-center px-6 py-4 text-sm font-medium text-slate-400">Actions</th>
                  </tr>
                </thead>
                <tbody>
                  {leaderboard.data
                    .filter(entry => !search || entry.miner_pubkey.toLowerCase().includes(search.toLowerCase()))
                    .map((entry, idx) => (
                    <tr
                      key={entry.miner_pubkey}
                      className="border-b border-slate-700/30 hover:bg-slate-700/30 transition-colors"
                    >
                      <td className="px-6 py-4">
                        <div className={`text-lg font-bold ${
                          entry.rank === 1 ? "text-yellow-400" :
                          entry.rank === 2 ? "text-slate-300" :
                          entry.rank === 3 ? "text-amber-600" :
                          "text-slate-500"
                        }`}>
                          #{entry.rank}
                        </div>
                      </td>
                      <td className="px-6 py-4">
                        <Link
                          href={`/miners/${entry.miner_pubkey}`}
                          className="font-mono text-white hover:text-amber-400 transition-colors"
                        >
                          {truncateAddress(entry.miner_pubkey)}
                        </Link>
                      </td>
                      <td className={`px-6 py-4 text-right font-mono ${getValueClass(entry)}`}>
                        {formatValue(entry)}
                      </td>
                      <td className="px-6 py-4 text-right text-slate-400">
                        {entry.rounds_played.toLocaleString()}
                      </td>
                      <td className="px-6 py-4 text-center">
                        <Link
                          href={`/miners/${entry.miner_pubkey}`}
                          className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                        >
                          View Profile
                        </Link>
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
                  onClick={() => setPage(p => Math.max(1, p - 1))}
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
                  onClick={() => setPage(p => Math.min(leaderboard.total_pages, p + 1))}
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
    </div>
  );
}

