"use client";

import { useState, useCallback, useEffect } from "react";

interface RoundRangeFilterProps {
  /** Current minimum round (inclusive) */
  roundMin?: number;
  /** Current maximum round (inclusive) */
  roundMax?: number;
  /** Current/latest round ID for calculating "Last X" options */
  currentRoundId?: number;
  /** Callback when range changes */
  onChange: (min?: number, max?: number) => void;
  /** Optional: show compact version */
  compact?: boolean;
}

type QuickSelect = "all" | "last_60" | "last_100" | "last_1000" | "custom";

export function RoundRangeFilter({
  roundMin,
  roundMax,
  currentRoundId,
  onChange,
  compact = false,
}: RoundRangeFilterProps) {
  const [customMin, setCustomMin] = useState<string>(roundMin?.toString() ?? "");
  const [customMax, setCustomMax] = useState<string>(roundMax?.toString() ?? "");
  const [showCustomInputs, setShowCustomInputs] = useState(false);

  // Calculate the actual round numbers for quick filters
  const quickFilterRounds = {
    last_60: currentRoundId ? currentRoundId - 60 : undefined,
    last_100: currentRoundId ? currentRoundId - 100 : undefined,
    last_1000: currentRoundId ? currentRoundId - 1000 : undefined,
  };

  // Determine which quick select is active
  const getActiveQuickSelect = useCallback((): QuickSelect => {
    if (roundMin === undefined && roundMax === undefined) {
      return "all";
    }
    if (currentRoundId && roundMax === undefined) {
      if (roundMin === quickFilterRounds.last_60) return "last_60";
      if (roundMin === quickFilterRounds.last_100) return "last_100";
      if (roundMin === quickFilterRounds.last_1000) return "last_1000";
    }
    return "custom";
  }, [roundMin, roundMax, currentRoundId, quickFilterRounds.last_60, quickFilterRounds.last_100, quickFilterRounds.last_1000]);

  const activeSelect = getActiveQuickSelect();

  // Update custom inputs when props change
  useEffect(() => {
    setCustomMin(roundMin?.toString() ?? "");
    setCustomMax(roundMax?.toString() ?? "");
  }, [roundMin, roundMax]);

  // Show custom inputs if we have a custom filter active
  useEffect(() => {
    if (activeSelect === "custom" && (roundMin !== undefined || roundMax !== undefined)) {
      setShowCustomInputs(true);
    }
  }, [activeSelect, roundMin, roundMax]);

  const handleQuickSelect = (select: QuickSelect) => {
    switch (select) {
      case "all":
        setShowCustomInputs(false);
        onChange(undefined, undefined);
        break;
      case "last_60":
        setShowCustomInputs(false);
        if (currentRoundId) {
          onChange(currentRoundId - 60, undefined);
        }
        break;
      case "last_100":
        setShowCustomInputs(false);
        if (currentRoundId) {
          onChange(currentRoundId - 100, undefined);
        }
        break;
      case "last_1000":
        setShowCustomInputs(false);
        if (currentRoundId) {
          onChange(currentRoundId - 1000, undefined);
        }
        break;
      case "custom":
        setShowCustomInputs(true);
        break;
    }
  };

  const handleApplyCustom = () => {
    const min = customMin ? parseInt(customMin, 10) : undefined;
    const max = customMax ? parseInt(customMax, 10) : undefined;
    onChange(
      min !== undefined && !isNaN(min) ? min : undefined,
      max !== undefined && !isNaN(max) ? max : undefined
    );
  };

  const handleClearCustom = () => {
    setCustomMin("");
    setCustomMax("");
    onChange(undefined, undefined);
    setShowCustomInputs(false);
  };

  // Format the current filter description
  const getFilterDescription = () => {
    if (roundMin === undefined && roundMax === undefined) return null;
    
    const minStr = roundMin !== undefined ? `#${roundMin.toLocaleString()}` : "Start";
    const maxStr = roundMax !== undefined ? `#${roundMax.toLocaleString()}` : "Now";
    
    if (activeSelect !== "custom" && activeSelect !== "all") {
      const count = activeSelect === "last_60" ? 60 : activeSelect === "last_100" ? 100 : 1000;
      return `Showing last ${count} rounds (${minStr} → ${maxStr})`;
    }
    
    return `${minStr} → ${maxStr}`;
  };

  if (compact) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500 uppercase tracking-wide">From:</span>
        
        {/* Quick filter chips */}
        <div className="flex flex-wrap gap-1">
          <button
            onClick={() => handleQuickSelect("all")}
            className={`px-2.5 py-1 text-xs rounded-full transition-all ${
              activeSelect === "all"
                ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/50"
                : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300"
            }`}
          >
            All
          </button>
          
          {[
            { key: "last_60" as const, count: 60 },
            { key: "last_100" as const, count: 100 },
            { key: "last_1000" as const, count: 1000 },
          ].map(({ key, count }) => (
            <button
              key={key}
              onClick={() => handleQuickSelect(key)}
              disabled={!currentRoundId}
              className={`px-2.5 py-1 text-xs rounded-full transition-all ${
                activeSelect === key
                  ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/50"
                  : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300 disabled:opacity-50 disabled:cursor-not-allowed"
              }`}
              title={quickFilterRounds[key] ? `Round #${quickFilterRounds[key].toLocaleString()}+` : undefined}
            >
              Last {count}
            </button>
          ))}
          
          <button
            onClick={() => handleQuickSelect("custom")}
            className={`px-2.5 py-1 text-xs rounded-full transition-all ${
              activeSelect === "custom" || showCustomInputs
                ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/50"
                : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300"
            }`}
          >
            Custom
          </button>
        </div>

        {/* Custom inputs */}
        {showCustomInputs && (
          <div className="flex items-center gap-1.5">
            <input
              type="number"
              placeholder="Min"
              value={customMin}
              onChange={(e) => setCustomMin(e.target.value)}
              className="w-20 px-2 py-1 text-xs bg-slate-800 border border-slate-700 rounded text-white placeholder-slate-500 focus:border-amber-500 focus:outline-none"
            />
            <span className="text-slate-600">→</span>
            <input
              type="number"
              placeholder="Max"
              value={customMax}
              onChange={(e) => setCustomMax(e.target.value)}
              className="w-20 px-2 py-1 text-xs bg-slate-800 border border-slate-700 rounded text-white placeholder-slate-500 focus:border-amber-500 focus:outline-none"
            />
            <button
              onClick={handleApplyCustom}
              className="px-2 py-1 text-xs bg-amber-500 text-black rounded hover:bg-amber-400 font-medium"
            >
              Go
            </button>
            {(roundMin !== undefined || roundMax !== undefined) && (
              <button
                onClick={handleClearCustom}
                className="px-2 py-1 text-xs text-slate-400 hover:text-white"
              >
                ✕
              </button>
            )}
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="bg-slate-900/50 border border-slate-800/50 rounded-xl p-4">
      <div className="flex flex-col gap-3">
        {/* Header with current filter */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <svg className="w-4 h-4 text-slate-500" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
            </svg>
            <span className="text-sm font-medium text-slate-300">Round Filter</span>
          </div>
          
          {getFilterDescription() && (
            <span className="text-xs text-amber-400/80 font-mono">
              {getFilterDescription()}
            </span>
          )}
        </div>

        {/* Quick filters - show as "Starting from" with round numbers */}
        <div className="flex flex-wrap items-center gap-2">
          <button
            onClick={() => handleQuickSelect("all")}
            className={`px-3 py-1.5 text-sm rounded-lg transition-all ${
              activeSelect === "all"
                ? "bg-gradient-to-r from-amber-500/20 to-orange-500/20 text-amber-400 ring-1 ring-amber-500/30 font-medium"
                : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300 border border-slate-700/50"
            }`}
          >
            All Time
          </button>

          <div className="h-4 w-px bg-slate-700/50" />
          
          <span className="text-xs text-slate-500 uppercase tracking-wide">Last:</span>
          
          {[
            { key: "last_60" as const, count: 60 },
            { key: "last_100" as const, count: 100 },
            { key: "last_1000" as const, count: 1000 },
          ].map(({ key, count }) => {
            const roundNum = quickFilterRounds[key];
            return (
              <button
                key={key}
                onClick={() => handleQuickSelect(key)}
                disabled={!currentRoundId}
                className={`group relative px-3 py-1.5 text-sm rounded-lg transition-all ${
                  activeSelect === key
                    ? "bg-gradient-to-r from-amber-500/20 to-orange-500/20 text-amber-400 ring-1 ring-amber-500/30 font-medium"
                    : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300 border border-slate-700/50 disabled:opacity-50 disabled:cursor-not-allowed"
                }`}
              >
                <span>{count}</span>
                {roundNum && activeSelect !== key && (
                  <span className="ml-1.5 text-xs text-slate-500 font-mono">
                    #{roundNum.toLocaleString()}+
                  </span>
                )}
              </button>
            );
          })}

          <div className="h-4 w-px bg-slate-700/50" />

          <button
            onClick={() => handleQuickSelect("custom")}
            className={`px-3 py-1.5 text-sm rounded-lg transition-all flex items-center gap-1.5 ${
              showCustomInputs
                ? "bg-gradient-to-r from-amber-500/20 to-orange-500/20 text-amber-400 ring-1 ring-amber-500/30 font-medium"
                : "bg-slate-800/50 text-slate-400 hover:bg-slate-700/50 hover:text-slate-300 border border-slate-700/50"
            }`}
          >
            <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
            </svg>
            Custom Range
          </button>
        </div>

        {/* Custom range inputs - slide in when active */}
        {showCustomInputs && (
          <div className="flex items-center gap-3 pt-2 border-t border-slate-800/50">
            <div className="flex items-center gap-2">
              <label className="text-xs text-slate-500 uppercase tracking-wide">From:</label>
              <input
                type="number"
                placeholder="Start round"
                value={customMin}
                onChange={(e) => setCustomMin(e.target.value)}
                className="w-32 px-3 py-2 text-sm bg-slate-800/50 border border-slate-700/50 rounded-lg text-white placeholder-slate-500 focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 focus:outline-none font-mono"
              />
            </div>
            
            <span className="text-slate-600">→</span>
            
            <div className="flex items-center gap-2">
              <label className="text-xs text-slate-500 uppercase tracking-wide">To:</label>
              <input
                type="number"
                placeholder="End round"
                value={customMax}
                onChange={(e) => setCustomMax(e.target.value)}
                className="w-32 px-3 py-2 text-sm bg-slate-800/50 border border-slate-700/50 rounded-lg text-white placeholder-slate-500 focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 focus:outline-none font-mono"
              />
            </div>

            <div className="flex items-center gap-2 ml-auto">
              <button
                onClick={handleApplyCustom}
                className="px-4 py-2 text-sm bg-gradient-to-r from-amber-500 to-orange-500 text-black rounded-lg hover:from-amber-400 hover:to-orange-400 font-medium transition-all"
              >
                Apply
              </button>
              <button
                onClick={handleClearCustom}
                className="px-4 py-2 text-sm bg-slate-800/50 text-slate-400 rounded-lg hover:bg-slate-700/50 hover:text-slate-300 border border-slate-700/50 transition-all"
              >
                Clear
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
