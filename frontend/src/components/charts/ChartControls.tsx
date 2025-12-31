"use client";

import { useState } from "react";
import { ChartVariant } from "./TimeSeriesChart";

export type TimeRangePreset = "24h" | "7d" | "30d" | "90d" | "1y" | "custom";
export type ScaleType = "linear" | "log";

export interface ChartControlsProps {
  // Time range
  timeRange: TimeRangePreset;
  onTimeRangeChange: (range: TimeRangePreset) => void;
  customStartDate?: Date;
  customEndDate?: Date;
  onCustomDateChange?: (start: Date, end: Date) => void;
  
  // Chart type (for applicable charts)
  chartVariant?: ChartVariant;
  onChartVariantChange?: (variant: ChartVariant) => void;
  showChartVariant?: boolean;
  
  // Scale
  scale?: ScaleType;
  onScaleChange?: (scale: ScaleType) => void;
  showScale?: boolean;
  
  // Grid
  showGrid?: boolean;
  onShowGridChange?: (show: boolean) => void;
  showGridToggle?: boolean;
  
  // Export
  onExportPNG?: () => void;
  onExportCSV?: () => void;
  showExport?: boolean;
  
  // Compact mode
  compact?: boolean;
}

const TIME_RANGE_OPTIONS: { value: TimeRangePreset; label: string }[] = [
  { value: "24h", label: "24H" },
  { value: "7d", label: "7D" },
  { value: "30d", label: "30D" },
  { value: "90d", label: "90D" },
  { value: "1y", label: "1Y" },
];

/**
 * Chart controls panel with time range, scale, and export options
 */
export function ChartControls({
  timeRange,
  onTimeRangeChange,
  chartVariant,
  onChartVariantChange,
  showChartVariant = false,
  scale,
  onScaleChange,
  showScale = false,
  showGrid,
  onShowGridChange,
  showGridToggle = false,
  onExportPNG,
  onExportCSV,
  showExport = false,
  compact = false,
}: ChartControlsProps) {
  const [showExportMenu, setShowExportMenu] = useState(false);

  const buttonBase = `
    px-3 py-1.5 text-xs font-medium rounded-lg transition-colors
    focus:outline-none focus:ring-2 focus:ring-amber-500/50
  `;
  
  const buttonActive = `bg-amber-500/20 text-amber-400 border border-amber-500/30`;
  const buttonInactive = `bg-slate-800 text-slate-400 border border-slate-700 hover:text-slate-300 hover:border-slate-600`;

  return (
    <div className={`flex items-center gap-3 ${compact ? "flex-wrap" : ""}`}>
      {/* Time Range Selector */}
      <div className="flex items-center gap-1 bg-slate-800/50 rounded-lg p-1">
        {TIME_RANGE_OPTIONS.map((option) => (
          <button
            key={option.value}
            onClick={() => onTimeRangeChange(option.value)}
            className={`px-2.5 py-1 text-xs font-medium rounded-md transition-colors ${
              timeRange === option.value
                ? "bg-amber-500/20 text-amber-400"
                : "text-slate-400 hover:text-slate-300"
            }`}
          >
            {option.label}
          </button>
        ))}
      </div>

      {/* Divider */}
      {(showChartVariant || showScale || showGridToggle) && (
        <div className="w-px h-6 bg-slate-700" />
      )}

      {/* Chart Variant Toggle */}
      {showChartVariant && onChartVariantChange && (
        <div className="flex items-center gap-1">
          <button
            onClick={() => onChartVariantChange("area")}
            className={`${buttonBase} ${chartVariant === "area" ? buttonActive : buttonInactive}`}
            title="Area chart"
          >
            <svg className="w-4 h-4" fill="currentColor" viewBox="0 0 24 24">
              <path d="M3 18v-6l4-2 4 3 4-4 6 4v5H3z" opacity={0.3} />
              <path d="M3 18v-6l4-2 4 3 4-4 6 4v5H3zm18-8l-6-4-4 4-4-3-4 2v6h18v-5z" />
            </svg>
          </button>
          <button
            onClick={() => onChartVariantChange("line")}
            className={`${buttonBase} ${chartVariant === "line" ? buttonActive : buttonInactive}`}
            title="Line chart"
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M3 17l4-4 4 4 4-8 6 4" />
            </svg>
          </button>
        </div>
      )}

      {/* Scale Toggle */}
      {showScale && onScaleChange && (
        <div className="flex items-center gap-1">
          <button
            onClick={() => onScaleChange("linear")}
            className={`${buttonBase} ${scale === "linear" ? buttonActive : buttonInactive}`}
            title="Linear scale"
          >
            Lin
          </button>
          <button
            onClick={() => onScaleChange("log")}
            className={`${buttonBase} ${scale === "log" ? buttonActive : buttonInactive}`}
            title="Logarithmic scale"
          >
            Log
          </button>
        </div>
      )}

      {/* Grid Toggle */}
      {showGridToggle && onShowGridChange && (
        <button
          onClick={() => onShowGridChange(!showGrid)}
          className={`${buttonBase} ${showGrid ? buttonActive : buttonInactive}`}
          title="Toggle grid"
        >
          <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" d="M4 6h16M4 12h16M4 18h16M8 4v16M16 4v16" />
          </svg>
        </button>
      )}

      {/* Spacer */}
      {showExport && <div className="flex-1" />}

      {/* Export Menu */}
      {showExport && (onExportPNG || onExportCSV) && (
        <div className="relative">
          <button
            onClick={() => setShowExportMenu(!showExportMenu)}
            className={`${buttonBase} ${buttonInactive} flex items-center gap-1.5`}
          >
            <svg className="w-4 h-4" fill="none" stroke="currentColor" strokeWidth={2} viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
            </svg>
            Export
          </button>

          {showExportMenu && (
            <>
              <div
                className="fixed inset-0 z-10"
                onClick={() => setShowExportMenu(false)}
              />
              <div className="absolute right-0 mt-2 w-32 bg-slate-800 border border-slate-700 rounded-lg shadow-xl z-20 overflow-hidden">
                {onExportPNG && (
                  <button
                    onClick={() => {
                      onExportPNG();
                      setShowExportMenu(false);
                    }}
                    className="w-full px-3 py-2 text-left text-sm text-slate-300 hover:bg-slate-700 transition-colors"
                  >
                    Export PNG
                  </button>
                )}
                {onExportCSV && (
                  <button
                    onClick={() => {
                      onExportCSV();
                      setShowExportMenu(false);
                    }}
                    className="w-full px-3 py-2 text-left text-sm text-slate-300 hover:bg-slate-700 transition-colors"
                  >
                    Export CSV
                  </button>
                )}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}

/**
 * Minimal time range selector for use within chart cards
 */
export function TimeRangeSelector({
  value,
  onChange,
  options = TIME_RANGE_OPTIONS,
}: {
  value: TimeRangePreset;
  onChange: (value: TimeRangePreset) => void;
  options?: { value: TimeRangePreset; label: string }[];
}) {
  return (
    <div className="flex items-center gap-0.5 bg-slate-800/50 rounded-lg p-0.5">
      {options.map((option) => (
        <button
          key={option.value}
          onClick={() => onChange(option.value)}
          className={`px-2 py-1 text-xs font-medium rounded transition-colors ${
            value === option.value
              ? "bg-amber-500/20 text-amber-400"
              : "text-slate-500 hover:text-slate-400"
          }`}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

