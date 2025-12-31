"use client";

import { colors, chartTheme, formatters } from "./theme";

export type ValueFormatter = (value: number) => string;

export interface TooltipPayloadItem {
  name: string;
  value: number;
  color?: string;
  dataKey?: string;
  payload?: Record<string, unknown>;
}

export interface CustomTooltipProps {
  active?: boolean;
  payload?: TooltipPayloadItem[];
  label?: string | number;
  labelFormatter?: (label: string | number) => string;
  valueFormatter?: ValueFormatter;
  showTotal?: boolean;
}

/**
 * Custom tooltip component for Recharts with dark theme styling
 */
export function ChartTooltip({
  active,
  payload,
  label,
  labelFormatter,
  valueFormatter = formatters.number,
  showTotal = false,
}: CustomTooltipProps) {
  if (!active || !payload || payload.length === 0) {
    return null;
  }

  const formattedLabel = labelFormatter ? labelFormatter(label || "") : String(label);

  const total = showTotal
    ? payload.reduce((sum, item) => sum + (item.value || 0), 0)
    : null;

  return (
    <div
      className="rounded-lg border shadow-xl"
      style={{
        backgroundColor: chartTheme.tooltip.backgroundColor,
        borderColor: chartTheme.tooltip.borderColor,
        padding: chartTheme.tooltip.padding,
      }}
    >
      {/* Label/Timestamp */}
      <p
        className="text-xs font-medium mb-2 pb-2 border-b"
        style={{
          color: chartTheme.tooltip.labelColor,
          borderColor: colors.grid,
        }}
      >
        {formattedLabel}
      </p>

      {/* Values */}
      <div className="space-y-1.5">
        {payload.map((item, index) => (
          <div key={index} className="flex items-center justify-between gap-6">
            <div className="flex items-center gap-2">
              <span
                className="w-2.5 h-2.5 rounded-full"
                style={{ backgroundColor: item.color || colors.primary }}
              />
              <span
                className="text-xs"
                style={{ color: chartTheme.tooltip.labelColor }}
              >
                {item.name}
              </span>
            </div>
            <span
              className="text-sm font-mono font-medium"
              style={{ color: chartTheme.tooltip.textColor }}
            >
              {valueFormatter(item.value)}
            </span>
          </div>
        ))}
      </div>

      {/* Total */}
      {showTotal && total !== null && (
        <div
          className="mt-2 pt-2 border-t flex items-center justify-between"
          style={{ borderColor: colors.grid }}
        >
          <span
            className="text-xs font-medium"
            style={{ color: chartTheme.tooltip.labelColor }}
          >
            Total
          </span>
          <span
            className="text-sm font-mono font-bold"
            style={{ color: colors.primary }}
          >
            {valueFormatter(total)}
          </span>
        </div>
      )}
    </div>
  );
}

/**
 * Simple tooltip for single-value charts
 */
export function SimpleTooltip({
  active,
  payload,
  label,
  labelFormatter,
  valueFormatter = formatters.number,
  unit = "",
}: CustomTooltipProps & { unit?: string }) {
  if (!active || !payload || payload.length === 0) {
    return null;
  }

  const formattedLabel = labelFormatter ? labelFormatter(label || "") : String(label);
  const value = payload[0]?.value;
  const color = payload[0]?.color || colors.primary;

  return (
    <div
      className="rounded-lg border shadow-xl px-3 py-2"
      style={{
        backgroundColor: chartTheme.tooltip.backgroundColor,
        borderColor: chartTheme.tooltip.borderColor,
      }}
    >
      <p
        className="text-xs mb-1"
        style={{ color: chartTheme.tooltip.labelColor }}
      >
        {formattedLabel}
      </p>
      <p
        className="text-sm font-mono font-semibold"
        style={{ color }}
      >
        {valueFormatter(value)} {unit}
      </p>
    </div>
  );
}

