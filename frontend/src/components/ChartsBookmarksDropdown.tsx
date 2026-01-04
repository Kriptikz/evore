"use client";

import { useState, useRef, useEffect } from "react";
import Link from "next/link";
import { useChartsBookmarks, ChartsBookmark } from "@/hooks/useChartsBookmarks";

export function ChartsBookmarksDropdown() {
  const { bookmarks, removeBookmark } = useChartsBookmarks();
  const [isOpen, setIsOpen] = useState(false);
  const dropdownRef = useRef<HTMLDivElement>(null);

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
        className={`relative p-2 rounded-lg transition-colors ${
          isOpen
            ? "bg-purple-500/20 text-purple-400"
            : "text-slate-400 hover:text-white hover:bg-slate-800"
        }`}
        title="Chart Views"
      >
        <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z"
          />
        </svg>
        {/* Badge */}
        {bookmarks.length > 0 && (
          <span className="absolute -top-1 -right-1 w-4 h-4 bg-purple-500 text-white text-[10px] font-bold rounded-full flex items-center justify-center">
            {bookmarks.length > 9 ? "9+" : bookmarks.length}
          </span>
        )}
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-72 bg-slate-800 border border-slate-700 rounded-xl shadow-xl z-50 overflow-hidden">
          <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
            <h3 className="text-sm font-medium text-white">Saved Chart Views</h3>
            <Link
              href="/charts"
              className="text-xs text-purple-400 hover:text-purple-300"
              onClick={() => setIsOpen(false)}
            >
              Charts Page
            </Link>
          </div>

          <div className="max-h-80 overflow-y-auto">
            {bookmarks.length === 0 ? (
              <div className="px-4 py-8 text-center">
                <div className="text-3xl mb-2">📊</div>
                <p className="text-slate-400 text-sm">No chart views saved yet</p>
                <p className="text-slate-500 text-xs mt-1">
                  Configure charts and save your view for quick access
                </p>
              </div>
            ) : (
              <div className="divide-y divide-slate-700/50">
                {bookmarks.map((bookmark) => (
                  <BookmarkItem
                    key={bookmark.id}
                    bookmark={bookmark}
                    onRemove={() => removeBookmark(bookmark.id)}
                    onClose={() => setIsOpen(false)}
                  />
                ))}
              </div>
            )}
          </div>

          {bookmarks.length > 0 && (
            <div className="px-4 py-2 border-t border-slate-700 bg-slate-850">
              <Link
                href="/charts"
                className="block w-full text-center text-xs text-slate-400 hover:text-white py-1"
                onClick={() => setIsOpen(false)}
              >
                Create new chart view →
              </Link>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function BookmarkItem({
  bookmark,
  onRemove,
  onClose,
}: {
  bookmark: ChartsBookmark;
  onRemove: () => void;
  onClose: () => void;
}) {
  // Parse the query string to show a preview of chart types
  const chartTypes = parseChartTypesFromQuery(bookmark.queryString);

  return (
    <div className="flex items-center gap-2 px-4 py-3 hover:bg-slate-700/50 transition-colors group">
      <Link
        href={`/charts?c=${bookmark.queryString}`}
        className="flex-1 min-w-0"
        onClick={onClose}
      >
        <div className="text-sm text-white group-hover:text-purple-400 transition-colors truncate">
          {bookmark.name}
        </div>
        <div className="text-xs text-slate-500 truncate">
          {chartTypes.length > 0 ? chartTypes.join(", ") : "Charts view"}
        </div>
      </Link>

      {/* Remove button */}
      <button
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          onRemove();
        }}
        className="p-1 text-slate-500 hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all"
        title="Remove bookmark"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>
  );
}

function parseChartTypesFromQuery(queryString: string): string[] {
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

    return parts
      .map((part) => {
        const type = part.split(":")[0];
        return typeLabels[type] || type;
      })
      .filter(Boolean);
  } catch {
    return [];
  }
}

