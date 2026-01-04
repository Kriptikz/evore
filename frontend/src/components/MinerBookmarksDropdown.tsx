"use client";

import { useState, useRef, useEffect } from "react";
import Link from "next/link";
import { useMinerBookmarks, MinerBookmark } from "@/hooks/useMinerBookmarks";
import { truncateAddress } from "@/lib/format";

export function MinerBookmarksDropdown() {
  const { bookmarks, removeBookmark } = useMinerBookmarks();
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
            ? "bg-amber-500/20 text-amber-400"
            : "text-slate-400 hover:text-white hover:bg-slate-800"
        }`}
        title="Miner Bookmarks"
      >
        <svg className="w-5 h-5" fill={bookmarks.length > 0 ? "currentColor" : "none"} stroke="currentColor" viewBox="0 0 24 24">
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={1.5}
            d="M11.049 2.927c.3-.921 1.603-.921 1.902 0l1.519 4.674a1 1 0 00.95.69h4.915c.969 0 1.371 1.24.588 1.81l-3.976 2.888a1 1 0 00-.363 1.118l1.518 4.674c.3.922-.755 1.688-1.538 1.118l-3.976-2.888a1 1 0 00-1.176 0l-3.976 2.888c-.783.57-1.838-.197-1.538-1.118l1.518-4.674a1 1 0 00-.363-1.118l-3.976-2.888c-.784-.57-.38-1.81.588-1.81h4.914a1 1 0 00.951-.69l1.519-4.674z"
          />
        </svg>
        {/* Badge */}
        {bookmarks.length > 0 && (
          <span className="absolute -top-1 -right-1 w-4 h-4 bg-amber-500 text-black text-[10px] font-bold rounded-full flex items-center justify-center">
            {bookmarks.length > 9 ? "9+" : bookmarks.length}
          </span>
        )}
      </button>

      {/* Dropdown */}
      {isOpen && (
        <div className="absolute right-0 top-full mt-2 w-72 bg-slate-800 border border-slate-700 rounded-xl shadow-xl z-50 overflow-hidden">
          <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
            <h3 className="text-sm font-medium text-white">Miner Bookmarks</h3>
            {bookmarks.length > 0 && (
              <Link
                href="/miners"
                className="text-xs text-amber-400 hover:text-amber-300"
                onClick={() => setIsOpen(false)}
              >
                View All
              </Link>
            )}
          </div>

          <div className="max-h-80 overflow-y-auto">
            {bookmarks.length === 0 ? (
              <div className="px-4 py-8 text-center">
                <div className="text-3xl mb-2">⭐</div>
                <p className="text-slate-400 text-sm">No miners bookmarked yet</p>
                <p className="text-slate-500 text-xs mt-1">
                  Visit a miner profile and click the star to bookmark
                </p>
              </div>
            ) : (
              <div className="divide-y divide-slate-700/50">
                {bookmarks.map((bookmark) => (
                  <BookmarkItem
                    key={bookmark.pubkey}
                    bookmark={bookmark}
                    onRemove={() => removeBookmark(bookmark.pubkey)}
                    onClose={() => setIsOpen(false)}
                  />
                ))}
              </div>
            )}
          </div>

          {bookmarks.length > 0 && (
            <div className="px-4 py-2 border-t border-slate-700 bg-slate-850">
              <Link
                href="/miners"
                className="block w-full text-center text-xs text-slate-400 hover:text-white py-1"
                onClick={() => setIsOpen(false)}
              >
                See portfolio totals on Miners page →
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
  bookmark: MinerBookmark;
  onRemove: () => void;
  onClose: () => void;
}) {
  return (
    <div className="flex items-center gap-2 px-4 py-3 hover:bg-slate-700/50 transition-colors group">
      <Link
        href={`/miners/${bookmark.pubkey}`}
        className="flex-1 min-w-0"
        onClick={onClose}
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

      {/* Include in Totals indicator */}
      <div
        className={`w-2 h-2 rounded-full ${
          bookmark.includeInTotals ? "bg-green-500" : "bg-slate-600"
        }`}
        title={bookmark.includeInTotals ? "Included in totals" : "Not in totals"}
      />

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

