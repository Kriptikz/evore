"use client";

import { useState, useEffect, useCallback } from "react";

const STORAGE_KEY = "ore-stats-miner-bookmarks";

export interface MinerBookmark {
  pubkey: string;
  label?: string;
  includeInTotals: boolean;
  addedAt: number;
}

interface UseMinerBookmarksReturn {
  bookmarks: MinerBookmark[];
  addBookmark: (pubkey: string, label?: string) => void;
  removeBookmark: (pubkey: string) => void;
  updateBookmark: (pubkey: string, updates: Partial<Omit<MinerBookmark, "pubkey">>) => void;
  toggleIncludeInTotals: (pubkey: string) => void;
  isBookmarked: (pubkey: string) => boolean;
  getBookmark: (pubkey: string) => MinerBookmark | undefined;
}

function loadBookmarks(): MinerBookmark[] {
  if (typeof window === "undefined") return [];
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return [];
    const parsed = JSON.parse(stored);
    if (!Array.isArray(parsed)) return [];
    return parsed;
  } catch {
    return [];
  }
}

function saveBookmarks(bookmarks: MinerBookmark[]): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(bookmarks));
  } catch (e) {
    console.error("Failed to save bookmarks:", e);
  }
}

export function useMinerBookmarks(): UseMinerBookmarksReturn {
  const [bookmarks, setBookmarks] = useState<MinerBookmark[]>([]);
  const [isHydrated, setIsHydrated] = useState(false);

  // Load bookmarks from localStorage on mount (client-side only)
  useEffect(() => {
    setBookmarks(loadBookmarks());
    setIsHydrated(true);
  }, []);

  // Save to localStorage whenever bookmarks change (after hydration)
  useEffect(() => {
    if (isHydrated) {
      saveBookmarks(bookmarks);
    }
  }, [bookmarks, isHydrated]);

  const addBookmark = useCallback((pubkey: string, label?: string) => {
    setBookmarks((prev) => {
      // Don't add duplicates
      if (prev.some((b) => b.pubkey === pubkey)) return prev;
      return [
        ...prev,
        {
          pubkey,
          label,
          includeInTotals: true,
          addedAt: Date.now(),
        },
      ];
    });
  }, []);

  const removeBookmark = useCallback((pubkey: string) => {
    setBookmarks((prev) => prev.filter((b) => b.pubkey !== pubkey));
  }, []);

  const updateBookmark = useCallback(
    (pubkey: string, updates: Partial<Omit<MinerBookmark, "pubkey">>) => {
      setBookmarks((prev) =>
        prev.map((b) => (b.pubkey === pubkey ? { ...b, ...updates } : b))
      );
    },
    []
  );

  const toggleIncludeInTotals = useCallback((pubkey: string) => {
    setBookmarks((prev) =>
      prev.map((b) =>
        b.pubkey === pubkey ? { ...b, includeInTotals: !b.includeInTotals } : b
      )
    );
  }, []);

  const isBookmarked = useCallback(
    (pubkey: string) => bookmarks.some((b) => b.pubkey === pubkey),
    [bookmarks]
  );

  const getBookmark = useCallback(
    (pubkey: string) => bookmarks.find((b) => b.pubkey === pubkey),
    [bookmarks]
  );

  return {
    bookmarks,
    addBookmark,
    removeBookmark,
    updateBookmark,
    toggleIncludeInTotals,
    isBookmarked,
    getBookmark,
  };
}

