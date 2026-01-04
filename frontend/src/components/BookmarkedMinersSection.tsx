"use client";

import { useState, useEffect, useMemo } from "react";
import Link from "next/link";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { formatOre, truncateAddress } from "@/lib/format";

interface MinerData extends MinerSnapshotEntry {
  loading?: boolean;
  error?: string;
}

export function BookmarkedMinersSection() {
  const { bookmarks, removeBookmark, toggleIncludeInTotals, updateBookmark } = useMinerBookmarks();
  const [minerData, setMinerData] = useState<Map<string, MinerData>>(new Map());
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
                error: "No data",
              });
            }
          } catch {
            newData.set(bookmark.pubkey, {
              miner_pubkey: bookmark.pubkey,
              refined_ore: 0,
              unclaimed_ore: 0,
              lifetime_sol: 0,
              lifetime_ore: 0,
              loading: false,
              error: "Error",
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

  const handleSaveLabel = (pubkey: string) => {
    updateBookmark(pubkey, { label: labelInput || undefined });
    setEditingLabel(null);
    setLabelInput("");
  };

  if (bookmarks.length === 0) {
    return null;
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 mb-6 overflow-hidden">
      {/* Compact Header with Totals */}
      <div className="px-4 py-3 flex items-center justify-between flex-wrap gap-3">
        <div className="flex items-center gap-2">
          <span className="text-amber-400">⭐</span>
          <span className="text-sm font-medium text-white">Bookmarked ({bookmarks.length})</span>
        </div>
        
        {/* Totals */}
        {totals.count > 0 && (
          <div className="flex items-center gap-4 text-sm">
            <div className="flex items-center gap-1.5">
              <span className="text-slate-500 text-xs">Unclaimed:</span>
              <span className="text-amber-400 font-mono text-sm">{formatOre(totals.totalUnclaimed)}</span>
            </div>
            <div className="flex items-center gap-1.5">
              <span className="text-slate-500 text-xs">Refined:</span>
              <span className="text-green-400 font-mono text-sm">{formatOre(totals.totalRefined)}</span>
            </div>
          </div>
        )}
      </div>

      {/* Compact Table */}
      <div className="border-t border-slate-700/50">
        <table className="w-full text-sm">
          <thead className="bg-slate-900/50 text-xs text-slate-500">
            <tr>
              <th className="px-4 py-2 text-left font-medium">Miner</th>
              <th className="px-4 py-2 text-right font-medium">Unclaimed</th>
              <th className="px-4 py-2 text-right font-medium">Refined</th>
              <th className="px-4 py-2 text-center font-medium w-20">Totals</th>
              <th className="px-4 py-2 w-10"></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-slate-700/30">
            {bookmarks.map((bookmark) => {
              const data = minerData.get(bookmark.pubkey);
              const isLoading = data?.loading || false;
              const hasError = !!data?.error;

              return (
                <tr
                  key={bookmark.pubkey}
                  className={`hover:bg-slate-700/30 transition-colors ${
                    !bookmark.includeInTotals ? "opacity-60" : ""
                  }`}
                >
                  {/* Miner */}
                  <td className="px-4 py-2">
                    {editingLabel === bookmark.pubkey ? (
                      <div className="flex items-center gap-1">
                        <input
                          type="text"
                          value={labelInput}
                          onChange={(e) => setLabelInput(e.target.value)}
                          onKeyDown={(e) => {
                            if (e.key === "Enter") handleSaveLabel(bookmark.pubkey);
                            if (e.key === "Escape") {
                              setEditingLabel(null);
                              setLabelInput("");
                            }
                          }}
                          onBlur={() => handleSaveLabel(bookmark.pubkey)}
                          placeholder="Label..."
                          className="w-24 px-1.5 py-0.5 bg-slate-800 border border-slate-600 rounded text-xs text-white focus:outline-none focus:ring-1 focus:ring-amber-500"
                          autoFocus
                        />
                      </div>
                    ) : (
                      <div className="flex items-center gap-2">
                        <Link
                          href={`/miners/${bookmark.pubkey}`}
                          className="font-mono text-white hover:text-amber-400 transition-colors"
                        >
                          {bookmark.label || truncateAddress(bookmark.pubkey)}
                        </Link>
                        <button
                          onClick={() => {
                            setEditingLabel(bookmark.pubkey);
                            setLabelInput(bookmark.label || "");
                          }}
                          className="text-slate-600 hover:text-slate-400 transition-colors"
                          title="Edit label"
                        >
                          <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                          </svg>
                        </button>
                      </div>
                    )}
                  </td>

                  {/* Unclaimed */}
                  <td className="px-4 py-2 text-right font-mono">
                    {isLoading ? (
                      <div className="w-4 h-4 border border-slate-600 border-t-amber-500 rounded-full animate-spin ml-auto" />
                    ) : hasError ? (
                      <span className="text-slate-600">-</span>
                    ) : (
                      <span className="text-amber-400">{formatOre(data?.unclaimed_ore || 0)}</span>
                    )}
                  </td>

                  {/* Refined */}
                  <td className="px-4 py-2 text-right font-mono">
                    {isLoading ? (
                      <div className="w-4 h-4 border border-slate-600 border-t-green-500 rounded-full animate-spin ml-auto" />
                    ) : hasError ? (
                      <span className="text-slate-600">-</span>
                    ) : (
                      <span className="text-green-400">{formatOre(data?.refined_ore || 0)}</span>
                    )}
                  </td>

                  {/* Include Toggle */}
                  <td className="px-4 py-2 text-center">
                    <input
                      type="checkbox"
                      checked={bookmark.includeInTotals}
                      onChange={() => toggleIncludeInTotals(bookmark.pubkey)}
                      className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-amber-500 focus:ring-amber-500/50 cursor-pointer"
                      title={bookmark.includeInTotals ? "Included in totals" : "Not in totals"}
                    />
                  </td>

                  {/* Remove */}
                  <td className="px-4 py-2 text-center">
                    <button
                      onClick={() => removeBookmark(bookmark.pubkey)}
                      className="text-slate-600 hover:text-red-400 transition-colors"
                      title="Remove"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
