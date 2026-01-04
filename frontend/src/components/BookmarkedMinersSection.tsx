"use client";

import { useState, useEffect, useMemo } from "react";
import Link from "next/link";
import { useMinerBookmarks, MinerBookmark } from "@/hooks/useMinerBookmarks";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { formatOre, truncateAddress } from "@/lib/format";

interface MinerData extends MinerSnapshotEntry {
  loading?: boolean;
  error?: string;
}

export function BookmarkedMinersSection() {
  const { bookmarks, removeBookmark, toggleIncludeInTotals, updateBookmark } = useMinerBookmarks();
  const [minerData, setMinerData] = useState<Map<string, MinerData>>(new Map());
  const [isCollapsed, setIsCollapsed] = useState(false);
  const [editingLabel, setEditingLabel] = useState<string | null>(null);
  const [labelInput, setLabelInput] = useState("");

  // Fetch data for all bookmarked miners
  useEffect(() => {
    if (bookmarks.length === 0) return;

    const fetchMinerData = async () => {
      const newData = new Map<string, MinerData>();

      // Initialize with loading state
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

      // Fetch each miner's data
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
    };

    fetchMinerData();
  }, [bookmarks]);

  // Calculate totals for miners included in totals
  const totals = useMemo(() => {
    let totalUnclaimed = 0;
    let totalRefined = 0;
    let count = 0;

    bookmarks.forEach((bookmark) => {
      if (bookmark.includeInTotals) {
        const data = minerData.get(bookmark.pubkey);
        if (data && !data.loading && !data.error) {
          totalUnclaimed += data.unclaimed_ore;
          totalRefined += data.refined_ore;
          count++;
        }
      }
    });

    return { totalUnclaimed, totalRefined, count };
  }, [bookmarks, minerData]);

  const handleStartEditLabel = (pubkey: string, currentLabel?: string) => {
    setEditingLabel(pubkey);
    setLabelInput(currentLabel || "");
  };

  const handleSaveLabel = (pubkey: string) => {
    updateBookmark(pubkey, { label: labelInput || undefined });
    setEditingLabel(null);
    setLabelInput("");
  };

  const handleCancelEdit = () => {
    setEditingLabel(null);
    setLabelInput("");
  };

  if (bookmarks.length === 0) {
    return null;
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 mb-8 overflow-hidden">
      {/* Header */}
      <div
        className="px-6 py-4 border-b border-slate-700/50 flex items-center justify-between cursor-pointer hover:bg-slate-700/20 transition-colors"
        onClick={() => setIsCollapsed(!isCollapsed)}
      >
        <div className="flex items-center gap-3">
          <span className="text-xl">⭐</span>
          <h2 className="text-lg font-semibold text-white">Bookmarked Miners</h2>
          <span className="text-sm text-slate-400">({bookmarks.length})</span>
        </div>
        <div className="flex items-center gap-4">
          {/* Quick Totals Preview */}
          {totals.count > 0 && (
            <div className="flex items-center gap-4 text-sm">
              <div className="flex items-center gap-1.5">
                <span className="text-slate-400">Unclaimed:</span>
                <span className="text-amber-400 font-mono">{formatOre(totals.totalUnclaimed)}</span>
              </div>
              <div className="flex items-center gap-1.5">
                <span className="text-slate-400">Refined:</span>
                <span className="text-green-400 font-mono">{formatOre(totals.totalRefined)}</span>
              </div>
            </div>
          )}
          {/* Collapse Toggle */}
          <svg
            className={`w-5 h-5 text-slate-400 transition-transform ${isCollapsed ? "" : "rotate-180"}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </div>
      </div>

      {/* Content */}
      {!isCollapsed && (
        <div className="p-6">
          {/* Totals Card */}
          {totals.count > 0 && (
            <div className="bg-gradient-to-br from-amber-500/10 to-orange-500/10 rounded-xl border border-amber-500/30 p-6 mb-6">
              <h3 className="text-sm font-medium text-amber-400 mb-4">
                Portfolio Totals ({totals.count} miner{totals.count !== 1 ? "s" : ""} included)
              </h3>
              <div className="grid grid-cols-2 gap-6">
                <div>
                  <div className="text-sm text-slate-400 mb-1">Total Unclaimed ORE</div>
                  <div className="text-3xl font-bold text-amber-400 font-mono">
                    {formatOre(totals.totalUnclaimed)}
                  </div>
                </div>
                <div>
                  <div className="text-sm text-slate-400 mb-1">Total Refined ORE</div>
                  <div className="text-3xl font-bold text-green-400 font-mono">
                    {formatOre(totals.totalRefined)}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Miner Cards */}
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {bookmarks.map((bookmark) => {
              const data = minerData.get(bookmark.pubkey);
              const isLoading = data?.loading || false;
              const hasError = !!data?.error;

              return (
                <div
                  key={bookmark.pubkey}
                  className={`bg-slate-900/50 rounded-lg border p-4 transition-all ${
                    bookmark.includeInTotals
                      ? "border-amber-500/30"
                      : "border-slate-700/50 opacity-75"
                  }`}
                >
                  {/* Header */}
                  <div className="flex items-start justify-between mb-3">
                    <div className="flex-1 min-w-0">
                      {editingLabel === bookmark.pubkey ? (
                        <div className="flex items-center gap-2">
                          <input
                            type="text"
                            value={labelInput}
                            onChange={(e) => setLabelInput(e.target.value)}
                            onKeyDown={(e) => {
                              if (e.key === "Enter") handleSaveLabel(bookmark.pubkey);
                              if (e.key === "Escape") handleCancelEdit();
                            }}
                            placeholder="Enter label..."
                            className="flex-1 px-2 py-1 bg-slate-800 border border-slate-600 rounded text-sm text-white focus:outline-none focus:ring-1 focus:ring-amber-500"
                            autoFocus
                          />
                          <button
                            onClick={() => handleSaveLabel(bookmark.pubkey)}
                            className="p-1 text-green-400 hover:text-green-300"
                          >
                            ✓
                          </button>
                          <button
                            onClick={handleCancelEdit}
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
                              <div className="font-medium text-white group-hover:text-amber-400 transition-colors truncate">
                                {bookmark.label}
                              </div>
                              <div className="text-xs text-slate-500 font-mono truncate">
                                {truncateAddress(bookmark.pubkey)}
                              </div>
                            </>
                          ) : (
                            <div className="font-mono text-sm text-white group-hover:text-amber-400 transition-colors truncate">
                              {truncateAddress(bookmark.pubkey)}
                            </div>
                          )}
                        </Link>
                      )}
                    </div>

                    {/* Actions */}
                    <div className="flex items-center gap-1 ml-2">
                      <button
                        onClick={() => handleStartEditLabel(bookmark.pubkey, bookmark.label)}
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
                        onClick={() => removeBookmark(bookmark.pubkey)}
                        className="p-1.5 text-slate-500 hover:text-red-400 rounded transition-colors"
                        title="Remove bookmark"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            strokeWidth={2}
                            d="M6 18L18 6M6 6l12 12"
                          />
                        </svg>
                      </button>
                    </div>
                  </div>

                  {/* Data */}
                  {isLoading ? (
                    <div className="flex items-center justify-center py-4">
                      <div className="w-5 h-5 border-2 border-amber-500/30 border-t-amber-500 rounded-full animate-spin" />
                    </div>
                  ) : hasError ? (
                    <div className="text-sm text-slate-500 py-2">{data?.error}</div>
                  ) : (
                    <div className="grid grid-cols-2 gap-3 text-sm">
                      <div>
                        <div className="text-slate-500 text-xs mb-0.5">Unclaimed</div>
                        <div className="text-amber-400 font-mono">
                          {formatOre(data?.unclaimed_ore || 0)}
                        </div>
                      </div>
                      <div>
                        <div className="text-slate-500 text-xs mb-0.5">Refined</div>
                        <div className="text-green-400 font-mono">
                          {formatOre(data?.refined_ore || 0)}
                        </div>
                      </div>
                    </div>
                  )}

                  {/* Include in Totals Toggle */}
                  <div className="mt-3 pt-3 border-t border-slate-700/50">
                    <label className="flex items-center gap-2 cursor-pointer text-xs">
                      <input
                        type="checkbox"
                        checked={bookmark.includeInTotals}
                        onChange={() => toggleIncludeInTotals(bookmark.pubkey)}
                        className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-amber-500 focus:ring-amber-500/50"
                      />
                      <span className={bookmark.includeInTotals ? "text-slate-300" : "text-slate-500"}>
                        Include in portfolio totals
                      </span>
                    </label>
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}

