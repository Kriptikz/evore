"use client";

import Link from "next/link";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { truncateAddress } from "@/lib/format";

interface BookmarksQuickNavProps {
  currentPubkey?: string;
}

export function BookmarksQuickNav({ currentPubkey }: BookmarksQuickNavProps) {
  const { bookmarks } = useMinerBookmarks();

  // Get current index for navigation hints
  const currentIndex = currentPubkey
    ? bookmarks.findIndex((b) => b.pubkey === currentPubkey)
    : -1;
  const isCurrentBookmarked = currentIndex !== -1;

  // Filter out current miner for display
  const otherBookmarks = currentPubkey
    ? bookmarks.filter((b) => b.pubkey !== currentPubkey)
    : bookmarks;

  if (bookmarks.length === 0) {
    return null;
  }

  return (
    <div className="flex items-center justify-between gap-4 flex-wrap">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-xs text-slate-500">⭐ Bookmarks:</span>
        {otherBookmarks.length > 0 ? (
          otherBookmarks.map((bookmark, idx) => {
            const actualIdx = bookmarks.findIndex(b => b.pubkey === bookmark.pubkey);
            const isPrev = isCurrentBookmarked && actualIdx === currentIndex - 1;
            const isNext = isCurrentBookmarked && actualIdx === currentIndex + 1;
            
            return (
              <Link
                key={bookmark.pubkey}
                href={`/miners/${bookmark.pubkey}`}
                className={`inline-flex items-center gap-1.5 px-2.5 py-1 bg-slate-800/80 hover:bg-slate-700 border rounded-lg text-xs transition-all ${
                  isPrev || isNext
                    ? "border-amber-500/30 text-amber-400"
                    : "border-slate-700 hover:border-amber-500/50 text-slate-300 hover:text-amber-400"
                }`}
              >
                {isPrev && <span className="text-[10px] text-amber-500">←</span>}
                <span className="font-mono">
                  {bookmark.label || truncateAddress(bookmark.pubkey, 4)}
                </span>
                {isNext && <span className="text-[10px] text-amber-500">→</span>}
              </Link>
            );
          })
        ) : (
          <span className="text-xs text-slate-600">Current miner only</span>
        )}
      </div>
      
      {/* Keyboard hints */}
      {bookmarks.length > 0 && (
        <div className="flex items-center gap-2 text-[10px] text-slate-600">
          <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">B</kbd>
          <span>bookmark</span>
          {bookmarks.length > 1 && (
            <>
              <span className="mx-1">·</span>
              <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">←</kbd>
              <kbd className="px-1.5 py-0.5 bg-slate-800 rounded border border-slate-700">→</kbd>
              <span>navigate</span>
            </>
          )}
        </div>
      )}
    </div>
  );
}

