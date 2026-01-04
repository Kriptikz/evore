"use client";

import { useState, useEffect, useMemo } from "react";
import Link from "next/link";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { usePortfolio } from "@/hooks/usePortfolio";
import { useToast } from "@/components/Toast";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { formatOre, truncateAddress } from "@/lib/format";

interface MinerData extends MinerSnapshotEntry {
  loading?: boolean;
  error?: string;
}

export function BookmarkedMinersSection() {
  const { bookmarks, removeBookmark, updateBookmark } = useMinerBookmarks();
  const { addEntry, isInPortfolio } = usePortfolio();
  const { success } = useToast();
  const [minerData, setMinerData] = useState<Map<string, MinerData>>(new Map());
  const [isExpanded, setIsExpanded] = useState(false);
  const [editingLabel, setEditingLabel] = useState<string | null>(null);
  const [labelInput, setLabelInput] = useState("");

  // Fetch data for all bookmarked miners
  useEffect(() => {
    if (bookmarks.length === 0) return;

    // Helper to retry on 429 rate limit
    const fetchWithRetry = async <T,>(
      fn: () => Promise<T>,
      maxRetries = 3,
      baseDelay = 1000
    ): Promise<T> => {
      for (let attempt = 0; attempt <= maxRetries; attempt++) {
        try {
          return await fn();
        } catch (err) {
          const is429 = err instanceof Error && err.message.includes("429");
          if (is429 && attempt < maxRetries) {
            const delay = baseDelay * Math.pow(2, attempt);
            await new Promise((resolve) => setTimeout(resolve, delay));
            continue;
          }
          throw err;
        }
      }
      throw new Error("Max retries exceeded");
    };

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

      // Fetch sequentially to avoid rate limits
      for (const bookmark of bookmarks) {
        try {
          const result = await fetchWithRetry(() =>
            api.getMinerSnapshots({
              search: bookmark.pubkey,
              limit: 1,
            })
          );

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
        // Small delay between requests
        await new Promise((resolve) => setTimeout(resolve, 100));
      }
    };

    fetchMinerData();
  }, [bookmarks]);

  // Calculate totals for all bookmarks
  const totals = useMemo(() => {
    let totalUnclaimed = 0;
    let totalRefined = 0;

    bookmarks.forEach((bookmark) => {
      const data = minerData.get(bookmark.pubkey);
      if (data && !data.loading && !data.error) {
        totalUnclaimed += data.unclaimed_ore;
        totalRefined += data.refined_ore;
      }
    });

    return { totalUnclaimed, totalRefined };
  }, [bookmarks, minerData]);

  const handleSaveLabel = (pubkey: string) => {
    updateBookmark(pubkey, { label: labelInput || undefined });
    setEditingLabel(null);
    setLabelInput("");
  };

  const handleAddToPortfolio = (pubkey: string, label?: string) => {
    addEntry(pubkey, label);
    success("Added to portfolio");
  };

  if (bookmarks.length === 0) {
    return null;
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 mb-6 overflow-hidden">
      {/* Compact Header - Always visible */}
      <button
        onClick={() => setIsExpanded(!isExpanded)}
        className="w-full px-4 py-3 flex items-center justify-between hover:bg-slate-700/30 transition-colors"
      >
        <div className="flex items-center gap-3">
          <span className="text-amber-400">⭐</span>
          <span className="text-sm font-medium text-white">Bookmarks</span>
          <span className="px-1.5 py-0.5 bg-slate-700 rounded text-xs text-slate-300">{bookmarks.length}</span>
        </div>
        
        <div className="flex items-center gap-4">
          {/* Totals - always visible */}
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
          
          {/* Expand/Collapse icon */}
          <svg
            className={`w-5 h-5 text-slate-500 transition-transform ${isExpanded ? "rotate-180" : ""}`}
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </div>
      </button>

      {/* Expanded Content */}
      {isExpanded && (
        <div className="border-t border-slate-700/50">
          <table className="w-full text-sm">
            <thead className="bg-slate-900/50 text-xs text-slate-500">
              <tr>
                <th className="px-4 py-2 text-left font-medium">Miner</th>
                <th className="px-4 py-2 text-right font-medium">Unclaimed</th>
                <th className="px-4 py-2 text-right font-medium">Refined</th>
                <th className="px-4 py-2 text-center font-medium w-24">Portfolio</th>
                <th className="px-4 py-2 w-10"></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-slate-700/30">
              {bookmarks.map((bookmark) => {
                const data = minerData.get(bookmark.pubkey);
                const isLoading = data?.loading || false;
                const hasError = !!data?.error;
                const inPortfolio = isInPortfolio(bookmark.pubkey);

                return (
                  <tr
                    key={bookmark.pubkey}
                    className="hover:bg-slate-700/30 transition-colors"
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

                    {/* Add to Portfolio */}
                    <td className="px-4 py-2 text-center">
                      {inPortfolio ? (
                        <span className="text-xs text-purple-400">💼 In portfolio</span>
                      ) : (
                        <button
                          onClick={() => handleAddToPortfolio(bookmark.pubkey, bookmark.label)}
                          className="text-xs text-slate-500 hover:text-purple-400 transition-colors"
                        >
                          + Add
                        </button>
                      )}
                    </td>

                    {/* Remove */}
                    <td className="px-4 py-2 text-center">
                      <button
                        onClick={() => removeBookmark(bookmark.pubkey)}
                        className="text-slate-600 hover:text-red-400 transition-colors"
                        title="Remove bookmark"
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
          
          {/* Footer with link to portfolio */}
          <div className="px-4 py-2 border-t border-slate-700/50 bg-slate-900/30">
            <Link 
              href="/portfolio" 
              className="text-xs text-purple-400 hover:text-purple-300 flex items-center gap-1"
            >
              <span>💼</span>
              <span>View Portfolio for claimable totals →</span>
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
