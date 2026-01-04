"use client";

import { useState, useCallback } from "react";
import { useRouter } from "next/navigation";

interface MinerSearchBarProps {
  placeholder?: string;
  className?: string;
  showGoButton?: boolean;
  onNavigate?: (pubkey: string) => void;
}

export function MinerSearchBar({
  placeholder = "Search by miner address...",
  className = "",
  showGoButton = true,
  onNavigate,
}: MinerSearchBarProps) {
  const router = useRouter();
  const [search, setSearch] = useState("");

  const isValidPubkey = search.trim().length >= 32 && search.trim().length <= 44;

  const handleNavigate = useCallback(() => {
    const address = search.trim();
    if (!isValidPubkey) return;
    
    if (onNavigate) {
      onNavigate(address);
    } else {
      router.push(`/miners/${address}`);
    }
    setSearch("");
  }, [search, isValidPubkey, onNavigate, router]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleNavigate();
    }
  };

  const handleClear = () => {
    setSearch("");
  };

  return (
    <div className={`flex gap-2 ${className}`}>
      <div className="relative flex-1">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="w-full px-4 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50 pr-10"
        />
        {search && (
          <button
            onClick={handleClear}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-slate-500 hover:text-slate-300 transition-colors"
            title="Clear search"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        )}
      </div>
      {showGoButton && isValidPubkey && (
        <button
          onClick={handleNavigate}
          className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black font-medium rounded-lg transition-colors whitespace-nowrap"
        >
          View Profile →
        </button>
      )}
    </div>
  );
}

