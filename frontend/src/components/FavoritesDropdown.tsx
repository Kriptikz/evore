"use client";

import { useState, useRef, useEffect } from "react";
import Link from "next/link";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { useChartsBookmarks } from "@/hooks/useChartsBookmarks";
import { usePortfolio } from "@/hooks/usePortfolio";
import { truncateAddress } from "@/lib/format";

type TabType = "miners" | "charts";

export function FavoritesDropdown() {
  const { bookmarks: minerBookmarks, removeBookmark: removeMinerBookmark } = useMinerBookmarks();
  const { bookmarks: chartsBookmarks, removeBookmark: removeChartBookmark } = useChartsBookmarks();
  const { isInPortfolio } = usePortfolio();
  const [isOpen, setIsOpen] = useState(false);
  const [activeTab, setActiveTab] = useState<TabType>("miners");
  const dropdownRef = useRef<HTMLDivElement>(null);

  const totalCount = minerBookmarks.length + chartsBookmarks.length;

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (dropdownRef.current && !dropdownRef.current.contains(event.target as Node)) {
        setIsOpen(false);
      }
    }

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  return (
    <div className="relative" ref={dropdownRef}>
      {/* Trigger Button */}
      <button
        onClick={() => setIsOpen(!isOpen)}
        className={`flex items-center gap-1.5 px-3 py-2 rounded-lg transition-colors ${
          isOpen
            ? "bg-amber-500/20 text-amber-400"
            : totalCount > 0 
              ? "text-amber-400 hover:text-amber-300 hover:bg-slate-800"
              : "text-slate-400 hover:text-white hover:bg-slate-800"
        }`}
        title="Favorites"
      >
        <svg className="w-5 h-5" fill={totalCount > 0 ? "currentColor" : "none"} stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z"
          />
        </svg>
        <span className="text-sm hidden sm:inline">Favorites</span>
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-80 bg-slate-800 border border-slate-700 rounded-xl shadow-xl z-50 overflow-hidden">
          {/* Tabs */}
          <div className="flex border-b border-slate-700">
            <button
              onClick={() => setActiveTab("miners")}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors flex items-center justify-center gap-2 ${
                activeTab === "miners"
                  ? "bg-slate-700/50 text-amber-400 border-b-2 border-amber-400"
                  : "text-slate-400 hover:text-white hover:bg-slate-700/30"
              }`}
            >
              <span>⛏️</span>
              <span>Miners</span>
              {minerBookmarks.length > 0 && (
                <span className="px-1.5 py-0.5 bg-slate-600 rounded text-xs">
                  {minerBookmarks.length}
                </span>
              )}
            </button>
            <button
              onClick={() => setActiveTab("charts")}
              className={`flex-1 px-4 py-3 text-sm font-medium transition-colors flex items-center justify-center gap-2 ${
                activeTab === "charts"
                  ? "bg-slate-700/50 text-purple-400 border-b-2 border-purple-400"
                  : "text-slate-400 hover:text-white hover:bg-slate-700/30"
              }`}
            >
              <span>📊</span>
              <span>Charts</span>
              {chartsBookmarks.length > 0 && (
                <span className="px-1.5 py-0.5 bg-slate-600 rounded text-xs">
                  {chartsBookmarks.length}
                </span>
              )}
            </button>
          </div>

          {/* Content */}
          <div className="max-h-80 overflow-y-auto">
            {activeTab === "miners" ? (
              minerBookmarks.length === 0 ? (
                <EmptyState
                  icon="⭐"
                  title="No miners bookmarked"
                  description="Visit a miner and press B to bookmark"
                />
              ) : (
                <div className="divide-y divide-slate-700/30">
                  {minerBookmarks.map((bookmark) => (
                    <div
                      key={bookmark.pubkey}
                      className="flex items-center gap-2 px-4 py-3 hover:bg-slate-700/50 transition-colors group"
                    >
                      <Link
                        href={`/miners/${bookmark.pubkey}`}
                        className="flex-1 min-w-0"
                        onClick={() => setIsOpen(false)}
                      >
                        <div className="font-mono text-sm text-white group-hover:text-amber-400 transition-colors truncate">
                          {bookmark.label || truncateAddress(bookmark.pubkey)}
                        </div>
                        {bookmark.label && (
                          <div className="text-xs text-slate-500 truncate">
                            {truncateAddress(bookmark.pubkey)}
                          </div>
                        )}
                      </Link>
                      {isInPortfolio(bookmark.pubkey) && (
                        <div className="px-1.5 py-0.5 text-[10px] bg-purple-500/20 text-purple-400 rounded" title="In portfolio">
                          💼
                        </div>
                      )}
                      <button
                        onClick={() => removeMinerBookmark(bookmark.pubkey)}
                        className="p-1 text-slate-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all"
                      >
                        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                        </svg>
                      </button>
                    </div>
                  ))}
                </div>
              )
            ) : chartsBookmarks.length === 0 ? (
              <EmptyState
                icon="📊"
                title="No chart views saved"
                description="Configure charts and save your view"
              />
            ) : (
              <div className="divide-y divide-slate-700/30">
                {chartsBookmarks.map((bookmark) => (
                  <div
                    key={bookmark.id}
                    className="flex items-center gap-2 px-4 py-3 hover:bg-slate-700/50 transition-colors group"
                  >
                    <Link
                      href={`/charts?c=${bookmark.queryString}`}
                      className="flex-1 min-w-0"
                      onClick={() => setIsOpen(false)}
                    >
                      <div className="text-sm text-white group-hover:text-purple-400 transition-colors truncate">
                        {bookmark.name}
                      </div>
                      <div className="text-xs text-slate-500 truncate">
                        {parseChartTypes(bookmark.queryString)}
                      </div>
                    </Link>
                    <button
                      onClick={() => removeChartBookmark(bookmark.id)}
                      className="p-1 text-slate-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all"
                    >
                      <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                      </svg>
                    </button>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* Footer */}
          {activeTab === "miners" && minerBookmarks.length > 0 && (
            <div className="border-t border-slate-700 px-4 py-2">
              <Link
                href="/portfolio"
                className="block text-center text-xs text-amber-400 hover:text-amber-300 py-1"
                onClick={() => setIsOpen(false)}
              >
                View Portfolio →
              </Link>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function EmptyState({ icon, title, description }: { icon: string; title: string; description: string }) {
  return (
    <div className="px-4 py-8 text-center">
      <div className="text-3xl mb-2">{icon}</div>
      <p className="text-slate-400 text-sm">{title}</p>
      <p className="text-slate-500 text-xs mt-1">{description}</p>
    </div>
  );
}

function parseChartTypes(queryString: string): string {
  try {
    const parts = queryString.split("|");
    const typeLabels: Record<string, string> = {
      rounds: "Rounds",
      treasury: "Treasury",
      mint: "Mint",
      inflation: "Inflation",
      cost_per_ore: "Cost/ORE",
      miners: "Miners",
    };
    
    const types = parts
      .map((part) => typeLabels[part.split(":")[0]])
      .filter(Boolean);
    
    return types.join(", ") || "Charts";
  } catch {
    return "Charts";
  }
}

