"use client";

import { useState, useEffect } from "react";

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

export function RoundRangeFilter({
  roundMin,
  roundMax,
  currentRoundId,
  onChange,
  compact = false,
}: RoundRangeFilterProps) {
  const [minInput, setMinInput] = useState<string>(roundMin?.toString() ?? "");
  const [maxInput, setMaxInput] = useState<string>(roundMax?.toString() ?? "");

  // Sync inputs when props change (e.g., from URL navigation)
  useEffect(() => {
    setMinInput(roundMin?.toString() ?? "");
    setMaxInput(roundMax?.toString() ?? "");
  }, [roundMin, roundMax]);

  const isAllTime = roundMin === undefined && roundMax === undefined;

  // Check if inputs differ from current applied values
  const minInputValue = minInput ? parseInt(minInput, 10) : undefined;
  const maxInputValue = maxInput ? parseInt(maxInput, 10) : undefined;
  const hasUnappliedChanges = 
    (minInputValue !== roundMin) || 
    (maxInputValue !== roundMax);

  // Quick preset values for min
  const presets = [
    { label: "60", offset: 60 },
    { label: "100", offset: 100 },
    { label: "1k", offset: 1000 },
  ];

  const handleAllTime = () => {
    setMinInput("");
    setMaxInput("");
    onChange(undefined, undefined);
  };

  const handlePresetClick = (offset: number) => {
    if (!currentRoundId) return;
    const newMin = currentRoundId - offset;
    setMinInput(newMin.toString());
    setMaxInput("");
    onChange(newMin, undefined);
  };

  const handleApply = () => {
    const min = minInput ? parseInt(minInput, 10) : undefined;
    const max = maxInput ? parseInt(maxInput, 10) : undefined;
    onChange(
      min !== undefined && !isNaN(min) ? min : undefined,
      max !== undefined && !isNaN(max) ? max : undefined
    );
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleApply();
    }
  };

  // Check if a preset is currently active
  const getActivePreset = (): number | null => {
    if (!currentRoundId || roundMin === undefined || roundMax !== undefined) return null;
    for (const preset of presets) {
      if (roundMin === currentRoundId - preset.offset) {
        return preset.offset;
      }
    }
    return null;
  };

  const activePreset = getActivePreset();

  if (compact) {
    return (
      <div className="flex items-center gap-3">
        {/* All Time button */}
        <button
          onClick={handleAllTime}
          className={`px-3 py-1.5 text-xs font-medium rounded-md transition-all ${
            isAllTime
              ? "bg-amber-500 text-black"
              : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white"
          }`}
        >
          All Time
        </button>

        <div className="h-4 w-px bg-slate-700" />

        {/* Range inputs with presets */}
        <div className="flex items-center gap-2">
          {/* Min input with presets above */}
          <div className="flex flex-col gap-1">
            <div className="flex gap-0.5">
              {presets.map(({ label, offset }) => (
                <button
                  key={offset}
                  onClick={() => handlePresetClick(offset)}
                  disabled={!currentRoundId}
                  className={`px-1.5 py-0.5 text-[10px] font-mono rounded transition-all ${
                    activePreset === offset
                      ? "bg-amber-500/30 text-amber-400"
                      : "bg-slate-800/50 text-slate-500 hover:bg-slate-700 hover:text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed"
                  }`}
                  title={currentRoundId ? `Round #${(currentRoundId - offset).toLocaleString()}` : undefined}
                >
                  {label}
                </button>
              ))}
            </div>
            <input
              type="number"
              placeholder="Min"
              value={minInput}
              onChange={(e) => setMinInput(e.target.value)}
              onKeyDown={handleKeyDown}
              className="w-24 px-2 py-1 text-xs font-mono bg-slate-800 border border-slate-700 rounded text-white placeholder-slate-500 focus:border-amber-500/50 focus:outline-none"
            />
          </div>

          <span className="text-slate-600 text-xs">→</span>

          {/* Max input */}
          <input
            type="number"
            placeholder="Max"
            value={maxInput}
            onChange={(e) => setMaxInput(e.target.value)}
            onKeyDown={handleKeyDown}
            className="w-24 px-2 py-1 text-xs font-mono bg-slate-800 border border-slate-700 rounded text-white placeholder-slate-500 focus:border-amber-500/50 focus:outline-none"
          />

          {/* Apply button - only show when there are unapplied changes */}
          {hasUnappliedChanges && (
            <button
              onClick={handleApply}
              className="px-2 py-1 text-xs font-medium bg-amber-500 text-black rounded hover:bg-amber-400 transition-colors"
            >
              Apply
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div className="bg-slate-900/50 border border-slate-800/50 rounded-xl p-4">
      <div className="flex items-center gap-4">
        {/* All Time button */}
        <button
          onClick={handleAllTime}
          className={`px-4 py-2 text-sm font-medium rounded-lg transition-all ${
            isAllTime
              ? "bg-gradient-to-r from-amber-500 to-orange-500 text-black"
              : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
          }`}
        >
          All Time
        </button>

        <div className="h-8 w-px bg-slate-700/50" />

        {/* Range selector */}
        <div className="flex items-center gap-3">
          {/* Min input with presets */}
          <div className="flex flex-col gap-1.5">
            <div className="flex gap-1">
              {presets.map(({ label, offset }) => (
                <button
                  key={offset}
                  onClick={() => handlePresetClick(offset)}
                  disabled={!currentRoundId}
                  className={`px-2 py-0.5 text-xs font-mono rounded transition-all ${
                    activePreset === offset
                      ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/30"
                      : "bg-slate-800/80 text-slate-500 hover:bg-slate-700 hover:text-slate-300 disabled:opacity-40 disabled:cursor-not-allowed"
                  }`}
                  title={currentRoundId ? `Set min to #${(currentRoundId - offset).toLocaleString()}` : "Loading..."}
                >
                  {label}
                </button>
              ))}
            </div>
            <input
              type="number"
              placeholder="Min round"
              value={minInput}
              onChange={(e) => setMinInput(e.target.value)}
              onKeyDown={handleKeyDown}
              className="w-32 px-3 py-2 text-sm font-mono bg-slate-800/50 border border-slate-700/50 rounded-lg text-white placeholder-slate-500 focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 focus:outline-none"
            />
          </div>

          <span className="text-slate-600 mt-5">→</span>

          {/* Max input */}
          <div className="flex flex-col gap-1.5">
            <div className="h-5" /> {/* Spacer to align with presets row */}
            <input
              type="number"
              placeholder="Max round"
              value={maxInput}
              onChange={(e) => setMaxInput(e.target.value)}
              onKeyDown={handleKeyDown}
              className="w-32 px-3 py-2 text-sm font-mono bg-slate-800/50 border border-slate-700/50 rounded-lg text-white placeholder-slate-500 focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 focus:outline-none"
            />
          </div>

          {/* Apply button - only show when there are unapplied changes */}
          {hasUnappliedChanges && (
            <button
              onClick={handleApply}
              className="mt-5 px-4 py-2 text-sm font-medium bg-gradient-to-r from-amber-500 to-orange-500 text-black rounded-lg hover:from-amber-400 hover:to-orange-400 transition-all"
            >
              Apply
            </button>
          )}
        </div>

        {/* Current filter indicator */}
        {!isAllTime && !hasUnappliedChanges && (
          <div className="ml-auto text-xs text-slate-500">
            <span className="text-amber-400/70 font-mono">
              #{roundMin?.toLocaleString() ?? "1"} → #{roundMax?.toLocaleString() ?? "now"}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
