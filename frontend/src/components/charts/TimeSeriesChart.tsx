"use client";

import { useMemo, useState } from "react";
import {
  AreaChart,
  Area,
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
  Brush,
  ReferenceLine,
} from "recharts";
import { colors, chartTheme, formatters, gradients, margins, seriesColors } from "./theme";
import { ChartTooltip, ValueFormatter } from "./ChartTooltip";

export type ChartVariant = "area" | "line";

export interface DataSeries {
  key: string;
  name: string;
  color?: string;
  yAxisId?: "left" | "right";
  hidden?: boolean;
}

export interface TimeSeriesChartProps<T = Record<string, unknown>> {
  data: T[];
  series: DataSeries[];
  xKey: string;
  variant?: ChartVariant;
  height?: number;
  xFormatter?: (value: number) => string;
  yFormatter?: ValueFormatter;
  yFormatterRight?: ValueFormatter;
  showGrid?: boolean;
  showBrush?: boolean;
  showLegend?: boolean;
  referenceLines?: { y: number; label: string; color?: string }[];
  dualAxis?: boolean;
  animate?: boolean;
}

/**
 * Time series chart component for displaying data over time
 * Supports area and line variants, dual Y-axes, brushing, and more
 */
export function TimeSeriesChart<T = Record<string, unknown>>({
  data,
  series,
  xKey,
  variant = "area",
  height = 300,
  xFormatter = formatters.dateTime,
  yFormatter = formatters.number,
  yFormatterRight,
  showGrid = true,
  showBrush = false,
  showLegend = true,
  referenceLines = [],
  dualAxis = false,
  animate = true,
}: TimeSeriesChartProps<T>) {
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
  const visibleSeries = seriesWithColors.filter(
    (s) => !hiddenSeries.has(s.key) && !s.hidden
  );

  // Gradient definitions
  const gradientDefs = (
    <defs>
      {seriesWithColors.map((s) => (
        <linearGradient key={s.key} id={`gradient-${s.key}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={s.color} stopOpacity={0.4} />
          <stop offset="100%" stopColor={s.color} stopOpacity={0.05} />
        </linearGradient>
      ))}
    </defs>
  );

  // Common chart props
  const chartProps = {
    data,
    margin: showBrush ? margins.withLegend : margins.default,
  };

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
              className="w-2.5 h-2.5 rounded-full"
              style={{ backgroundColor: s.color }}
            />
            <span style={{ color: chartTheme.legend.textColor }}>{s.name}</span>
          </button>
        );
      })}
    </div>
  );

  // Tooltip content
  const tooltipContent = (
    <Tooltip
      content={
        <ChartTooltip
          labelFormatter={(label) => xFormatter(Number(label))}
          valueFormatter={yFormatter}
        />
      }
    />
  );

  const ChartComponent = variant === "area" ? AreaChart : LineChart;

  return (
    <div style={{ height }}>
      <ResponsiveContainer width="100%" height={showLegend ? "90%" : "100%"}>
        <ChartComponent {...chartProps}>
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

          {tooltipContent}

          {referenceLines.map((line, i) => (
            <ReferenceLine
              key={i}
              y={line.y}
              yAxisId="left"
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

          {visibleSeries.map((s) =>
            variant === "area" ? (
              <Area
                key={s.key}
                type="monotone"
                dataKey={s.key}
                name={s.name}
                stroke={s.color}
                strokeWidth={2}
                fill={`url(#gradient-${s.key})`}
                yAxisId={s.yAxisId || "left"}
                isAnimationActive={animate}
                animationDuration={chartTheme.animation.duration}
              />
            ) : (
              <Line
                key={s.key}
                type="monotone"
                dataKey={s.key}
                name={s.name}
                stroke={s.color}
                strokeWidth={2}
                dot={false}
                yAxisId={s.yAxisId || "left"}
                isAnimationActive={animate}
                animationDuration={chartTheme.animation.duration}
              />
            )
          )}

          {showBrush && (
            <Brush
              dataKey={xKey}
              height={30}
              stroke={colors.grid}
              fill={colors.backgroundDark}
              tickFormatter={xFormatter}
            />
          )}
        </ChartComponent>
      </ResponsiveContainer>

      {showLegend && renderLegend()}
    </div>
  );
}

