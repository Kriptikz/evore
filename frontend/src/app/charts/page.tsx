"use client";

import { useState, useEffect, useCallback, Suspense, useMemo, useRef } from "react";
import { useSearchParams, useRouter, usePathname } from "next/navigation";
import { Header } from "@/components/Header";
import {
  AreaChart,
  Area,
  LineChart,
  Line,
  BarChart,
  Bar,
  ComposedChart,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Brush,
  Legend,
} from "recharts";
import {
  formatters,
  colors,
  chartTheme,
} from "@/components/charts";
import {
  api,
  RoundsHourlyData,
  RoundsDailyData,
  TreasuryHourlyData,
  MintHourlyData,
  MintDailyData,
  InflationHourlyData,
  InflationDailyData,
  CostPerOreDailyData,
  MinerActivityDailyData,
} from "@/lib/api";

// ============================================================================
// Types
// ============================================================================

type ChartType = 
  | "rounds"
  | "treasury"
  | "mint"
  | "inflation"
  | "cost_per_ore"
  | "miners";

type TimeRange = "24h" | "7d" | "30d" | "90d" | "1y";
type ScaleType = "linear" | "log";
type ChartStyle = "area" | "line";

interface SeriesConfig {
  key: string;
  name: string;
  color: string;
  unit?: string;
  yAxisId?: "left" | "right";
  type?: "area" | "line" | "bar";
}

interface ChartConfig {
  id: string;
  type: ChartType;
  range: TimeRange;
  enabledSeries: string[];  // Keys of enabled series
  scale: ScaleType;
  style: ChartStyle;
  showGrid: boolean;
  showBrush: boolean;
}

interface ChartData {
  type: ChartType;
  range: TimeRange;
  data: unknown[];
  loading: boolean;
  error: string | null;
}

// ============================================================================
// Series Definitions per Chart Type
// ============================================================================

const CHART_SERIES: Record<ChartType, SeriesConfig[]> = {
  rounds: [
    { key: "total_deployed", name: "Total Deployed", color: colors.positive, unit: "SOL", type: "area" },
    { key: "total_winnings", name: "Total Winnings", color: colors.purple, unit: "SOL", type: "area" },
    { key: "total_vaulted", name: "Total Vaulted", color: colors.blue, unit: "SOL", type: "line" },
    { key: "rounds_count", name: "Rounds Count", color: colors.primary, yAxisId: "right", type: "line" },
    { key: "unique_miners", name: "Unique Miners", color: colors.cyan, yAxisId: "right", type: "line" },
    { key: "motherlode_hits", name: "Motherlode Hits", color: colors.orange, yAxisId: "right", type: "bar" },
  ],
  treasury: [
    { key: "balance", name: "Balance", color: colors.primary, unit: "SOL", type: "area" },
    { key: "motherlode", name: "Motherlode", color: colors.purple, unit: "ORE", type: "line" },
    { key: "total_unclaimed", name: "Unclaimed", color: colors.blue, unit: "ORE", type: "line" },
    { key: "total_staked", name: "Staked", color: colors.positive, unit: "ORE", type: "line" },
    { key: "total_refined", name: "Refined", color: colors.cyan, unit: "ORE", type: "line" },
  ],
  miners: [
    { key: "active_miners", name: "Active Miners", color: colors.blue, type: "bar" },
    { key: "total_deployments", name: "Deployments", color: colors.primary, yAxisId: "right", type: "line" },
    { key: "total_deployed", name: "Total Deployed", color: colors.positive, unit: "SOL", type: "area" },
    { key: "total_won", name: "Total Won", color: colors.purple, unit: "SOL", type: "area" },
  ],
  cost_per_ore: [
    { key: "cost_per_ore_sol", name: "Daily Cost/ORE", color: colors.primary, unit: "SOL", type: "bar" },
    { key: "cumulative_cost_sol", name: "Cumulative Avg", color: colors.positive, unit: "SOL", type: "line" },
    { key: "total_vaulted_sol", name: "Total Vaulted", color: colors.blue, unit: "SOL", yAxisId: "right", type: "line" },
    { key: "ore_minted_ore", name: "ORE Minted", color: colors.purple, unit: "ORE", yAxisId: "right", type: "line" },
  ],
  mint: [
    { key: "supply", name: "Total Supply", color: colors.primary, unit: "ORE", type: "area" },
    { key: "supply_change_total", name: "Supply Change", color: colors.positive, unit: "ORE", yAxisId: "right", type: "bar" },
  ],
  inflation: [
    { key: "circulating_end", name: "Circulating", color: colors.primary, unit: "ORE", type: "area" },
    { key: "market_inflation_total", name: "Market Inflation", color: colors.positive, unit: "ORE", yAxisId: "right", type: "bar" },
    { key: "supply_change_total", name: "Supply Change", color: colors.blue, unit: "ORE", yAxisId: "right", type: "line" },
    { key: "supply_end", name: "Total Supply", color: colors.purple, unit: "ORE", type: "line" },
  ],
};

// Default enabled series per chart type
const DEFAULT_SERIES: Record<ChartType, string[]> = {
  rounds: ["total_deployed", "total_winnings"],
  treasury: ["balance", "motherlode"],
  miners: ["active_miners", "total_deployments"],
  cost_per_ore: ["cost_per_ore_sol", "cumulative_cost_sol"],
  mint: ["supply", "supply_change_total"],
  inflation: ["circulating_end", "market_inflation_total"],
};

// ============================================================================
// Constants
// ============================================================================

const CHART_TYPES: { value: ChartType; label: string; icon: string }[] = [
  { value: "rounds", label: "Round Activity", icon: "⚡" },
  { value: "treasury", label: "Treasury", icon: "💰" },
  { value: "miners", label: "Miner Activity", icon: "⛏️" },
  { value: "cost_per_ore", label: "Cost per ORE", icon: "📊" },
  { value: "mint", label: "Mint Supply", icon: "🪙" },
  { value: "inflation", label: "Market Inflation", icon: "📈" },
];

const TIME_RANGES: { value: TimeRange; label: string; hours?: number; days?: number }[] = [
  { value: "24h", label: "24H", hours: 24 },
  { value: "7d", label: "7D", days: 7 },
  { value: "30d", label: "30D", days: 30 },
  { value: "90d", label: "90D", days: 90 },
  { value: "1y", label: "1Y", days: 365 },
];

const MAX_CHARTS = 6;

// ============================================================================
// URL State Management
// ============================================================================

// Format: c=type:range:series1,series2:scale:style:grid:brush|type:range:...
function parseChartsFromUrl(searchParams: URLSearchParams): ChartConfig[] {
  const chartsParam = searchParams.get("c");
  if (!chartsParam) {
    return [
      { id: "1", type: "rounds", range: "7d", enabledSeries: DEFAULT_SERIES.rounds, scale: "linear", style: "area", showGrid: true, showBrush: false },
      { id: "2", type: "treasury", range: "7d", enabledSeries: DEFAULT_SERIES.treasury, scale: "linear", style: "area", showGrid: true, showBrush: false },
    ];
  }

  try {
    const configs: ChartConfig[] = [];
    const chartParts = chartsParam.split("|");
    
    chartParts.forEach((part, idx) => {
      const [type, range, seriesStr, scale, style, grid, brush] = part.split(":");
      
      if (!CHART_TYPES.some(ct => ct.value === type) || !TIME_RANGES.some(tr => tr.value === range)) {
        return;
      }

      const chartType = type as ChartType;
      const availableSeries = CHART_SERIES[chartType].map(s => s.key);
      const enabledSeries = seriesStr 
        ? seriesStr.split(",").filter(s => availableSeries.includes(s))
        : DEFAULT_SERIES[chartType];

      configs.push({
        id: String(idx + 1),
        type: chartType,
        range: range as TimeRange,
        enabledSeries: enabledSeries.length > 0 ? enabledSeries : DEFAULT_SERIES[chartType],
        scale: (scale === "log" ? "log" : "linear") as ScaleType,
        style: (style === "line" ? "line" : "area") as ChartStyle,
        showGrid: grid !== "0",
        showBrush: brush === "1",
      });
    });
    
    return configs.length > 0 ? configs : [
      { id: "1", type: "rounds", range: "7d", enabledSeries: DEFAULT_SERIES.rounds, scale: "linear", style: "area", showGrid: true, showBrush: false },
    ];
  } catch {
    return [
      { id: "1", type: "rounds", range: "7d", enabledSeries: DEFAULT_SERIES.rounds, scale: "linear", style: "area", showGrid: true, showBrush: false },
    ];
  }
}

function chartsToUrlParam(charts: ChartConfig[]): string {
  return charts.map(c => {
    const series = c.enabledSeries.join(",");
    const scale = c.scale === "log" ? "log" : "lin";
    const style = c.style === "line" ? "line" : "area";
    const grid = c.showGrid ? "1" : "0";
    const brush = c.showBrush ? "1" : "0";
    return `${c.type}:${c.range}:${series}:${scale}:${style}:${grid}:${brush}`;
  }).join("|");
}

// ============================================================================
// Data Fetching
// ============================================================================

async function fetchChartData(type: ChartType, range: TimeRange): Promise<unknown[]> {
  const rangeConfig = TIME_RANGES.find(r => r.value === range);
  const hours = rangeConfig?.hours || (rangeConfig?.days || 30) * 24;
  const days = rangeConfig?.days || Math.ceil(hours / 24);

  switch (type) {
    case "rounds":
      if (hours <= 168) {
        return api.getChartRoundsHourly(hours);
      }
      return api.getChartRoundsDaily(days);
    case "treasury":
      return api.getChartTreasuryHourly(Math.min(hours, 720));
    case "mint":
      if (hours <= 168) {
        return api.getChartMintHourly(hours);
      }
      return api.getChartMintDaily(days);
    case "inflation":
      if (hours <= 168) {
        return api.getChartInflationHourly(hours);
      }
      return api.getChartInflationDaily(days);
    case "cost_per_ore":
      return api.getChartCostPerOreDaily(days);
    case "miners":
      return api.getChartMinersDaily(days);
    default:
      return [];
  }
}

// ============================================================================
// Data Transformers
// ============================================================================

function transformChartData(type: ChartType, data: unknown[], range: TimeRange): Record<string, unknown>[] {
  const isHourly = range === "24h" || range === "7d";
  
  switch (type) {
    case "cost_per_ore":
      return (data as CostPerOreDailyData[]).map(d => ({
        ...d,
        cost_per_ore_sol: d.cost_per_ore_lamports / 1e9,
        cumulative_cost_sol: d.cumulative_cost_per_ore / 1e9,
        total_vaulted_sol: d.total_vaulted / 1e9,
        ore_minted_ore: d.ore_minted_total / 1e11,
      }));
    case "mint":
      return (data as (MintHourlyData | MintDailyData)[]).map(d => ({
        ...d,
        supply: (d.supply || 0) / 1e11,
        supply_change_total: (d.supply_change_total || 0) / 1e11,
      }));
    case "inflation":
      return (data as (InflationHourlyData | InflationDailyData)[]).map(d => ({
        ...d,
        circulating_end: (d.circulating_end || 0) / 1e11,
        market_inflation_total: (d.market_inflation_total || 0) / 1e11,
        supply_change_total: (d.supply_change_total || 0) / 1e11,
        supply_end: ((d as InflationHourlyData).supply_end || (d as InflationDailyData).supply_end || 0) / 1e11,
      }));
    case "treasury":
      return (data as TreasuryHourlyData[]).map(d => ({
        ...d,
        balance: (d.balance || 0) / 1e9,
        motherlode: (d.motherlode || 0) / 1e11,
        total_unclaimed: (d.total_unclaimed || 0) / 1e11,
        total_staked: (d.total_staked || 0) / 1e11,
        total_refined: (d.total_refined || 0) / 1e11,
      }));
    case "rounds":
      return (data as (RoundsHourlyData | RoundsDailyData)[]).map(d => ({
        ...d,
        total_deployed: (d.total_deployed || 0) / 1e9,
        total_winnings: (d.total_winnings || 0) / 1e9,
        total_vaulted: (d.total_vaulted || 0) / 1e9,
      }));
    case "miners":
      return (data as MinerActivityDailyData[]).map(d => ({
        ...d,
        total_deployed: (d.total_deployed || 0) / 1e9,
        total_won: (d.total_won || 0) / 1e9,
      }));
    default:
      return data as Record<string, unknown>[];
  }
}

function getXKey(type: ChartType, range: TimeRange): string {
  const isHourly = range === "24h" || range === "7d";
  if (type === "cost_per_ore" || type === "miners") return "day";
  if (type === "treasury") return "hour";
  return isHourly ? "hour" : "day";
}

function getXFormatter(type: ChartType, range: TimeRange): (value: number) => string {
  const isHourly = range === "24h" || range === "7d";
  if (type === "treasury" || (isHourly && type !== "cost_per_ore" && type !== "miners")) {
    return formatters.dateTime;
  }
  return formatters.date;
}

// ============================================================================
// Custom Tooltip
// ============================================================================

interface CustomTooltipProps {
  active?: boolean;
  payload?: Array<{
    name: string;
    value: number;
    color: string;
    dataKey: string;
  }>;
  label?: string | number;
  xFormatter: (value: number) => string;
  seriesConfigs: SeriesConfig[];
}

function CustomTooltip({ active, payload, label, xFormatter, seriesConfigs }: CustomTooltipProps) {
  if (!active || !payload || payload.length === 0) return null;

  const formatValue = (key: string, value: number): string => {
    const config = seriesConfigs.find(s => s.key === key);
    if (!config) return formatters.number(value);
    
    if (config.unit === "SOL") {
      return `${value.toFixed(4)} SOL`;
    }
    if (config.unit === "ORE") {
      return `${formatters.ore(value * 1e11)} ORE`;
    }
    return formatters.number(value);
  };

  return (
    <div className="bg-slate-800 border border-slate-700 rounded-lg shadow-xl p-3 max-w-xs">
      <p className="text-xs text-slate-400 mb-2 pb-2 border-b border-slate-700">
        {typeof label === "number" ? xFormatter(label) : label}
      </p>
      <div className="space-y-1.5">
        {payload.map((item, i) => (
          <div key={i} className="flex items-center justify-between gap-4">
            <div className="flex items-center gap-2">
              <span
                className="w-2.5 h-2.5 rounded-full flex-shrink-0"
                style={{ backgroundColor: item.color }}
              />
              <span className="text-xs text-slate-400 truncate">{item.name}</span>
            </div>
            <span className="text-sm font-mono text-white">
              {formatValue(item.dataKey, item.value)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ============================================================================
// Series Toggle Component
// ============================================================================

function SeriesToggle({
  series,
  enabledSeries,
  onToggle,
}: {
  series: SeriesConfig[];
  enabledSeries: string[];
  onToggle: (key: string) => void;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2">
      {series.map(s => {
        const isEnabled = enabledSeries.includes(s.key);
        return (
          <button
            key={s.key}
            onClick={() => onToggle(s.key)}
            className={`flex items-center gap-1.5 px-2 py-1 rounded-md text-xs transition-all ${
              isEnabled
                ? "bg-slate-700 text-white"
                : "bg-slate-800/50 text-slate-500 hover:text-slate-400"
            }`}
          >
            <span
              className={`w-2 h-2 rounded-full transition-opacity ${isEnabled ? "opacity-100" : "opacity-30"}`}
              style={{ backgroundColor: s.color }}
            />
            <span>{s.name}</span>
          </button>
        );
      })}
    </div>
  );
}

// ============================================================================
// Chart Options Panel
// ============================================================================

function ChartOptions({
  config,
  onUpdate,
}: {
  config: ChartConfig;
  onUpdate: (updates: Partial<ChartConfig>) => void;
}) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="relative">
      <button
        onClick={() => setIsOpen(!isOpen)}
        className="p-1.5 text-slate-400 hover:text-slate-300 hover:bg-slate-800 rounded-lg transition-colors"
        title="Chart options"
      >
        <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>

      {isOpen && (
        <>
          <div className="fixed inset-0 z-10" onClick={() => setIsOpen(false)} />
          <div className="absolute right-0 top-full mt-2 w-48 bg-slate-800 border border-slate-700 rounded-lg shadow-xl z-20 p-3 space-y-3">
            {/* Scale Toggle */}
            <div>
              <label className="text-xs text-slate-400 block mb-1.5">Scale</label>
              <div className="flex gap-1">
                <button
                  onClick={() => onUpdate({ scale: "linear" })}
                  className={`flex-1 px-2 py-1 text-xs rounded ${
                    config.scale === "linear"
                      ? "bg-amber-500/20 text-amber-400"
                      : "bg-slate-700 text-slate-400"
                  }`}
                >
                  Linear
                </button>
                <button
                  onClick={() => onUpdate({ scale: "log" })}
                  className={`flex-1 px-2 py-1 text-xs rounded ${
                    config.scale === "log"
                      ? "bg-amber-500/20 text-amber-400"
                      : "bg-slate-700 text-slate-400"
                  }`}
                >
                  Log
                </button>
              </div>
            </div>

            {/* Style Toggle */}
            <div>
              <label className="text-xs text-slate-400 block mb-1.5">Style</label>
              <div className="flex gap-1">
                <button
                  onClick={() => onUpdate({ style: "area" })}
                  className={`flex-1 px-2 py-1 text-xs rounded ${
                    config.style === "area"
                      ? "bg-amber-500/20 text-amber-400"
                      : "bg-slate-700 text-slate-400"
                  }`}
                >
                  Area
                </button>
                <button
                  onClick={() => onUpdate({ style: "line" })}
                  className={`flex-1 px-2 py-1 text-xs rounded ${
                    config.style === "line"
                      ? "bg-amber-500/20 text-amber-400"
                      : "bg-slate-700 text-slate-400"
                  }`}
                >
                  Line
                </button>
              </div>
            </div>

            {/* Grid Toggle */}
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.showGrid}
                onChange={(e) => onUpdate({ showGrid: e.target.checked })}
                className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-amber-500 focus:ring-amber-500/50"
              />
              <span className="text-xs text-slate-300">Show Grid</span>
            </label>

            {/* Brush Toggle */}
            <label className="flex items-center gap-2 cursor-pointer">
              <input
                type="checkbox"
                checked={config.showBrush}
                onChange={(e) => onUpdate({ showBrush: e.target.checked })}
                className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-amber-500 focus:ring-amber-500/50"
              />
              <span className="text-xs text-slate-300">Show Brush (zoom)</span>
            </label>
          </div>
        </>
      )}
    </div>
  );
}

// ============================================================================
// Dynamic Chart Renderer
// ============================================================================

function DynamicChart({
  config,
  data,
}: {
  config: ChartConfig;
  data: Record<string, unknown>[];
}) {
  const seriesConfigs = CHART_SERIES[config.type];
  const enabledConfigs = seriesConfigs.filter(s => config.enabledSeries.includes(s.key));
  const xKey = getXKey(config.type, config.range);
  const xFormatter = getXFormatter(config.type, config.range);

  const hasRightAxis = enabledConfigs.some(s => s.yAxisId === "right");
  const hasBars = enabledConfigs.some(s => s.type === "bar");
  const hasLines = enabledConfigs.some(s => s.type === "line");
  const hasAreas = enabledConfigs.some(s => s.type === "area");

  // Use ComposedChart if we have mixed types, otherwise use specific chart type
  const useComposed = (hasBars && (hasLines || hasAreas)) || (hasLines && hasAreas);

  const commonProps = {
    data,
    margin: { top: 10, right: hasRightAxis ? 60 : 10, bottom: config.showBrush ? 40 : 20, left: 60 },
  };

  const renderSeries = () => {
    return enabledConfigs.map(s => {
      const commonSeriesProps = {
        key: s.key,
        dataKey: s.key,
        name: s.name,
        yAxisId: s.yAxisId || "left",
        isAnimationActive: true,
        animationDuration: 300,
      };

      if (s.type === "bar" || (hasBars && !hasLines && !hasAreas)) {
        return (
          <Bar
            {...commonSeriesProps}
            fill={s.color}
            radius={[2, 2, 0, 0]}
            opacity={0.8}
          />
        );
      }

      if (s.type === "line" || config.style === "line") {
        return (
          <Line
            {...commonSeriesProps}
            stroke={s.color}
            strokeWidth={2}
            dot={false}
            type="monotone"
          />
        );
      }

      // Default to area
      return (
        <Area
          {...commonSeriesProps}
          stroke={s.color}
          strokeWidth={2}
          fill={s.color}
          fillOpacity={0.2}
          type="monotone"
        />
      );
    });
  };

  const ChartComponent = useComposed ? ComposedChart : 
    hasBars ? BarChart : 
    config.style === "line" ? LineChart : AreaChart;

  return (
    <ResponsiveContainer width="100%" height={320}>
      <ChartComponent {...commonProps}>
        <defs>
          {enabledConfigs.map(s => (
            <linearGradient key={`grad-${s.key}`} id={`gradient-${s.key}`} x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor={s.color} stopOpacity={0.4} />
              <stop offset="100%" stopColor={s.color} stopOpacity={0.05} />
            </linearGradient>
          ))}
        </defs>

        {config.showGrid && (
          <CartesianGrid
            strokeDasharray="3 3"
            stroke={colors.grid}
            strokeOpacity={0.3}
            vertical={false}
          />
        )}

        <XAxis
          dataKey={xKey}
          stroke={chartTheme.axis.stroke}
          tick={{ fill: chartTheme.axis.tick.fill, fontSize: 10 }}
          tickFormatter={xFormatter}
          tickLine={false}
          axisLine={{ strokeWidth: 1 }}
          minTickGap={50}
        />

        <YAxis
          yAxisId="left"
          stroke={chartTheme.axis.stroke}
          tick={{ fill: chartTheme.axis.tick.fill, fontSize: 10 }}
          tickFormatter={formatters.number}
          tickLine={false}
          axisLine={false}
          scale={config.scale}
          domain={config.scale === "log" ? ["auto", "auto"] : undefined}
          width={55}
        />

        {hasRightAxis && (
          <YAxis
            yAxisId="right"
            orientation="right"
            stroke={chartTheme.axis.stroke}
            tick={{ fill: chartTheme.axis.tick.fill, fontSize: 10 }}
            tickFormatter={formatters.number}
            tickLine={false}
            axisLine={false}
            scale={config.scale}
            domain={config.scale === "log" ? ["auto", "auto"] : undefined}
            width={55}
          />
        )}

        <Tooltip
          content={
            <CustomTooltip
              xFormatter={xFormatter}
              seriesConfigs={seriesConfigs}
            />
          }
        />

        {renderSeries()}

        {config.showBrush && (
          <Brush
            dataKey={xKey}
            height={25}
            stroke={colors.grid}
            fill={colors.backgroundDark}
            tickFormatter={xFormatter}
          />
        )}
      </ChartComponent>
    </ResponsiveContainer>
  );
}

// ============================================================================
// Chart Card Component
// ============================================================================

function ChartCard({
  config,
  data,
  loading,
  error,
  onRemove,
  onUpdate,
}: {
  config: ChartConfig;
  data: unknown[];
  loading: boolean;
  error: string | null;
  onRemove: () => void;
  onUpdate: (updates: Partial<ChartConfig>) => void;
}) {
  const chartInfo = CHART_TYPES.find(c => c.value === config.type);
  const seriesConfigs = CHART_SERIES[config.type];
  
  const transformedData = useMemo(() => {
    if (data.length === 0) return [];
    return transformChartData(config.type, data, config.range);
  }, [data, config.type, config.range]);

  const toggleSeries = (key: string) => {
    const newEnabled = config.enabledSeries.includes(key)
      ? config.enabledSeries.filter(s => s !== key)
      : [...config.enabledSeries, key];
    
    // Ensure at least one series is enabled
    if (newEnabled.length > 0) {
      onUpdate({ enabledSeries: newEnabled });
    }
  };

  return (
    <div className="bg-slate-900/50 border border-slate-800/50 rounded-xl overflow-hidden">
      {/* Chart Header */}
      <div className="px-4 py-3 border-b border-slate-800/50">
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-3">
            {/* Chart Type Selector */}
            <select
              value={config.type}
              onChange={(e) => {
                const newType = e.target.value as ChartType;
                onUpdate({ 
                  type: newType, 
                  enabledSeries: DEFAULT_SERIES[newType] 
                });
              }}
              className="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50 cursor-pointer"
            >
              {CHART_TYPES.map(ct => (
                <option key={ct.value} value={ct.value}>
                  {ct.icon} {ct.label}
                </option>
              ))}
            </select>

            {/* Time Range Selector */}
            <div className="flex items-center gap-0.5 bg-slate-800/50 rounded-lg p-0.5">
              {TIME_RANGES.map(tr => (
                <button
                  key={tr.value}
                  onClick={() => onUpdate({ range: tr.value })}
                  className={`px-2 py-1 text-xs font-medium rounded transition-colors ${
                    config.range === tr.value
                      ? "bg-amber-500/20 text-amber-400"
                      : "text-slate-500 hover:text-slate-400"
                  }`}
                >
                  {tr.label}
                </button>
              ))}
            </div>
          </div>

          <div className="flex items-center gap-1">
            <ChartOptions config={config} onUpdate={onUpdate} />
            <button
              onClick={onRemove}
              className="p-1.5 text-slate-500 hover:text-red-400 hover:bg-slate-800 rounded-lg transition-colors"
              title="Remove chart"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>

        {/* Series Toggles */}
        <SeriesToggle
          series={seriesConfigs}
          enabledSeries={config.enabledSeries}
          onToggle={toggleSeries}
        />
      </div>

      {/* Chart Content */}
      <div className="p-4">
        {loading ? (
          <div className="h-80 flex items-center justify-center">
            <div className="flex items-center gap-2 text-slate-400">
              <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              <span>Loading chart data...</span>
            </div>
          </div>
        ) : error ? (
          <div className="h-80 flex items-center justify-center">
            <div className="text-center">
              <span className="text-red-400 text-sm">{error}</span>
              <p className="text-slate-500 text-xs mt-1">This data may not be available yet</p>
            </div>
          </div>
        ) : transformedData.length === 0 ? (
          <div className="h-80 flex items-center justify-center">
            <div className="text-center">
              <span className="text-4xl mb-2 block">{chartInfo?.icon}</span>
              <span className="text-slate-500 text-sm">No data available for this time range</span>
            </div>
          </div>
        ) : (
          <DynamicChart config={config} data={transformedData} />
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Main Charts Page Content
// ============================================================================

function ChartsContent() {
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  // Parse charts from URL
  const [charts, setCharts] = useState<ChartConfig[]>(() => 
    parseChartsFromUrl(searchParams)
  );

  // Chart data state
  const [chartData, setChartData] = useState<Map<string, ChartData>>(new Map());

  // Debounce URL updates
  const urlUpdateTimeout = useRef<NodeJS.Timeout>();

  // Sync charts to URL (debounced)
  useEffect(() => {
    if (urlUpdateTimeout.current) {
      clearTimeout(urlUpdateTimeout.current);
    }

    urlUpdateTimeout.current = setTimeout(() => {
      const urlParam = chartsToUrlParam(charts);
      const currentParam = searchParams.get("c");
      
      if (urlParam !== currentParam) {
        router.replace(`${pathname}?c=${urlParam}`, { scroll: false });
      }
    }, 300);

    return () => {
      if (urlUpdateTimeout.current) {
        clearTimeout(urlUpdateTimeout.current);
      }
    };
  }, [charts, router, pathname, searchParams]);

  // Fetch data for all charts
  useEffect(() => {
    charts.forEach(async (config) => {
      const key = `${config.type}:${config.range}`;
      
      const existing = chartData.get(key);
      if (existing && (existing.loading || existing.data.length > 0)) {
        return;
      }

      setChartData(prev => {
        const next = new Map(prev);
        next.set(key, { type: config.type, range: config.range, data: [], loading: true, error: null });
        return next;
      });

      try {
        const data = await fetchChartData(config.type, config.range);
        setChartData(prev => {
          const next = new Map(prev);
          next.set(key, { type: config.type, range: config.range, data, loading: false, error: null });
          return next;
        });
      } catch (err) {
        setChartData(prev => {
          const next = new Map(prev);
          next.set(key, { 
            type: config.type, 
            range: config.range, 
            data: [], 
            loading: false, 
            error: err instanceof Error ? err.message : "Failed to load data" 
          });
          return next;
        });
      }
    });
  }, [charts]);

  // Chart management functions
  const addChart = useCallback(() => {
    if (charts.length >= MAX_CHARTS) return;
    const newId = String(Date.now());
    const unusedType = CHART_TYPES.find(ct => !charts.some(c => c.type === ct.value))?.value || "rounds";
    setCharts(prev => [...prev, { 
      id: newId, 
      type: unusedType, 
      range: "7d",
      enabledSeries: DEFAULT_SERIES[unusedType],
      scale: "linear",
      style: "area",
      showGrid: true,
      showBrush: false,
    }]);
  }, [charts]);

  const removeChart = useCallback((id: string) => {
    setCharts(prev => prev.filter(c => c.id !== id));
  }, []);

  const updateChart = useCallback((id: string, updates: Partial<ChartConfig>) => {
    setCharts(prev => prev.map(c => c.id === id ? { ...c, ...updates } : c));
  }, []);

  // Copy share URL
  const copyShareUrl = useCallback(async () => {
    try {
      const url = window.location.href;
      await navigator.clipboard.writeText(url);
    } catch {
      console.log("Share URL:", window.location.href);
    }
  }, []);

  return (
    <div className="min-h-screen bg-slate-950">
      <Header />

      <main className="max-w-7xl mx-auto px-4 py-6">
        {/* Page Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold text-white">Charts</h1>
            <p className="text-slate-400 text-sm mt-1">
              Interactive time series charts with customizable series and filters
            </p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={copyShareUrl}
              className="flex items-center gap-2 px-3 py-2 bg-slate-800 hover:bg-slate-700 border border-slate-700 rounded-lg text-sm text-slate-300 transition-colors"
              title="Copy shareable URL with all chart configurations"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M8.684 13.342C8.886 12.938 9 12.482 9 12c0-.482-.114-.938-.316-1.342m0 2.684a3 3 0 110-2.684m0 2.684l6.632 3.316m-6.632-6l6.632-3.316m0 0a3 3 0 105.367-2.684 3 3 0 00-5.367 2.684zm0 9.316a3 3 0 105.368 2.684 3 3 0 00-5.368-2.684z" />
              </svg>
              Share
            </button>
            <button
              onClick={addChart}
              disabled={charts.length >= MAX_CHARTS}
              className="flex items-center gap-2 px-4 py-2 bg-amber-600 hover:bg-amber-500 disabled:bg-slate-700 disabled:text-slate-500 rounded-lg text-sm font-medium text-white transition-colors"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              Add Chart
            </button>
          </div>
        </div>

        {/* Charts Grid */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {charts.map(config => {
            const key = `${config.type}:${config.range}`;
            const data = chartData.get(key);
            return (
              <ChartCard
                key={config.id}
                config={config}
                data={data?.data || []}
                loading={data?.loading || false}
                error={data?.error || null}
                onRemove={() => removeChart(config.id)}
                onUpdate={(updates) => updateChart(config.id, updates)}
              />
            );
          })}
        </div>

        {/* Empty State */}
        {charts.length === 0 && (
          <div className="text-center py-16">
            <div className="text-6xl mb-4">📊</div>
            <h2 className="text-xl font-semibold text-white mb-2">No charts configured</h2>
            <p className="text-slate-400 mb-6">Add a chart to start visualizing ORE mining data</p>
            <button
              onClick={addChart}
              className="px-6 py-3 bg-amber-600 hover:bg-amber-500 rounded-lg text-white font-medium transition-colors"
            >
              Add Your First Chart
            </button>
          </div>
        )}

        {/* Quick Add Section */}
        {charts.length > 0 && charts.length < MAX_CHARTS && (
          <div className="mt-6 p-4 bg-slate-900/30 border border-slate-800/50 rounded-xl">
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-3">
                <span className="text-slate-400 text-sm">Quick add:</span>
                <div className="flex items-center gap-2">
                  {CHART_TYPES.filter(ct => !charts.some(c => c.type === ct.value)).slice(0, 4).map(ct => (
                    <button
                      key={ct.value}
                      onClick={() => {
                        const newId = String(Date.now());
                        setCharts(prev => [...prev, { 
                          id: newId, 
                          type: ct.value, 
                          range: "7d",
                          enabledSeries: DEFAULT_SERIES[ct.value],
                          scale: "linear",
                          style: "area",
                          showGrid: true,
                          showBrush: false,
                        }]);
                      }}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-slate-800/50 hover:bg-slate-800 border border-slate-700/50 rounded-lg text-xs text-slate-400 hover:text-slate-300 transition-colors"
                    >
                      <span>{ct.icon}</span>
                      <span>{ct.label}</span>
                    </button>
                  ))}
                </div>
              </div>
              <span className="text-slate-500 text-xs">{charts.length}/{MAX_CHARTS} charts</span>
            </div>
          </div>
        )}

        {/* Help Text */}
        <div className="mt-8 p-4 bg-slate-900/30 border border-slate-800/50 rounded-xl">
          <h3 className="text-sm font-medium text-slate-300 mb-2">Tips</h3>
          <ul className="text-xs text-slate-500 space-y-1">
            <li>• Click series buttons to toggle visibility - all settings are saved in the URL</li>
            <li>• Use the settings icon (⚙️) to switch between linear/log scale, area/line style, and toggle grid/brush</li>
            <li>• Enable the brush to zoom into specific time ranges</li>
            <li>• Share button copies the current URL with all your chart configurations</li>
          </ul>
        </div>
      </main>
    </div>
  );
}

// ============================================================================
// Page Export with Suspense
// ============================================================================

export default function ChartsPage() {
  return (
    <Suspense fallback={
      <div className="min-h-screen bg-slate-950 flex items-center justify-center">
        <div className="flex items-center gap-2 text-slate-400">
          <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
            <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
            <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
          </svg>
          <span>Loading charts...</span>
        </div>
      </div>
    }>
      <ChartsContent />
    </Suspense>
  );
}
