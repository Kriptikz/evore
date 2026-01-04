"use client";

import { useState, useEffect, useCallback } from "react";

const STORAGE_KEY = "ore-stats-charts-bookmarks";

export interface ChartsBookmark {
  id: string;
  name: string;
  queryString: string;
  addedAt: number;
}

interface UseChartsBookmarksReturn {
  bookmarks: ChartsBookmark[];
  addBookmark: (name: string, queryString: string) => void;
  removeBookmark: (id: string) => void;
  updateBookmark: (id: string, updates: Partial<Omit<ChartsBookmark, "id">>) => void;
  getBookmark: (id: string) => ChartsBookmark | undefined;
}

function loadBookmarks(): ChartsBookmark[] {
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

function saveBookmarks(bookmarks: ChartsBookmark[]): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(bookmarks));
  } catch (e) {
    console.error("Failed to save chart bookmarks:", e);
  }
}

function generateId(): string {
  return `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
}

export function useChartsBookmarks(): UseChartsBookmarksReturn {
  const [bookmarks, setBookmarks] = useState<ChartsBookmark[]>([]);
  const [isHydrated, setIsHydrated] = useState(false);

  // Load bookmarks from localStorage on mount (client-side only)
  useEffect(() => {
    setBookmarks(loadBookmarks());
    setIsHydrated(true);
  }, []);

  // Listen for localStorage changes from other tabs/components
  useEffect(() => {
    function handleStorageChange(e: StorageEvent) {
      if (e.key === STORAGE_KEY) {
        setBookmarks(loadBookmarks());
      }
    }
    window.addEventListener("storage", handleStorageChange);
    return () => window.removeEventListener("storage", handleStorageChange);
  }, []);

  // Save to localStorage whenever bookmarks change (after hydration)
  useEffect(() => {
    if (isHydrated) {
      saveBookmarks(bookmarks);
      // Dispatch a custom event for same-tab updates
      window.dispatchEvent(new CustomEvent("chartsBookmarksUpdate"));
    }
  }, [bookmarks, isHydrated]);

  // Listen for same-tab updates
  useEffect(() => {
    function handleCustomUpdate() {
      const stored = loadBookmarks();
      // Only update if different
      if (JSON.stringify(stored) !== JSON.stringify(bookmarks)) {
        setBookmarks(stored);
      }
    }
    window.addEventListener("chartsBookmarksUpdate", handleCustomUpdate);
    return () => window.removeEventListener("chartsBookmarksUpdate", handleCustomUpdate);
  }, [bookmarks]);

  const addBookmark = useCallback((name: string, queryString: string) => {
    setBookmarks((prev) => [
      ...prev,
      {
        id: generateId(),
        name,
        queryString,
        addedAt: Date.now(),
      },
    ]);
  }, []);

  const removeBookmark = useCallback((id: string) => {
    setBookmarks((prev) => prev.filter((b) => b.id !== id));
  }, []);

  const updateBookmark = useCallback(
    (id: string, updates: Partial<Omit<ChartsBookmark, "id">>) => {
      setBookmarks((prev) =>
        prev.map((b) => (b.id === id ? { ...b, ...updates } : b))
      );
    },
    []
  );

  const getBookmark = useCallback(
    (id: string) => bookmarks.find((b) => b.id === id),
    [bookmarks]
  );

  return {
    bookmarks,
    addBookmark,
    removeBookmark,
    updateBookmark,
    getBookmark,
  };
}

