"use client";

import { useState, useEffect, useMemo, useCallback, useRef } from "react";
import Link from "next/link";
import { Header } from "@/components/Header";
import { useToast } from "@/components/Toast";
import { usePortfolio, PortfolioEntry } from "@/hooks/usePortfolio";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { formatOre, formatSol, truncateAddress } from "@/lib/format";

interface MinerData extends MinerSnapshotEntry {
  loading?: boolean;
  error?: string;
}

export default function PortfolioPage() {
  const { entries, addEntry, removeEntry, updateEntry, isInPortfolio } = usePortfolio();
  const { bookmarks } = useMinerBookmarks();
  const { success } = useToast();
  const [minerData, setMinerData] = useState<Map<string, MinerData>>(new Map());
  const [editingLabel, setEditingLabel] = useState<string | null>(null);
  const [labelInput, setLabelInput] = useState("");
  const [sortBy, setSortBy] = useState<"unclaimed" | "refined" | "claimable" | "lifetime">("claimable");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [showBookmarks, setShowBookmarks] = useState(false);
  
  // Search state
  const [search, setSearch] = useState("");
  const [searchResults, setSearchResults] = useState<MinerSnapshotEntry[]>([]);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchFocused, setSearchFocused] = useState(false);
  const searchRef = useRef<HTMLDivElement>(null);
  const debounceRef = useRef<NodeJS.Timeout>();

  // Fetch data for all portfolio entries
  const fetchAllData = useCallback(async () => {
    if (entries.length === 0) return;

    setIsRefreshing(true);
    const newData = new Map<string, MinerData>();

    // Set loading state
    entries.forEach((e) => {
      newData.set(e.pubkey, {
        miner_pubkey: e.pubkey,
        refined_ore: 0,
        unclaimed_ore: 0,
        lifetime_sol: 0,
        lifetime_ore: 0,
        loading: true,
      });
    });
    setMinerData(new Map(newData));

    // Fetch all in parallel
    await Promise.all(
      entries.map(async (entry) => {
        try {
          const result = await api.getMinerSnapshots({
            search: entry.pubkey,
            limit: 1,
          });

          if (result.data.length > 0) {
            newData.set(entry.pubkey, {
              ...result.data[0],
              loading: false,
            });
          } else {
            newData.set(entry.pubkey, {
              miner_pubkey: entry.pubkey,
              refined_ore: 0,
              unclaimed_ore: 0,
              lifetime_sol: 0,
              lifetime_ore: 0,
              loading: false,
              error: "No data found",
            });
          }
        } catch (err) {
          newData.set(entry.pubkey, {
            miner_pubkey: entry.pubkey,
            refined_ore: 0,
            unclaimed_ore: 0,
            lifetime_sol: 0,
            lifetime_ore: 0,
            loading: false,
            error: err instanceof Error ? err.message : "Failed to fetch",
          });
        }
        setMinerData(new Map(newData));
      })
    );
    setIsRefreshing(false);
  }, [entries]);

  useEffect(() => {
    fetchAllData();
  }, [fetchAllData]);

  // Close search dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (searchRef.current && !searchRef.current.contains(event.target as Node)) {
        setSearchFocused(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Search API with debounce
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    if (search.length < 3) {
      setSearchResults([]);
      setSearchLoading(false);
      return;
    }

    setSearchLoading(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const response = await api.getMinerSnapshots({
          search: search.trim(),
          limit: 10,
        });
        setSearchResults(response.data);
      } catch (err) {
        console.error("Search failed:", err);
        setSearchResults([]);
      } finally {
        setSearchLoading(false);
      }
    }, 300);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [search]);

  // Calculate totals
  const totals = useMemo(() => {
    let totalUnclaimed = 0;
    let totalRefined = 0;
    let totalLifetimeSol = 0;
    let totalLifetimeOre = 0;

    entries.forEach((entry) => {
      const data = minerData.get(entry.pubkey);
      if (data && !data.loading && !data.error) {
        totalUnclaimed += data.unclaimed_ore;
        totalRefined += data.refined_ore;
        totalLifetimeSol += data.lifetime_sol;
        totalLifetimeOre += data.lifetime_ore;
      }
    });

    // Claimable = unclaimed - 10% fee + refined
    const fee = totalUnclaimed * 0.1;
    const totalClaimable = totalUnclaimed - fee + totalRefined;

    return { 
      totalUnclaimed, 
      totalRefined, 
      totalClaimable,
      fee,
      totalLifetimeSol, 
      totalLifetimeOre,
    };
  }, [entries, minerData]);

  // Sorted entries
  const sortedEntries = useMemo(() => {
    return [...entries].sort((a, b) => {
      const dataA = minerData.get(a.pubkey);
      const dataB = minerData.get(b.pubkey);

      if (!dataA || !dataB) return 0;

      const claimableA = (dataA.unclaimed_ore * 0.9) + dataA.refined_ore;
      const claimableB = (dataB.unclaimed_ore * 0.9) + dataB.refined_ore;

      switch (sortBy) {
        case "unclaimed":
          return dataB.unclaimed_ore - dataA.unclaimed_ore;
        case "refined":
          return dataB.refined_ore - dataA.refined_ore;
        case "claimable":
          return claimableB - claimableA;
        case "lifetime":
          return dataB.lifetime_ore - dataA.lifetime_ore;
        default:
          return 0;
      }
    });
  }, [entries, minerData, sortBy]);

  // Bookmarks not in portfolio
  const availableBookmarks = useMemo(() => {
    return bookmarks.filter((b) => !isInPortfolio(b.pubkey));
  }, [bookmarks, isInPortfolio]);

  const handleAddToPortfolio = (pubkey: string, label?: string) => {
    addEntry(pubkey, label);
    success("Added to portfolio");
    setSearch("");
    setSearchFocused(false);
  };

  const handleRemoveEntry = (pubkey: string, label?: string) => {
    const displayName = label || truncateAddress(pubkey);
    removeEntry(pubkey);
    success(`Removed ${displayName} from portfolio`);
  };

  const handleStartEditLabel = (pubkey: string, currentLabel?: string) => {
    setEditingLabel(pubkey);
    setLabelInput(currentLabel || "");
  };

  const handleSaveLabel = (pubkey: string) => {
    updateEntry(pubkey, { label: labelInput || undefined });
    setEditingLabel(null);
    setLabelInput("");
  };

  const isValidPubkey = search.trim().length >= 32 && search.trim().length <= 44;

  return (
    <>
      <Header />
      <main className="max-w-7xl mx-auto px-4 py-8">
        {/* Page Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold text-white mb-2 flex items-center gap-3">
              <span className="text-amber-400">💼</span>
              Portfolio
            </h1>
            <p className="text-slate-400">
              Track your miners and aggregate claimable ORE
            </p>
          </div>
          <button
            onClick={fetchAllData}
            disabled={isRefreshing || entries.length === 0}
            className={`flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-xl transition-colors ${
              isRefreshing || entries.length === 0 ? "opacity-50 cursor-not-allowed" : ""
            }`}
          >
            <svg
              className={`w-4 h-4 ${isRefreshing ? "animate-spin" : ""}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
              />
            </svg>
            {isRefreshing ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        {/* Search to Add */}
        <div className="mb-6" ref={searchRef}>
          <label className="block text-sm text-slate-400 mb-2">Add miner to portfolio</label>
          <div className="relative">
            <div className="relative">
              <div className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                </svg>
              </div>
              <input
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                onFocus={() => setSearchFocused(true)}
                placeholder="Search by address or paste pubkey..."
                className="w-full pl-10 pr-4 py-3 bg-slate-800 border border-slate-700 rounded-xl text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
              />
            </div>

            {/* Search Dropdown */}
            {searchFocused && (search.length >= 3 || isValidPubkey) && (
              <div className="absolute left-0 right-0 top-full mt-1 bg-slate-800 border border-slate-700 rounded-xl shadow-xl z-50 overflow-hidden">
                {searchLoading ? (
                  <div className="px-4 py-3 flex items-center gap-2 text-slate-400">
                    <div className="w-4 h-4 border-2 border-slate-500 border-t-amber-500 rounded-full animate-spin" />
                    <span className="text-sm">Searching...</span>
                  </div>
                ) : searchResults.length > 0 ? (
                  <div className="max-h-60 overflow-y-auto">
                    {searchResults.map((miner) => {
                      const inPortfolio = isInPortfolio(miner.miner_pubkey);
                      return (
                        <button
                          key={miner.miner_pubkey}
                          onClick={() => !inPortfolio && handleAddToPortfolio(miner.miner_pubkey)}
                          disabled={inPortfolio}
                          className={`w-full px-4 py-3 flex items-center gap-3 transition-colors text-left ${
                            inPortfolio
                              ? "bg-slate-700/30 cursor-not-allowed"
                              : "hover:bg-slate-700/50"
                          }`}
                        >
                          <span className="font-mono text-sm text-white">
                            {truncateAddress(miner.miner_pubkey, 8)}
                          </span>
                          {inPortfolio ? (
                            <span className="text-xs text-green-400">✓ In portfolio</span>
                          ) : (
                            <span className="text-xs text-amber-400">+ Add</span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                ) : search.length >= 3 ? (
                  <div className="px-4 py-3 text-sm text-slate-400">
                    No miners found
                  </div>
                ) : null}

                {/* Direct add for valid pubkey */}
                {isValidPubkey && !searchResults.some(r => r.miner_pubkey === search.trim()) && !isInPortfolio(search.trim()) && (
                  <div className="border-t border-slate-700/50">
                    <button
                      onClick={() => handleAddToPortfolio(search.trim())}
                      className="w-full px-4 py-3 text-left hover:bg-slate-700/50 transition-colors flex items-center gap-2"
                    >
                      <span className="text-amber-400 text-sm">Add address:</span>
                      <span className="font-mono text-white text-sm">{truncateAddress(search.trim(), 8)}</span>
                    </button>
                  </div>
                )}
              </div>
            )}
          </div>

          {/* Quick add from bookmarks */}
          {availableBookmarks.length > 0 && (
            <div className="mt-3">
              <button
                onClick={() => setShowBookmarks(!showBookmarks)}
                className="text-xs text-slate-500 hover:text-slate-300 flex items-center gap-1"
              >
                <span>⭐ Add from bookmarks ({availableBookmarks.length})</span>
                <svg
                  className={`w-3 h-3 transition-transform ${showBookmarks ? "rotate-180" : ""}`}
                  fill="none"
                  stroke="currentColor"
                  viewBox="0 0 24 24"
                >
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
                </svg>
              </button>
              {showBookmarks && (
                <div className="mt-2 flex flex-wrap gap-2">
                  {availableBookmarks.map((bookmark) => (
                    <button
                      key={bookmark.pubkey}
                      onClick={() => handleAddToPortfolio(bookmark.pubkey, bookmark.label)}
                      className="px-2.5 py-1.5 bg-slate-800 hover:bg-slate-700 border border-slate-700 hover:border-amber-500/50 rounded-lg text-xs text-slate-300 hover:text-amber-400 transition-all"
                    >
                      + {bookmark.label || truncateAddress(bookmark.pubkey, 4)}
                    </button>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>

        {entries.length === 0 ? (
          <EmptyPortfolio />
        ) : (
          <>
            {/* Portfolio Summary Cards */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 mb-8">
              <SummaryCard
                title="Total Claimable"
                value={formatOre(totals.totalClaimable)}
                subtitle={`After 10% fee (${formatOre(totals.fee)})`}
                icon="💎"
                color="purple"
                large
              />
              <div className="grid grid-cols-2 gap-4">
                <SummaryCard
                  title="Unclaimed"
                  value={formatOre(totals.totalUnclaimed)}
                  icon="⛏️"
                  color="amber"
                />
                <SummaryCard
                  title="Refined"
                  value={formatOre(totals.totalRefined)}
                  icon="✨"
                  color="green"
                />
              </div>
              <div className="grid grid-cols-2 gap-4">
                <SummaryCard
                  title="Lifetime ORE"
                  value={formatOre(totals.totalLifetimeOre)}
                  icon="📈"
                  color="blue"
                />
                <SummaryCard
                  title="Lifetime SOL"
                  value={formatSol(totals.totalLifetimeSol)}
                  subtitle="Deployed"
                  icon="◎"
                  color="slate"
                />
              </div>
            </div>

            {/* Miners Table */}
            <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
              {/* Table Header */}
              <div className="px-6 py-4 border-b border-slate-700/50 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <h2 className="text-lg font-semibold text-white">Portfolio Miners</h2>
                  <span className="text-sm text-slate-400">({entries.length})</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-slate-500">Sort by:</span>
                  <select
                    value={sortBy}
                    onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
                    className="px-3 py-1.5 bg-slate-700 border border-slate-600 rounded-lg text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50"
                  >
                    <option value="claimable">Claimable</option>
                    <option value="unclaimed">Unclaimed</option>
                    <option value="refined">Refined</option>
                    <option value="lifetime">Lifetime</option>
                  </select>
                </div>
              </div>

              {/* Table */}
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead className="bg-slate-900/50">
                    <tr className="text-left text-xs text-slate-500 uppercase tracking-wider">
                      <th className="px-6 py-3">Miner</th>
                      <th className="px-6 py-3 text-right">Unclaimed</th>
                      <th className="px-6 py-3 text-right">Refined</th>
                      <th className="px-6 py-3 text-right">Claimable</th>
                      <th className="px-6 py-3 text-right">Lifetime ORE</th>
                      <th className="px-6 py-3 text-right">Lifetime SOL</th>
                      <th className="px-6 py-3 text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-700/30">
                    {sortedEntries.map((entry) => {
                      const data = minerData.get(entry.pubkey);
                      const isLoading = data?.loading || false;
                      const hasError = !!data?.error;
                      const claimable = data ? (data.unclaimed_ore * 0.9) + data.refined_ore : 0;

                      return (
                        <tr
                          key={entry.pubkey}
                          className="transition-colors hover:bg-slate-700/30"
                        >
                          {/* Miner Info */}
                          <td className="px-6 py-4">
                            {editingLabel === entry.pubkey ? (
                              <div className="flex items-center gap-2">
                                <input
                                  type="text"
                                  value={labelInput}
                                  onChange={(e) => setLabelInput(e.target.value)}
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter") handleSaveLabel(entry.pubkey);
                                    if (e.key === "Escape") setEditingLabel(null);
                                  }}
                                  placeholder="Enter label..."
                                  className="px-2 py-1 bg-slate-800 border border-slate-600 rounded text-sm text-white focus:outline-none focus:ring-1 focus:ring-amber-500"
                                  autoFocus
                                />
                                <button
                                  onClick={() => handleSaveLabel(entry.pubkey)}
                                  className="p-1 text-green-400 hover:text-green-300"
                                >
                                  ✓
                                </button>
                                <button
                                  onClick={() => setEditingLabel(null)}
                                  className="p-1 text-slate-400 hover:text-slate-300"
                                >
                                  ✕
                                </button>
                              </div>
                            ) : (
                              <Link
                                href={`/miners/${entry.pubkey}`}
                                className="block group"
                              >
                                {entry.label ? (
                                  <>
                                    <div className="font-medium text-white group-hover:text-amber-400 transition-colors">
                                      {entry.label}
                                    </div>
                                    <div className="text-xs text-slate-500 font-mono">
                                      {truncateAddress(entry.pubkey)}
                                    </div>
                                  </>
                                ) : (
                                  <div className="font-mono text-sm text-white group-hover:text-amber-400 transition-colors">
                                    {truncateAddress(entry.pubkey)}
                                  </div>
                                )}
                              </Link>
                            )}
                          </td>

                          {/* Stats */}
                          {isLoading ? (
                            <td colSpan={5} className="px-6 py-4 text-center">
                              <div className="flex justify-center">
                                <div className="w-4 h-4 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin" />
                              </div>
                            </td>
                          ) : hasError ? (
                            <td colSpan={5} className="px-6 py-4 text-center text-slate-500 text-sm">
                              {data?.error}
                            </td>
                          ) : (
                            <>
                              <td className="px-6 py-4 text-right font-mono text-amber-400">
                                {formatOre(data?.unclaimed_ore || 0)}
                              </td>
                              <td className="px-6 py-4 text-right font-mono text-green-400">
                                {formatOre(data?.refined_ore || 0)}
                              </td>
                              <td className="px-6 py-4 text-right font-mono text-purple-400 font-medium">
                                {formatOre(claimable)}
                              </td>
                              <td className="px-6 py-4 text-right font-mono text-slate-300">
                                {formatOre(data?.lifetime_ore || 0)}
                              </td>
                              <td className="px-6 py-4 text-right font-mono text-slate-300">
                                {formatSol(data?.lifetime_sol || 0)}
                              </td>
                            </>
                          )}

                          {/* Actions */}
                          <td className="px-6 py-4">
                            <div className="flex items-center justify-end gap-1">
                              <button
                                onClick={() => handleStartEditLabel(entry.pubkey, entry.label)}
                                className="p-1.5 text-slate-500 hover:text-slate-300 rounded transition-colors"
                                title="Edit label"
                              >
                                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
                                  />
                                </svg>
                              </button>
                              <button
                                onClick={() => handleRemoveEntry(entry.pubkey, entry.label)}
                                className="p-1.5 text-slate-500 hover:text-red-400 rounded transition-colors"
                                title="Remove from portfolio"
                              >
                                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
                                  />
                                </svg>
                              </button>
                            </div>
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            </div>
          </>
        )}
      </main>
    </>
  );
}

function SummaryCard({
  title,
  value,
  subtitle,
  icon,
  color,
  large,
}: {
  title: string;
  value: string;
  subtitle?: string;
  icon: string;
  color: "amber" | "green" | "purple" | "blue" | "slate";
  large?: boolean;
}) {
  const colorStyles = {
    amber: "from-amber-500/20 to-orange-500/10 border-amber-500/30",
    green: "from-green-500/20 to-emerald-500/10 border-green-500/30",
    purple: "from-purple-500/20 to-violet-500/10 border-purple-500/30",
    blue: "from-blue-500/20 to-cyan-500/10 border-blue-500/30",
    slate: "from-slate-600/20 to-slate-700/10 border-slate-500/30",
  };

  const textColors = {
    amber: "text-amber-400",
    green: "text-green-400",
    purple: "text-purple-400",
    blue: "text-blue-400",
    slate: "text-slate-300",
  };

  return (
    <div
      className={`bg-gradient-to-br ${colorStyles[color]} rounded-xl border ${large ? "p-6" : "p-4"}`}
    >
      <div className="flex items-center gap-2 mb-2">
        <span className={large ? "text-2xl" : "text-lg"}>{icon}</span>
        <span className={`${large ? "text-sm" : "text-xs"} text-slate-400`}>{title}</span>
      </div>
      <div className={`${large ? "text-3xl" : "text-xl"} font-bold font-mono ${textColors[color]} mb-1`}>
        {value}
      </div>
      {subtitle && <div className="text-xs text-slate-500">{subtitle}</div>}
    </div>
  );
}

function EmptyPortfolio() {
  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-12 text-center">
      <div className="text-5xl mb-4">💼</div>
      <h2 className="text-xl font-semibold text-white mb-2">Your Portfolio is Empty</h2>
      <p className="text-slate-400 mb-6 max-w-md mx-auto">
        Add miners to your portfolio using the search bar above or from your bookmarks.
        Track total claimable ORE across all your miners.
      </p>
      <Link
        href="/miners"
        className="inline-flex items-center gap-2 px-4 py-2 bg-amber-500 hover:bg-amber-400 text-black font-medium rounded-xl transition-colors"
      >
        <span>Browse Miners</span>
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
        </svg>
      </Link>
    </div>
  );
}
