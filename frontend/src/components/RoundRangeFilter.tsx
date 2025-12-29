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
  const [showCustom, setShowCustom] = useState(false);

  // Determine which quick select is active
  const getActiveQuickSelect = useCallback((): QuickSelect => {
    if (roundMin === undefined && roundMax === undefined) {
      return "all";
    }
    if (currentRoundId && roundMax === undefined) {
      const minThreshold60 = currentRoundId - 60;
      const minThreshold100 = currentRoundId - 100;
      const minThreshold1000 = currentRoundId - 1000;
      
      if (roundMin === minThreshold60) return "last_60";
      if (roundMin === minThreshold100) return "last_100";
      if (roundMin === minThreshold1000) return "last_1000";
    }
    return "custom";
  }, [roundMin, roundMax, currentRoundId]);

  const [activeSelect, setActiveSelect] = useState<QuickSelect>(getActiveQuickSelect);

  // Sync activeSelect when props change
  useEffect(() => {
    const newActive = getActiveQuickSelect();
    setActiveSelect(newActive);
    if (newActive === "custom") {
      setShowCustom(true);
    }
  }, [getActiveQuickSelect]);

  // Update custom inputs when props change
  useEffect(() => {
    setCustomMin(roundMin?.toString() ?? "");
    setCustomMax(roundMax?.toString() ?? "");
  }, [roundMin, roundMax]);

  const handleQuickSelect = (select: QuickSelect) => {
    setActiveSelect(select);
    
    switch (select) {
      case "all":
        setShowCustom(false);
        onChange(undefined, undefined);
        break;
      case "last_60":
        setShowCustom(false);
        if (currentRoundId) {
          onChange(currentRoundId - 60, undefined);
        }
        break;
      case "last_100":
        setShowCustom(false);
        if (currentRoundId) {
          onChange(currentRoundId - 100, undefined);
        }
        break;
      case "last_1000":
        setShowCustom(false);
        if (currentRoundId) {
          onChange(currentRoundId - 1000, undefined);
        }
        break;
      case "custom":
        setShowCustom(true);
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
    setShowCustom(false);
    setActiveSelect("all");
  };

  const quickButtons = [
    { key: "all" as QuickSelect, label: "All Time" },
    { key: "last_60" as QuickSelect, label: "Last 60" },
    { key: "last_100" as QuickSelect, label: "Last 100" },
    { key: "last_1000" as QuickSelect, label: "Last 1000" },
    { key: "custom" as QuickSelect, label: "Custom" },
  ];

  if (compact) {
    return (
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-sm text-zinc-400">Rounds:</span>
        <div className="flex flex-wrap gap-1">
          {quickButtons.map(({ key, label }) => (
            <button
              key={key}
              onClick={() => handleQuickSelect(key)}
              className={`px-2 py-1 text-xs rounded transition-colors ${
                activeSelect === key
                  ? "bg-amber-500 text-black font-medium"
                  : "bg-zinc-800 text-zinc-300 hover:bg-zinc-700"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
        {showCustom && (
          <div className="flex items-center gap-1">
            <input
              type="number"
              placeholder="Min"
              value={customMin}
              onChange={(e) => setCustomMin(e.target.value)}
              className="w-20 px-2 py-1 text-xs bg-zinc-800 border border-zinc-700 rounded text-white"
            />
            <span className="text-zinc-500">-</span>
            <input
              type="number"
              placeholder="Max"
              value={customMax}
              onChange={(e) => setCustomMax(e.target.value)}
              className="w-20 px-2 py-1 text-xs bg-zinc-800 border border-zinc-700 rounded text-white"
            />
            <button
              onClick={handleApplyCustom}
              className="px-2 py-1 text-xs bg-amber-500 text-black rounded hover:bg-amber-400"
            >
              Apply
            </button>
          </div>
        )}
      </div>
    );
  }

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-4">
      <div className="flex flex-wrap items-center gap-3">
        <span className="text-sm font-medium text-zinc-300">Round Range:</span>
        
        {/* Quick select buttons */}
        <div className="flex flex-wrap gap-2">
          {quickButtons.map(({ key, label }) => (
            <button
              key={key}
              onClick={() => handleQuickSelect(key)}
              className={`px-3 py-1.5 text-sm rounded-md transition-colors ${
                activeSelect === key
                  ? "bg-amber-500 text-black font-medium"
                  : "bg-zinc-800 text-zinc-300 hover:bg-zinc-700 border border-zinc-700"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Custom range inputs */}
        {showCustom && (
          <div className="flex items-center gap-2 ml-4">
            <input
              type="number"
              placeholder="Min Round"
              value={customMin}
              onChange={(e) => setCustomMin(e.target.value)}
              className="w-28 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded-md text-white placeholder-zinc-500 focus:border-amber-500 focus:outline-none"
            />
            <span className="text-zinc-500">to</span>
            <input
              type="number"
              placeholder="Max Round"
              value={customMax}
              onChange={(e) => setCustomMax(e.target.value)}
              className="w-28 px-3 py-1.5 text-sm bg-zinc-800 border border-zinc-700 rounded-md text-white placeholder-zinc-500 focus:border-amber-500 focus:outline-none"
            />
            <button
              onClick={handleApplyCustom}
              className="px-3 py-1.5 text-sm bg-amber-500 text-black rounded-md hover:bg-amber-400 font-medium"
            >
              Apply
            </button>
            <button
              onClick={handleClearCustom}
              className="px-3 py-1.5 text-sm bg-zinc-700 text-zinc-300 rounded-md hover:bg-zinc-600"
            >
              Clear
            </button>
          </div>
        )}
      </div>

      {/* Show current filter info */}
      {(roundMin !== undefined || roundMax !== undefined) && (
        <div className="mt-2 text-xs text-zinc-500">
          Filtering: {roundMin !== undefined ? `Round ${roundMin}` : "Start"} 
          {" → "}
          {roundMax !== undefined ? `Round ${roundMax}` : "Latest"}
        </div>
      )}
    </div>
  );
}

