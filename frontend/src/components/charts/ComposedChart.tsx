"use client";

import { useMemo, useState } from "react";
import {
  ComposedChart as RechartsComposedChart,
  Bar,
  Line,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
  Brush,
  ReferenceLine,
} from "recharts";
import { colors, chartTheme, formatters, margins, seriesColors } from "./theme";
import { ChartTooltip, ValueFormatter } from "./ChartTooltip";

export type SeriesType = "bar" | "line" | "area";

export interface ComposedSeries {
  key: string;
  name: string;
  type: SeriesType;
  color?: string;
  yAxisId?: "left" | "right";
  stackId?: string;
}

export interface ComposedChartProps<T = Record<string, unknown>> {
  data: T[];
  series: ComposedSeries[];
  xKey: string;
  height?: number;
  xFormatter?: (value: number) => string;
  yFormatter?: ValueFormatter;
  yFormatterRight?: ValueFormatter;
  showGrid?: boolean;
  showBrush?: boolean;
  showLegend?: boolean;
  dualAxis?: boolean;
  referenceLines?: { y: number; label: string; color?: string; yAxisId?: "left" | "right" }[];
  animate?: boolean;
}

/**
 * Composed chart that can mix bars, lines, and areas
 * Perfect for showing daily values (bars) with cumulative trends (line)
 */
export function ComposedChart<T = Record<string, unknown>>({
  data,
  series,
  xKey,
  height = 300,
  xFormatter = formatters.dateTime,
  yFormatter = formatters.number,
  yFormatterRight,
  showGrid = true,
  showBrush = false,
  showLegend = true,
  dualAxis = false,
  referenceLines = [],
  animate = true,
}: ComposedChartProps<T>) {
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
    }));
  }, [series]);

  // Get visible series
  const visibleSeries = seriesWithColors.filter((s) => !hiddenSeries.has(s.key));

  // Gradient definitions for area charts
  const gradientDefs = (
    <defs>
      {seriesWithColors
        .filter((s) => s.type === "area")
        .map((s) => (
          <linearGradient key={s.key} id={`gradient-${s.key}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor={s.color} stopOpacity={0.4} />
            <stop offset="100%" stopColor={s.color} stopOpacity={0.05} />
          </linearGradient>
        ))}
    </defs>
  );

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
              className={`w-2.5 h-2.5 ${s.type === "bar" ? "rounded-sm" : "rounded-full"}`}
              style={{ backgroundColor: s.color }}
            />
            <span style={{ color: chartTheme.legend.textColor }}>{s.name}</span>
          </button>
        );
      })}
    </div>
  );

  // Render series based on type
  const renderSeries = (s: (typeof seriesWithColors)[0]) => {
    const commonProps = {
      key: s.key,
      dataKey: s.key,
      name: s.name,
      yAxisId: s.yAxisId || "left",
      isAnimationActive: animate,
      animationDuration: chartTheme.animation.duration,
    };

    switch (s.type) {
      case "bar":
        return (
          <Bar
            {...commonProps}
            fill={s.color}
            stackId={s.stackId}
            radius={[2, 2, 0, 0]}
            barSize={undefined}
          />
        );
      case "line":
        return (
          <Line
            {...commonProps}
            stroke={s.color}
            strokeWidth={2}
            dot={false}
            type="monotone"
          />
        );
      case "area":
        return (
          <Area
            {...commonProps}
            stroke={s.color}
            strokeWidth={2}
            fill={`url(#gradient-${s.key})`}
            type="monotone"
          />
        );
    }
  };

  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height={showLegend ? "90%" : "100%"}>
        <RechartsComposedChart
          data={data}
          margin={showBrush ? margins.withLegend : margins.default}
        >
          {gradientDefs}

          {showGrid && (
            <CartesianGrid
              strokeDasharray={chartTheme.grid.strokeDasharray}
              stroke={chartTheme.grid.stroke}
              strokeOpacity={chartTheme.grid.strokeOpacity}
              vertical={false}
            />
          )}

          <XAxis
            dataKey={xKey}
            stroke={chartTheme.axis.stroke}
            tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
            tickFormatter={xFormatter}
            tickLine={false}
            axisLine={{ strokeWidth: chartTheme.axis.strokeWidth }}
            minTickGap={50}
          />

          <YAxis
            yAxisId="left"
            stroke={chartTheme.axis.stroke}
            tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
            tickFormatter={yFormatter}
            tickLine={false}
            axisLine={false}
            width={60}
          />

          {dualAxis && (
            <YAxis
              yAxisId="right"
              orientation="right"
              stroke={chartTheme.axis.stroke}
              tick={{ fill: chartTheme.axis.tick.fill, fontSize: chartTheme.axis.tick.fontSize }}
              tickFormatter={yFormatterRight || yFormatter}
              tickLine={false}
              axisLine={false}
              width={60}
            />
          )}

          <Tooltip
            content={
              <ChartTooltip
                labelFormatter={(label) => xFormatter(Number(label))}
                valueFormatter={yFormatter}
              />
            }
          />

          {referenceLines.map((line, i) => (
            <ReferenceLine
              key={i}
              y={line.y}
              yAxisId={line.yAxisId || "left"}
              stroke={line.color || colors.textMuted}
              strokeDasharray="5 5"
              label={{
                value: line.label,
                fill: chartTheme.axis.tick.fill,
                fontSize: 10,
                position: "right",
              }}
            />
          ))}

          {/* Render bars first (back), then areas, then lines (front) */}
          {visibleSeries.filter((s) => s.type === "bar").map(renderSeries)}
          {visibleSeries.filter((s) => s.type === "area").map(renderSeries)}
          {visibleSeries.filter((s) => s.type === "line").map(renderSeries)}

          {showBrush && (
            <Brush
              dataKey={xKey}
              height={30}
              stroke={colors.grid}
              fill={colors.backgroundDark}
              tickFormatter={xFormatter}
            />
          )}
        </RechartsComposedChart>
      </ResponsiveContainer>

      {showLegend && renderLegend()}
    </div>
  );
}

