"use client";

import { useMemo, useState } from "react";
import {
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
  Brush,
  Cell,
  ReferenceLine,
} from "recharts";
import { colors, chartTheme, formatters, margins, seriesColors } from "./theme";
import { ChartTooltip, ValueFormatter } from "./ChartTooltip";

export interface BarSeries {
  key: string;
  name: string;
  color?: string;
  stackId?: string;
}

export interface StatsBarChartProps<T = Record<string, unknown>> {
  data: T[];
  series: BarSeries[];
  xKey: string;
  height?: number;
  xFormatter?: (value: number) => string;
  yFormatter?: ValueFormatter;
  showGrid?: boolean;
  showBrush?: boolean;
  showLegend?: boolean;
  stacked?: boolean;
  horizontal?: boolean;
  barSize?: number;
  referenceLines?: { y: number; label: string; color?: string }[];
  animate?: boolean;
  colorByValue?: boolean; // Color bars based on positive/negative value
}

/**
 * Bar chart component for displaying categorical or time-based data
 * Supports stacking, horizontal orientation, and value-based coloring
 */
export function StatsBarChart<T = Record<string, unknown>>({
  data,
  series,
  xKey,
  height = 300,
  xFormatter = (v: number) => String(v),
  yFormatter = formatters.number,
  showGrid = true,
  showBrush = false,
  showLegend = true,
  stacked = false,
  horizontal = false,
  barSize,
  referenceLines = [],
  animate = true,
  colorByValue = false,
}: StatsBarChartProps<T>) {
  const [hiddenSeries, setHiddenSeries] = useState<Set<string>>(new Set());

  // Toggle series visibility
  const toggleSeries = (key: string) => {
    setHiddenSeries((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  // Assign colors to series
  const seriesWithColors = useMemo(() => {
    return series.map((s, i) => ({
      ...s,
      color: s.color || seriesColors[i % seriesColors.length],
      stackId: stacked ? s.stackId || "stack" : undefined,
    }));
  }, [series, stacked]);

  // Get visible series
  const visibleSeries = seriesWithColors.filter((s) => !hiddenSeries.has(s.key));

  // Custom legend renderer
  const renderLegend = () => (
    <div className="flex flex-wrap items-center justify-center gap-4 mt-2">
      {seriesWithColors.map((s) => {
        const isHidden = hiddenSeries.has(s.key);
        return (
          <button
            key={s.key}
            onClick={() => toggleSeries(s.key)}
            className={`flex items-center gap-1.5 text-xs transition-opacity ${
              isHidden ? "opacity-40" : "opacity-100"
            }`}
          >
            <span
              className="w-2.5 h-2.5 rounded-sm"
              style={{ backgroundColor: s.color }}
            />
            <span style={{ color: chartTheme.legend.textColor }}>{s.name}</span>
          </button>
        );
      })}
    </div>
  );

  // Get bar color based on value
  const getBarColor = (value: number, defaultColor: string) => {
    if (!colorByValue) return defaultColor;
    if (value > 0) return colors.positive;
    if (value < 0) return colors.negative;
    return colors.textMuted;
  };

  // Chart margins
  const chartMargins = horizontal
    ? { ...margins.default, left: 80 }
    : showBrush
    ? margins.withLegend
    : margins.default;

  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height={showLegend ? "90%" : "100%"}>
        <BarChart
          data={data}
          layout={horizontal ? "vertical" : "horizontal"}
          margin={chartMargins}
          barCategoryGap="15%"
        >
          {showGrid && (
            <CartesianGrid
              strokeDasharray={chartTheme.grid.strokeDasharray}
              stroke={chartTheme.grid.stroke}
              strokeOpacity={chartTheme.grid.strokeOpacity}
              vertical={!horizontal}
              horizontal={horizontal}
            />
          )}

          {horizontal ? (
            <>
              <XAxis
                type="number"
                stroke={chartTheme.axis.stroke}
                tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
                tickFormatter={yFormatter}
                tickLine={false}
                axisLine={false}
              />
              <YAxis
                type="category"
                dataKey={xKey}
                stroke={chartTheme.axis.stroke}
                tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
                tickFormatter={xFormatter}
                tickLine={false}
                axisLine={{ strokeWidth: chartTheme.axis.strokeWidth }}
                width={80}
              />
            </>
          ) : (
            <>
              <XAxis
                dataKey={xKey}
                stroke={chartTheme.axis.stroke}
                tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
                tickFormatter={xFormatter}
                tickLine={false}
                axisLine={{ strokeWidth: chartTheme.axis.strokeWidth }}
                minTickGap={30}
              />
              <YAxis
                stroke={chartTheme.axis.stroke}
                tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
                tickFormatter={yFormatter}
                tickLine={false}
                axisLine={false}
                width={60}
              />
            </>
          )}

          <Tooltip
            content={
              <ChartTooltip
                labelFormatter={(label) =>
                  typeof label === "number" ? xFormatter(label) : String(label)
                }
                valueFormatter={yFormatter}
                showTotal={stacked}
              />
            }
          />

          {referenceLines.map((line, i) => (
            <ReferenceLine
              key={i}
              y={horizontal ? undefined : line.y}
              x={horizontal ? line.y : undefined}
              stroke={line.color || colors.textMuted}
              strokeDasharray="5 5"
              label={{
                value: line.label,
                fill: chartTheme.axis.tick.fill,
                fontSize: 10,
                position: horizontal ? "top" : "right",
              }}
            />
          ))}

          {visibleSeries.map((s) => (
            <Bar
              key={s.key}
              dataKey={s.key}
              name={s.name}
              fill={s.color}
              stackId={s.stackId}
              radius={[2, 2, 0, 0]}
              barSize={barSize}
              isAnimationActive={animate}
              animationDuration={chartTheme.animation.duration}
            >
              {colorByValue &&
                data.map((entry, index) => (
                  <Cell
                    key={`cell-${index}`}
                    fill={getBarColor((entry as Record<string, unknown>)[s.key] as number, s.color!)}
                  />
                ))}
            </Bar>
          ))}

          {showBrush && !horizontal && (
            <Brush
              dataKey={xKey}
              height={30}
              stroke={colors.grid}
              fill={colors.backgroundDark}
              tickFormatter={xFormatter}
            />
          )}
        </BarChart>
      </ResponsiveContainer>

      {showLegend && series.length > 1 && renderLegend()}
    </div>
  );
}

