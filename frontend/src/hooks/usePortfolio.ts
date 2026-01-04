"use client";

import { useState, useEffect, useCallback } from "react";

const STORAGE_KEY = "ore-stats-portfolio";

export interface PortfolioEntry {
  pubkey: string;
  label?: string;
  addedAt: number;
}

interface UsePortfolioReturn {
  entries: PortfolioEntry[];
  addEntry: (pubkey: string, label?: string) => void;
  removeEntry: (pubkey: string) => void;
  updateEntry: (pubkey: string, updates: Partial<Omit<PortfolioEntry, "pubkey">>) => void;
  isInPortfolio: (pubkey: string) => boolean;
  getEntry: (pubkey: string) => PortfolioEntry | undefined;
}

function loadEntries(): PortfolioEntry[] {
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

function saveEntries(entries: PortfolioEntry[]): void {
  if (typeof window === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(entries));
  } catch (e) {
    console.error("Failed to save portfolio:", e);
  }
}

export function usePortfolio(): UsePortfolioReturn {
  const [entries, setEntries] = useState<PortfolioEntry[]>([]);
  const [isHydrated, setIsHydrated] = useState(false);

  // Load from localStorage on mount
  useEffect(() => {
    setEntries(loadEntries());
    setIsHydrated(true);
  }, []);

  // Listen for localStorage changes from other tabs/components
  useEffect(() => {
    function handleStorageChange(e: StorageEvent) {
      if (e.key === STORAGE_KEY) {
        setEntries(loadEntries());
      }
    }
    window.addEventListener("storage", handleStorageChange);
    return () => window.removeEventListener("storage", handleStorageChange);
  }, []);

  // Save to localStorage whenever entries change (after hydration)
  useEffect(() => {
    if (isHydrated) {
      saveEntries(entries);
      // Dispatch a custom event for same-tab updates
      window.dispatchEvent(new CustomEvent("portfolioUpdate"));
    }
  }, [entries, isHydrated]);

  // Listen for same-tab updates
  useEffect(() => {
    function handleCustomUpdate() {
      const stored = loadEntries();
      if (JSON.stringify(stored) !== JSON.stringify(entries)) {
        setEntries(stored);
      }
    }
    window.addEventListener("portfolioUpdate", handleCustomUpdate);
    return () => window.removeEventListener("portfolioUpdate", handleCustomUpdate);
  }, [entries]);

  const addEntry = useCallback((pubkey: string, label?: string) => {
    setEntries((prev) => {
      // Don't add duplicates
      if (prev.some((e) => e.pubkey === pubkey)) return prev;
      return [
        ...prev,
        {
          pubkey,
          label,
          addedAt: Date.now(),
        },
      ];
    });
  }, []);

  const removeEntry = useCallback((pubkey: string) => {
    setEntries((prev) => prev.filter((e) => e.pubkey !== pubkey));
  }, []);

  const updateEntry = useCallback(
    (pubkey: string, updates: Partial<Omit<PortfolioEntry, "pubkey">>) => {
      setEntries((prev) =>
        prev.map((e) => (e.pubkey === pubkey ? { ...e, ...updates } : e))
      );
    },
    []
  );

  const isInPortfolio = useCallback(
    (pubkey: string) => entries.some((e) => e.pubkey === pubkey),
    [entries]
  );

  const getEntry = useCallback(
    (pubkey: string) => entries.find((e) => e.pubkey === pubkey),
    [entries]
  );

  return {
    entries,
    addEntry,
    removeEntry,
    updateEntry,
    isInPortfolio,
    getEntry,
  };
}

