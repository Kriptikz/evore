"use client";

import Link from "next/link";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { truncateAddress } from "@/lib/format";

interface BookmarksQuickNavProps {
  currentPubkey?: string;
}

export function BookmarksQuickNav({ currentPubkey }: BookmarksQuickNavProps) {
  const { bookmarks } = useMinerBookmarks();

  if (bookmarks.length === 0) {
    return null;
  }

  // Get current index for navigation hints
  const currentIndex = bookmarks.findIndex((b) => b.pubkey === currentPubkey);

  return (
    <div className="flex items-center justify-between gap-4 flex-wrap">
      <div className="flex items-center gap-2 flex-wrap">
        <span className="text-xs text-slate-500">⭐ Bookmarks:</span>
        {bookmarks.map((bookmark, idx) => {
          const isCurrent = bookmark.pubkey === currentPubkey;
          // Show arrow hints for adjacent bookmarks
          const isPrev = currentIndex !== -1 && idx === currentIndex - 1;
          const isNext = currentIndex !== -1 && idx === currentIndex + 1;
          // Wrap around hints
          const isWrapPrev = currentIndex === 0 && idx === bookmarks.length - 1;
          const isWrapNext = currentIndex === bookmarks.length - 1 && idx === 0;
          
          return (
            <Link
              key={bookmark.pubkey}
              href={`/miners/${bookmark.pubkey}`}
              className={`inline-flex items-center gap-1.5 px-2.5 py-1 border rounded-lg text-xs transition-all ${
                isCurrent
                  ? "bg-amber-500/20 border-amber-500/50 text-amber-400"
                  : isPrev || isNext || isWrapPrev || isWrapNext
                  ? "bg-slate-800/80 border-amber-500/30 text-slate-300 hover:text-amber-400 hover:bg-slate-700"
                  : "bg-slate-800/80 border-slate-700 text-slate-300 hover:text-amber-400 hover:bg-slate-700 hover:border-amber-500/50"
              }`}
            >
              {(isPrev || isWrapPrev) && <span className="text-[10px] text-amber-500">←</span>}
              <span className="font-mono">
                {bookmark.label || truncateAddress(bookmark.pubkey, 4)}
              </span>
              {(isNext || isWrapNext) && <span className="text-[10px] text-amber-500">→</span>}
            </Link>
          );
        })}
      </div>
      
      {/* Keyboard hints */}
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
    </div>
  );
}

