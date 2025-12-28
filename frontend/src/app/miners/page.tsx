"use client";

import { useState, useEffect, useCallback } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { api, MinerSnapshotsResponse } from "@/lib/api";
import { Header } from "@/components/Header";

type SortByType = "refined_ore" | "unclaimed_ore" | "lifetime_sol" | "lifetime_ore";
type OrderType = "desc" | "asc";

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
  if (ore >= 1000) {
    return ore.toLocaleString(undefined, { maximumFractionDigits: 2 }) + " ORE";
  }
  return ore.toFixed(4) + " ORE";
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return addr.slice(0, 6) + "..." + addr.slice(-4);
}

export default function MinersPage() {
  const router = useRouter();
  const [data, setData] = useState<MinerSnapshotsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  const [sortBy, setSortBy] = useState<SortByType>("refined_ore");
  const [order, setOrder] = useState<OrderType>("desc");
  const [page, setPage] = useState(1);
  const [searchQuery, setSearchQuery] = useState("");
  const [debouncedSearch, setDebouncedSearch] = useState("");

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedSearch(searchQuery);
      if (searchQuery !== debouncedSearch) {
        setPage(1);
      }
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  const fetchMiners = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await api.getMinerSnapshots({
        sortBy,
        order,
        page,
        limit: 50,
        search: debouncedSearch || undefined,
      });
      setData(result);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load miners");
    } finally {
      setLoading(false);
    }
  }, [sortBy, order, page, debouncedSearch]);

  useEffect(() => {
    fetchMiners();
  }, [fetchMiners]);

  const handleSortChange = (newSort: SortByType) => {
    if (newSort === sortBy) {
      // Toggle order if same field
      setOrder(order === "desc" ? "asc" : "desc");
    } else {
      setSortBy(newSort);
      setOrder("desc");
    }
    setPage(1);
  };

  const getSortLabel = (s: SortByType): string => {
    switch (s) {
      case "refined_ore": return "Refined ORE";
      case "unclaimed_ore": return "Unclaimed ORE";
      case "lifetime_sol": return "Lifetime SOL";
      case "lifetime_ore": return "Lifetime ORE";
    }
  };

  const handleGoToMiner = () => {
    const address = searchQuery.trim();
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
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />

      <main className="max-w-7xl mx-auto px-4 py-8">
        <div className="flex items-center justify-between mb-6">
          <h1 className="text-2xl font-bold text-white">All Miners</h1>
          {data && (
            <div className="text-sm text-slate-400">
              Snapshot from Round #{data.round_id.toLocaleString()}
            </div>
          )}
        </div>

        {/* Filters */}
        <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8">
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            {/* Sort By */}
            <div>
              <label className="block text-sm text-slate-400 mb-2">Sort By</label>
              <div className="flex flex-wrap gap-2">
                {(["refined_ore", "unclaimed_ore", "lifetime_sol", "lifetime_ore"] as SortByType[]).map((s) => (
                  <button
                    key={s}
                    onClick={() => handleSortChange(s)}
                    className={`px-3 py-1.5 text-sm rounded-lg transition-colors flex items-center gap-1 ${
                      sortBy === s
                        ? "bg-amber-500 text-black font-medium"
                        : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                    }`}
                  >
                    {getSortLabel(s)}
                    {sortBy === s && (
                      <span className="text-xs">{order === "desc" ? "↓" : "↑"}</span>
                    )}
                  </button>
                ))}
              </div>
            </div>

            {/* Search */}
            <div>
              <label className="block text-sm text-slate-400 mb-2">Search Miner</label>
              <div className="flex gap-2">
                <input
                  type="text"
                  value={searchQuery}
                  onChange={(e) => setSearchQuery(e.target.value)}
                  onKeyDown={handleSearchKeyDown}
                  placeholder="Search by address..."
                  className="flex-1 px-4 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
                />
                {searchQuery.length >= 32 && (
                  <button
                    onClick={handleGoToMiner}
                    className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black font-medium rounded-lg transition-colors whitespace-nowrap"
                  >
                    View Profile ↗
                  </button>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Results */}
        {loading && !data ? (
          <div className="flex items-center justify-center py-20">
            <div className="w-8 h-8 border-4 border-amber-500/30 border-t-amber-500 rounded-full animate-spin"></div>
          </div>
        ) : error ? (
          <div className="bg-red-500/10 border border-red-500/50 rounded-xl p-6 text-center">
            <p className="text-red-400">{error}</p>
            <button
              onClick={fetchMiners}
              className="mt-4 px-4 py-2 bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded-lg transition-colors"
            >
              Retry
            </button>
          </div>
        ) : data && data.data.length === 0 ? (
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-12 text-center">
            <p className="text-slate-400">No miners found</p>
          </div>
        ) : data ? (
          <>
            {/* Stats */}
            <div className="mb-4 flex items-center justify-between text-sm text-slate-400">
              <span>
                Showing {((data.page - 1) * data.per_page) + 1} - {Math.min(data.page * data.per_page, Number(data.total_count))} of {Number(data.total_count).toLocaleString()} miners
              </span>
            </div>

            {/* Table */}
            <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead className="bg-slate-900/50">
                    <tr>
                      <th className="px-6 py-4 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                        #
                      </th>
                      <th className="px-6 py-4 text-left text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Miner
                      </th>
                      <th className="px-6 py-4 text-right text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Refined ORE
                      </th>
                      <th className="px-6 py-4 text-right text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Unclaimed ORE
                      </th>
                      <th className="px-6 py-4 text-right text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Lifetime SOL
                      </th>
                      <th className="px-6 py-4 text-right text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Lifetime ORE
                      </th>
                      <th className="px-6 py-4 text-center text-xs font-medium text-slate-400 uppercase tracking-wider">
                        Actions
                      </th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-700/50">
                    {data.data.map((entry, index) => (
                      <tr 
                        key={entry.miner_pubkey}
                        className="hover:bg-slate-800/50 transition-colors"
                      >
                        <td className="px-6 py-4 text-slate-400 font-mono text-sm">
                          {((data.page - 1) * data.per_page) + index + 1}
                        </td>
                        <td className="px-6 py-4">
                          <Link
                            href={`/miners/${entry.miner_pubkey}`}
                            className="font-mono text-white hover:text-amber-400 transition-colors"
                          >
                            {truncateAddress(entry.miner_pubkey)}
                          </Link>
                        </td>
                        <td className={`px-6 py-4 text-right font-mono ${sortBy === "refined_ore" ? "text-amber-400 font-medium" : "text-white"}`}>
                          {formatOre(entry.refined_ore)}
                        </td>
                        <td className={`px-6 py-4 text-right font-mono ${sortBy === "unclaimed_ore" ? "text-amber-400 font-medium" : "text-white"}`}>
                          {formatOre(entry.unclaimed_ore)}
                        </td>
                        <td className={`px-6 py-4 text-right font-mono ${sortBy === "lifetime_sol" ? "text-amber-400 font-medium" : "text-white"}`}>
                          {formatSol(entry.lifetime_sol)}
                        </td>
                        <td className={`px-6 py-4 text-right font-mono ${sortBy === "lifetime_ore" ? "text-amber-400 font-medium" : "text-white"}`}>
                          {formatOre(entry.lifetime_ore)}
                        </td>
                        <td className="px-6 py-4 text-center">
                          <Link
                            href={`/miners/${entry.miner_pubkey}`}
                            className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                          >
                            View
                          </Link>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Pagination */}
            {data.total_pages > 1 && (
              <div className="mt-6 flex items-center justify-center gap-2">
                <button
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={data.page <= 1}
                  className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Previous
                </button>
                <div className="flex items-center gap-1">
                  {/* Show first page */}
                  {data.page > 3 && (
                    <>
                      <button
                        onClick={() => setPage(1)}
                        className="w-10 h-10 bg-slate-800 hover:bg-slate-700 text-white rounded-lg transition-colors"
                      >
                        1
                      </button>
                      {data.page > 4 && <span className="text-slate-500 px-2">...</span>}
                    </>
                  )}
                  
                  {/* Show nearby pages */}
                  {Array.from({ length: 5 }, (_, i) => data.page - 2 + i)
                    .filter((p) => p >= 1 && p <= data.total_pages)
                    .map((p) => (
                      <button
                        key={p}
                        onClick={() => setPage(p)}
                        className={`w-10 h-10 rounded-lg transition-colors ${
                          p === data.page
                            ? "bg-amber-500 text-black font-medium"
                            : "bg-slate-800 hover:bg-slate-700 text-white"
                        }`}
                      >
                        {p}
                      </button>
                    ))}
                  
                  {/* Show last page */}
                  {data.page < data.total_pages - 2 && (
                    <>
                      {data.page < data.total_pages - 3 && <span className="text-slate-500 px-2">...</span>}
                      <button
                        onClick={() => setPage(data.total_pages)}
                        className="w-10 h-10 bg-slate-800 hover:bg-slate-700 text-white rounded-lg transition-colors"
                      >
                        {data.total_pages}
                      </button>
                    </>
                  )}
                </div>
                <button
                  onClick={() => setPage((p) => Math.min(data.total_pages, p + 1))}
                  disabled={data.page >= data.total_pages}
                  className="px-4 py-2 bg-slate-800 hover:bg-slate-700 text-white rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  Next
                </button>
              </div>
            )}
          </>
        ) : null}
      </main>
    </div>
  );
}
