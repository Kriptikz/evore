"use client";

import { useState, useCallback, useRef, useEffect } from "react";
import { useRouter } from "next/navigation";
import { api, MinerSnapshotEntry } from "@/lib/api";
import { useMinerBookmarks } from "@/hooks/useMinerBookmarks";
import { truncateAddress } from "@/lib/format";

interface MinerSearchBarProps {
  placeholder?: string;
  className?: string;
  showGoButton?: boolean;
  currentPubkey?: string;
  onNavigate?: (pubkey: string) => void;
}

export function MinerSearchBar({
  placeholder = "Search miners...",
  className = "",
  showGoButton = true,
  currentPubkey,
  onNavigate,
}: MinerSearchBarProps) {
  const router = useRouter();
  const { bookmarks } = useMinerBookmarks();
  const [search, setSearch] = useState("");
  const [isFocused, setIsFocused] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(-1);
  const [results, setResults] = useState<MinerSnapshotEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<NodeJS.Timeout>();

  const isValidPubkey = search.trim().length >= 32 && search.trim().length <= 44;

  // Check if a pubkey is bookmarked
  const isBookmarked = (pubkey: string) => bookmarks.some(b => b.pubkey === pubkey);
  const getBookmarkLabel = (pubkey: string) => bookmarks.find(b => b.pubkey === pubkey)?.label;

  // Filter out current miner from results
  const filteredResults = currentPubkey 
    ? results.filter(r => r.miner_pubkey !== currentPubkey)
    : results;

  const showDropdown = isFocused && (filteredResults.length > 0 || loading || (search.length >= 3 && !loading));

  // Close dropdown when clicking outside
  useEffect(() => {
    function handleClickOutside(event: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(event.target as Node)) {
        setIsFocused(false);
      }
    }
    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, []);

  // Search API with debounce
  useEffect(() => {
    if (debounceRef.current) {
      clearTimeout(debounceRef.current);
    }

    if (search.length < 3) {
      setResults([]);
      setLoading(false);
      return;
    }

    setLoading(true);
    debounceRef.current = setTimeout(async () => {
      try {
        const response = await api.getMinerSnapshots({
          search: search.trim(),
          limit: 10,
        });
        setResults(response.data);
      } catch (err) {
        console.error("Search failed:", err);
        setResults([]);
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
      }
    };
  }, [search]);

  const handleNavigate = useCallback((pubkey?: string) => {
    const address = pubkey || search.trim();
    if (!address || address.length < 32 || address.length > 44) return;
    
    if (onNavigate) {
      onNavigate(address);
    } else {
      router.push(`/miners/${address}`);
    }
    setSearch("");
    setIsFocused(false);
    setSelectedIndex(-1);
    setResults([]);
  }, [search, onNavigate, router]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!showDropdown || filteredResults.length === 0) {
      if (e.key === "Enter" && isValidPubkey) {
        handleNavigate();
      }
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setSelectedIndex((prev) => 
          prev < filteredResults.length - 1 ? prev + 1 : prev
        );
        break;
      case "ArrowUp":
        e.preventDefault();
        setSelectedIndex((prev) => (prev > 0 ? prev - 1 : -1));
        break;
      case "Enter":
        e.preventDefault();
        if (selectedIndex >= 0 && selectedIndex < filteredResults.length) {
          handleNavigate(filteredResults[selectedIndex].miner_pubkey);
        } else if (isValidPubkey) {
          handleNavigate();
        }
        break;
      case "Escape":
        setIsFocused(false);
        setSelectedIndex(-1);
        break;
    }
  };

  const handleClear = () => {
    setSearch("");
    setSelectedIndex(-1);
    setResults([]);
    inputRef.current?.focus();
  };

  return (
    <div className={`relative ${className}`} ref={containerRef}>
      <div className="flex gap-2">
        <div className="relative flex-1">
          <div className="absolute left-3 top-1/2 -translate-y-1/2 text-slate-500">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
            </svg>
          </div>
          <input
            ref={inputRef}
            type="text"
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setSelectedIndex(-1);
            }}
            onFocus={() => setIsFocused(true)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            className="w-full pl-10 pr-10 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white placeholder-slate-500 focus:outline-none focus:ring-2 focus:ring-amber-500/50"
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
            onClick={() => handleNavigate()}
            className="px-4 py-2 bg-amber-500 hover:bg-amber-600 text-black font-medium rounded-lg transition-colors whitespace-nowrap"
          >
            Go →
          </button>
        )}
      </div>

      {/* Search Results Dropdown */}
      {showDropdown && (
        <div className="absolute left-0 right-0 top-full mt-1 bg-slate-800 border border-slate-700 rounded-xl shadow-xl z-50 overflow-hidden">
          {loading ? (
            <div className="px-4 py-3 flex items-center gap-2 text-slate-400">
              <div className="w-4 h-4 border-2 border-slate-500 border-t-amber-500 rounded-full animate-spin" />
              <span className="text-sm">Searching...</span>
            </div>
          ) : filteredResults.length > 0 ? (
            <div className="max-h-72 overflow-y-auto">
              {filteredResults.map((miner, index) => {
                const bookmarked = isBookmarked(miner.miner_pubkey);
                const label = getBookmarkLabel(miner.miner_pubkey);
                
                return (
                  <button
                    key={miner.miner_pubkey}
                    onClick={() => handleNavigate(miner.miner_pubkey)}
                    onMouseEnter={() => setSelectedIndex(index)}
                    className={`w-full px-4 py-2.5 flex items-center gap-3 transition-colors text-left ${
                      selectedIndex === index
                        ? "bg-amber-500/20"
                        : "hover:bg-slate-700/50"
                    }`}
                  >
                    {bookmarked && (
                      <span className="text-amber-400 flex-shrink-0" title="Bookmarked">⭐</span>
                    )}
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-mono text-white truncate">
                          {truncateAddress(miner.miner_pubkey, 8)}
                        </span>
                        {label && (
                          <span className="text-xs text-amber-400 truncate">
                            ({label})
                          </span>
                        )}
                      </div>
                    </div>
                    <svg className="w-4 h-4 text-slate-500 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                    </svg>
                  </button>
                );
              })}
            </div>
          ) : search.length >= 3 ? (
            <div className="px-4 py-3 text-sm text-slate-400">
              No miners found matching "{search}"
            </div>
          ) : null}
          
          {/* Direct navigation option for valid pubkey */}
          {isValidPubkey && !filteredResults.some(r => r.miner_pubkey === search.trim()) && (
            <div className="border-t border-slate-700/50">
              <button
                onClick={() => handleNavigate()}
                className="w-full px-4 py-2.5 text-left hover:bg-slate-700/50 transition-colors flex items-center gap-2"
              >
                <span className="text-amber-400 text-sm">Go to address:</span>
                <span className="font-mono text-white text-sm">{truncateAddress(search.trim(), 8)}</span>
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
