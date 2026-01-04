"use client";

import { useState, useEffect, useMemo } from "react";
import Link from "next/link";
import { Header } from "@/components/Header";
import { useToast } from "@/components/Toast";
import { useMinerBookmarks, MinerBookmark } from "@/hooks/useMinerBookmarks";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { formatOre, formatSol, truncateAddress } from "@/lib/format";

interface MinerData extends MinerSnapshotEntry {
  loading?: boolean;
  error?: string;
}

export default function PortfolioPage() {
  const { bookmarks, removeBookmark, toggleIncludeInTotals, updateBookmark } =
    useMinerBookmarks();
  const { success } = useToast();
  const [minerData, setMinerData] = useState<Map<string, MinerData>>(new Map());
  const [editingLabel, setEditingLabel] = useState<string | null>(null);
  const [labelInput, setLabelInput] = useState("");
  const [sortBy, setSortBy] = useState<"unclaimed" | "refined" | "lifetime">("unclaimed");
  const [isRefreshing, setIsRefreshing] = useState(false);

  // Fetch data for all bookmarked miners
  const fetchAllData = async () => {
    if (bookmarks.length === 0) return;

    setIsRefreshing(true);
    const newData = new Map<string, MinerData>();

    // Set loading state
    bookmarks.forEach((b) => {
      newData.set(b.pubkey, {
        miner_pubkey: b.pubkey,
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
      bookmarks.map(async (bookmark) => {
        try {
          const result = await api.getMinerSnapshots({
            search: bookmark.pubkey,
            limit: 1,
          });

          if (result.data.length > 0) {
            newData.set(bookmark.pubkey, {
              ...result.data[0],
              loading: false,
            });
          } else {
            newData.set(bookmark.pubkey, {
              miner_pubkey: bookmark.pubkey,
              refined_ore: 0,
              unclaimed_ore: 0,
              lifetime_sol: 0,
              lifetime_ore: 0,
              loading: false,
              error: "No data found",
            });
          }
        } catch (err) {
          newData.set(bookmark.pubkey, {
            miner_pubkey: bookmark.pubkey,
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
  };

  useEffect(() => {
    fetchAllData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [bookmarks.length]);

  // Calculate totals
  const totals = useMemo(() => {
    let totalUnclaimed = 0;
    let totalRefined = 0;
    let totalLifetimeSol = 0;
    let totalLifetimeOre = 0;
    let includedCount = 0;

    bookmarks.forEach((bookmark) => {
      if (bookmark.includeInTotals) {
        const data = minerData.get(bookmark.pubkey);
        if (data && !data.loading && !data.error) {
          totalUnclaimed += data.unclaimed_ore;
          totalRefined += data.refined_ore;
          totalLifetimeSol += data.lifetime_sol;
          totalLifetimeOre += data.lifetime_ore;
          includedCount++;
        }
      }
    });

    return { totalUnclaimed, totalRefined, totalLifetimeSol, totalLifetimeOre, includedCount };
  }, [bookmarks, minerData]);

  // Sorted miners
  const sortedBookmarks = useMemo(() => {
    return [...bookmarks].sort((a, b) => {
      const dataA = minerData.get(a.pubkey);
      const dataB = minerData.get(b.pubkey);

      if (!dataA || !dataB) return 0;

      switch (sortBy) {
        case "unclaimed":
          return dataB.unclaimed_ore - dataA.unclaimed_ore;
        case "refined":
          return dataB.refined_ore - dataA.refined_ore;
        case "lifetime":
          return dataB.lifetime_ore - dataA.lifetime_ore;
        default:
          return 0;
      }
    });
  }, [bookmarks, minerData, sortBy]);

  const handleStartEditLabel = (pubkey: string, currentLabel?: string) => {
    setEditingLabel(pubkey);
    setLabelInput(currentLabel || "");
  };

  const handleSaveLabel = (pubkey: string) => {
    updateBookmark(pubkey, { label: labelInput || undefined });
    setEditingLabel(null);
    setLabelInput("");
  };

  const handleRemoveBookmark = (pubkey: string, label?: string) => {
    const displayName = label || truncateAddress(pubkey);
    removeBookmark(pubkey);
    success(`Removed ${displayName} from bookmarks`);
  };

  return (
    <>
      <Header />
      <main className="max-w-7xl mx-auto px-4 py-8">
        {/* Page Header */}
        <div className="flex items-center justify-between mb-8">
          <div>
            <h1 className="text-3xl font-bold text-white mb-2 flex items-center gap-3">
              <span className="text-amber-400">💼</span>
              Your Portfolio
            </h1>
            <p className="text-slate-400">
              Track your bookmarked miners and aggregate statistics
            </p>
          </div>
          <button
            onClick={fetchAllData}
            disabled={isRefreshing}
            className={`flex items-center gap-2 px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-xl transition-colors ${
              isRefreshing ? "opacity-50 cursor-not-allowed" : ""
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

        {bookmarks.length === 0 ? (
          <EmptyPortfolio />
        ) : (
          <>
            {/* Portfolio Summary Cards */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4 mb-8">
              <SummaryCard
                title="Total Unclaimed"
                value={formatOre(totals.totalUnclaimed)}
                subtitle={`${totals.includedCount} miner${totals.includedCount !== 1 ? "s" : ""} included`}
                icon="⛏️"
                color="amber"
              />
              <SummaryCard
                title="Total Refined"
                value={formatOre(totals.totalRefined)}
                subtitle="Ready to claim"
                icon="✨"
                color="green"
              />
              <SummaryCard
                title="Combined Value"
                value={formatOre(totals.totalUnclaimed + totals.totalRefined)}
                subtitle="Unclaimed + Refined"
                icon="💎"
                color="purple"
              />
              <SummaryCard
                title="Lifetime Earnings"
                value={formatSol(totals.totalLifetimeSol)}
                subtitle={`${formatOre(totals.totalLifetimeOre)} ORE`}
                icon="📈"
                color="blue"
              />
            </div>

            {/* Miners Table */}
            <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
              {/* Table Header */}
              <div className="px-6 py-4 border-b border-slate-700/50 flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <h2 className="text-lg font-semibold text-white">Bookmarked Miners</h2>
                  <span className="text-sm text-slate-400">({bookmarks.length})</span>
                </div>
                <div className="flex items-center gap-2">
                  <span className="text-xs text-slate-500">Sort by:</span>
                  <select
                    value={sortBy}
                    onChange={(e) => setSortBy(e.target.value as typeof sortBy)}
                    className="px-3 py-1.5 bg-slate-700 border border-slate-600 rounded-lg text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50"
                  >
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
                      <th className="px-6 py-3">Include</th>
                      <th className="px-6 py-3">Miner</th>
                      <th className="px-6 py-3 text-right">Unclaimed</th>
                      <th className="px-6 py-3 text-right">Refined</th>
                      <th className="px-6 py-3 text-right">Lifetime ORE</th>
                      <th className="px-6 py-3 text-right">Lifetime SOL</th>
                      <th className="px-6 py-3 text-right">Actions</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-slate-700/30">
                    {sortedBookmarks.map((bookmark) => {
                      const data = minerData.get(bookmark.pubkey);
                      const isLoading = data?.loading || false;
                      const hasError = !!data?.error;

                      return (
                        <tr
                          key={bookmark.pubkey}
                          className={`transition-colors hover:bg-slate-700/30 ${
                            bookmark.includeInTotals ? "" : "opacity-60"
                          }`}
                        >
                          {/* Include Toggle */}
                          <td className="px-6 py-4">
                            <button
                              onClick={() => toggleIncludeInTotals(bookmark.pubkey)}
                              className={`w-5 h-5 rounded flex items-center justify-center transition-colors ${
                                bookmark.includeInTotals
                                  ? "bg-amber-500 text-black"
                                  : "bg-slate-700 border border-slate-600 text-transparent"
                              }`}
                            >
                              <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={3} d="M5 13l4 4L19 7" />
                              </svg>
                            </button>
                          </td>

                          {/* Miner Info */}
                          <td className="px-6 py-4">
                            {editingLabel === bookmark.pubkey ? (
                              <div className="flex items-center gap-2">
                                <input
                                  type="text"
                                  value={labelInput}
                                  onChange={(e) => setLabelInput(e.target.value)}
                                  onKeyDown={(e) => {
                                    if (e.key === "Enter") handleSaveLabel(bookmark.pubkey);
                                    if (e.key === "Escape") setEditingLabel(null);
                                  }}
                                  placeholder="Enter label..."
                                  className="px-2 py-1 bg-slate-800 border border-slate-600 rounded text-sm text-white focus:outline-none focus:ring-1 focus:ring-amber-500"
                                  autoFocus
                                />
                                <button
                                  onClick={() => handleSaveLabel(bookmark.pubkey)}
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
                                href={`/miners/${bookmark.pubkey}`}
                                className="block group"
                              >
                                {bookmark.label ? (
                                  <>
                                    <div className="font-medium text-white group-hover:text-amber-400 transition-colors">
                                      {bookmark.label}
                                    </div>
                                    <div className="text-xs text-slate-500 font-mono">
                                      {truncateAddress(bookmark.pubkey)}
                                    </div>
                                  </>
                                ) : (
                                  <div className="font-mono text-sm text-white group-hover:text-amber-400 transition-colors">
                                    {truncateAddress(bookmark.pubkey)}
                                  </div>
                                )}
                              </Link>
                            )}
                          </td>

                          {/* Stats */}
                          {isLoading ? (
                            <td colSpan={4} className="px-6 py-4 text-center">
                              <div className="flex justify-center">
                                <div className="w-4 h-4 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin" />
                              </div>
                            </td>
                          ) : hasError ? (
                            <td colSpan={4} className="px-6 py-4 text-center text-slate-500 text-sm">
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
                                onClick={() =>
                                  handleStartEditLabel(bookmark.pubkey, bookmark.label)
                                }
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
                                onClick={() =>
                                  handleRemoveBookmark(bookmark.pubkey, bookmark.label)
                                }
                                className="p-1.5 text-slate-500 hover:text-red-400 rounded transition-colors"
                                title="Remove bookmark"
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

            {/* Tips */}
            <div className="mt-6 flex items-center gap-4 text-xs text-slate-500">
              <div className="flex items-center gap-2">
                <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">B</kbd>
                <span>Toggle bookmark on miner page</span>
              </div>
              <span className="text-slate-700">•</span>
              <div className="flex items-center gap-2">
                <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">←</kbd>
                <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">→</kbd>
                <span>Navigate between bookmarked miners</span>
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
}: {
  title: string;
  value: string;
  subtitle: string;
  icon: string;
  color: "amber" | "green" | "purple" | "blue";
}) {
  const colorStyles = {
    amber: "from-amber-500/20 to-orange-500/10 border-amber-500/30",
    green: "from-green-500/20 to-emerald-500/10 border-green-500/30",
    purple: "from-purple-500/20 to-violet-500/10 border-purple-500/30",
    blue: "from-blue-500/20 to-cyan-500/10 border-blue-500/30",
  };

  const textColors = {
    amber: "text-amber-400",
    green: "text-green-400",
    purple: "text-purple-400",
    blue: "text-blue-400",
  };

  return (
    <div
      className={`bg-gradient-to-br ${colorStyles[color]} rounded-xl border p-5`}
    >
      <div className="flex items-center gap-2 mb-3">
        <span className="text-xl">{icon}</span>
        <span className="text-sm text-slate-400">{title}</span>
      </div>
      <div className={`text-2xl font-bold font-mono ${textColors[color]} mb-1`}>
        {value}
      </div>
      <div className="text-xs text-slate-500">{subtitle}</div>
    </div>
  );
}

function EmptyPortfolio() {
  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-12 text-center">
      <div className="text-5xl mb-4">📊</div>
      <h2 className="text-xl font-semibold text-white mb-2">No Miners Bookmarked</h2>
      <p className="text-slate-400 mb-6 max-w-md mx-auto">
        Start building your portfolio by bookmarking miners. Visit any miner&apos;s page
        and click the star button or press <kbd className="px-1.5 py-0.5 bg-slate-700 rounded border border-slate-600 text-xs">B</kbd> to bookmark.
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

