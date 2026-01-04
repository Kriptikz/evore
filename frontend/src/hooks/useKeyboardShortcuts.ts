"use client";

import { useEffect, useCallback } from "react";
import { useRouter } from "next/navigation";
import { useMinerBookmarks } from "./useMinerBookmarks";

interface UseKeyboardShortcutsOptions {
  currentPubkey?: string;
  onBookmarkToggle?: () => void;
}

export function useKeyboardShortcuts({ 
  currentPubkey, 
  onBookmarkToggle 
}: UseKeyboardShortcutsOptions = {}) {
  const router = useRouter();
  const { bookmarks, addBookmark, removeBookmark, isBookmarked } = useMinerBookmarks();

  const handleKeyDown = useCallback((e: KeyboardEvent) => {
    // Ignore if user is typing in an input
    const target = e.target as HTMLElement;
    if (
      target.tagName === "INPUT" ||
      target.tagName === "TEXTAREA" ||
      target.isContentEditable
    ) {
      return;
    }

    // B - Toggle bookmark (only on miner page)
    if (e.key === "b" || e.key === "B") {
      if (currentPubkey && onBookmarkToggle) {
        e.preventDefault();
        onBookmarkToggle();
      }
    }

    // Arrow keys - Navigate between bookmarked miners
    if ((e.key === "ArrowLeft" || e.key === "ArrowRight") && currentPubkey && bookmarks.length > 0) {
      const currentIndex = bookmarks.findIndex((b) => b.pubkey === currentPubkey);
      
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        if (currentIndex > 0) {
          router.push(`/miners/${bookmarks[currentIndex - 1].pubkey}`);
        } else if (currentIndex === -1 || currentIndex === 0) {
          // Go to last bookmark
          router.push(`/miners/${bookmarks[bookmarks.length - 1].pubkey}`);
        }
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        if (currentIndex !== -1 && currentIndex < bookmarks.length - 1) {
          router.push(`/miners/${bookmarks[currentIndex + 1].pubkey}`);
        } else {
          // Go to first bookmark
          router.push(`/miners/${bookmarks[0].pubkey}`);
        }
      }
    }

    // ? - Show keyboard shortcuts help (could be implemented later)
    if (e.key === "?") {
      // TODO: Show help modal
    }
  }, [currentPubkey, onBookmarkToggle, bookmarks, router]);

  useEffect(() => {
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [handleKeyDown]);

  return {
    bookmarks,
    addBookmark,
    removeBookmark,
    isBookmarked,
  };
}

