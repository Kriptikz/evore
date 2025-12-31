"use client";

import { useState, useEffect, useCallback, Suspense, useMemo } from "react";
import { useSearchParams, useRouter, usePathname } from "next/navigation";
import { Header } from "@/components/Header";
import {
  TimeSeriesChart,
  StatsBarChart,
  ComposedChart,
  TimeRangeSelector,
  formatters,
  colors,
  ChartVariant,
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

interface ChartConfig {
  id: string;
  type: ChartType;
  range: TimeRange;
  variant?: ChartVariant;
}

interface ChartData {
  type: ChartType;
  range: TimeRange;
  data: unknown[];
  loading: boolean;
  error: string | null;
}

// ============================================================================
// Constants
// ============================================================================

const CHART_TYPES: { value: ChartType; label: string; description: string; icon: string }[] = [
  { value: "rounds", label: "Round Activity", description: "Deployments, miners, and winnings", icon: "⚡" },
  { value: "treasury", label: "Treasury", description: "Balance and unclaimed ORE", icon: "💰" },
  { value: "miners", label: "Miner Activity", description: "Active miners and volume", icon: "⛏️" },
  { value: "cost_per_ore", label: "Cost per ORE", description: "SOL cost per ORE mined", icon: "📊" },
  { value: "mint", label: "Mint Supply", description: "Total ORE supply", icon: "🪙" },
  { value: "inflation", label: "Market Inflation", description: "Circulating supply", icon: "📈" },
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

function parseChartsFromUrl(searchParams: URLSearchParams): ChartConfig[] {
  const chartsParam = searchParams.get("c");
  if (!chartsParam) {
    return [
      { id: "1", type: "rounds", range: "7d" },
      { id: "2", type: "treasury", range: "7d" },
    ];
  }

  try {
    const configs: ChartConfig[] = [];
    const parts = chartsParam.split(",");
    parts.forEach((part, idx) => {
      const [type, range] = part.split(":");
      if (CHART_TYPES.some(ct => ct.value === type) && TIME_RANGES.some(tr => tr.value === range)) {
        configs.push({
          id: String(idx + 1),
          type: type as ChartType,
          range: range as TimeRange,
        });
      }
    });
    return configs.length > 0 ? configs : [{ id: "1", type: "rounds", range: "7d" }];
  } catch {
    return [{ id: "1", type: "rounds", range: "7d" }];
  }
}

function chartsToUrlParam(charts: ChartConfig[]): string {
  return charts.map(c => `${c.type}:${c.range}`).join(",");
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
// Chart Renderers
// ============================================================================

function RoundsChart({ data, range }: { data: (RoundsHourlyData | RoundsDailyData)[]; range: TimeRange }) {
  const isHourly = range === "24h" || range === "7d";
  const xKey = isHourly ? "hour" : "day";
  const xFormatter = isHourly ? formatters.dateTime : formatters.date;

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard
          label="Rounds"
          value={data.reduce((sum, d) => sum + d.rounds_count, 0)}
          formatter={formatters.number}
          color="text-amber-400"
        />
        <StatCard
          label="Total Deployed"
          value={data.reduce((sum, d) => sum + d.total_deployed, 0)}
          formatter={formatters.sol}
          suffix=" SOL"
          color="text-emerald-400"
        />
        <StatCard
          label="Total Won"
          value={data.reduce((sum, d) => sum + d.total_winnings, 0)}
          formatter={formatters.sol}
          suffix=" SOL"
          color="text-purple-400"
        />
        <StatCard
          label="Motherlode Hits"
          value={data.reduce((sum, d) => sum + d.motherlode_hits, 0)}
          formatter={formatters.number}
          color="text-cyan-400"
        />
      </div>

      {/* Chart */}
      <TimeSeriesChart
        data={data}
        series={[
          { key: "total_deployed", name: "Deployed (SOL)", color: colors.positive },
          { key: "total_winnings", name: "Winnings (SOL)", color: colors.purple },
        ]}
        xKey={xKey}
        height={280}
        xFormatter={xFormatter}
        yFormatter={formatters.sol}
        showLegend={true}
        showGrid={true}
      />
    </div>
  );
}

function TreasuryChart({ data }: { data: TreasuryHourlyData[] }) {
  const latest = data[data.length - 1];

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard
          label="Current Balance"
          value={latest?.balance || 0}
          formatter={formatters.sol}
          suffix=" SOL"
          color="text-amber-400"
        />
        <StatCard
          label="Motherlode"
          value={latest?.motherlode || 0}
          formatter={formatters.ore}
          suffix=" ORE"
          color="text-purple-400"
        />
        <StatCard
          label="Unclaimed"
          value={latest?.total_unclaimed || 0}
          formatter={formatters.ore}
          suffix=" ORE"
          color="text-slate-300"
        />
        <StatCard
          label="Total Staked"
          value={latest?.total_staked || 0}
          formatter={formatters.ore}
          suffix=" ORE"
          color="text-emerald-400"
        />
      </div>

      {/* Chart */}
      <TimeSeriesChart
        data={data}
        series={[
          { key: "balance", name: "Balance (SOL)", color: colors.primary },
        ]}
        xKey="hour"
        height={280}
        xFormatter={formatters.dateTime}
        yFormatter={formatters.sol}
        showLegend={false}
        showGrid={true}
      />
    </div>
  );
}

function MinersChart({ data }: { data: MinerActivityDailyData[] }) {
  const latest = data[data.length - 1];
  const avgMiners = data.length > 0
    ? Math.round(data.reduce((sum, d) => sum + d.active_miners, 0) / data.length)
    : 0;

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard
          label="Active Today"
          value={latest?.active_miners || 0}
          formatter={formatters.number}
          color="text-amber-400"
        />
        <StatCard
          label="Avg Daily"
          value={avgMiners}
          formatter={formatters.number}
          color="text-slate-300"
        />
        <StatCard
          label="Deployments Today"
          value={latest?.total_deployments || 0}
          formatter={formatters.number}
          color="text-emerald-400"
        />
        <StatCard
          label="Won Today"
          value={latest?.total_won || 0}
          formatter={formatters.sol}
          suffix=" SOL"
          color="text-purple-400"
        />
      </div>

      {/* Chart */}
      <StatsBarChart
        data={data}
        series={[
          { key: "active_miners", name: "Active Miners", color: colors.blue },
        ]}
        xKey="day"
        height={280}
        xFormatter={formatters.date}
        yFormatter={formatters.number}
        showLegend={false}
        showGrid={true}
      />
    </div>
  );
}

function CostPerOreChart({ data }: { data: CostPerOreDailyData[] }) {
  const latest = data[data.length - 1];
  const avgCost = data.length > 0
    ? data.reduce((sum, d) => sum + d.cost_per_ore_lamports, 0) / data.length
    : 0;

  // Transform data for chart - convert lamports to SOL for display
  const chartData = data.map(d => ({
    ...d,
    cost_per_ore_sol: d.cost_per_ore_lamports / 1e9,
    cumulative_cost_sol: d.cumulative_cost_per_ore / 1e9,
  }));

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-3 gap-3">
        <StatCard
          label="Today's Cost/ORE"
          value={(latest?.cost_per_ore_lamports || 0) / 1e9}
          formatter={(v) => v.toFixed(4)}
          suffix=" SOL"
          color="text-amber-400"
        />
        <StatCard
          label="Avg Cost/ORE"
          value={avgCost / 1e9}
          formatter={(v) => v.toFixed(4)}
          suffix=" SOL"
          color="text-slate-300"
        />
        <StatCard
          label="All-Time Cost/ORE"
          value={(latest?.cumulative_cost_per_ore || 0) / 1e9}
          formatter={(v) => v.toFixed(4)}
          suffix=" SOL"
          color="text-emerald-400"
        />
      </div>

      {/* Chart */}
      <ComposedChart
        data={chartData}
        series={[
          { key: "cost_per_ore_sol", name: "Daily Cost (SOL/ORE)", type: "bar", color: colors.primary },
          { key: "cumulative_cost_sol", name: "Cumulative Avg", type: "line", color: colors.positive, yAxisId: "right" },
        ]}
        xKey="day"
        height={280}
        xFormatter={formatters.date}
        yFormatter={(v) => v.toFixed(4)}
        dualAxis={true}
        showLegend={true}
        showGrid={true}
      />
    </div>
  );
}

function MintChart({ data, range }: { data: (MintHourlyData | MintDailyData)[]; range: TimeRange }) {
  const isHourly = range === "24h" || range === "7d";
  const xKey = isHourly ? "hour" : "day";
  const xFormatter = isHourly ? formatters.dateTime : formatters.date;
  
  const latest = data[data.length - 1];
  const totalChange = data.reduce((sum, d) => sum + d.supply_change_total, 0);

  if (data.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-slate-500">
        <div className="text-center">
          <span className="text-4xl mb-2 block">🪙</span>
          <p>Mint supply tracking begins after deployment</p>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-2 gap-3">
        <StatCard
          label="Current Supply"
          value={latest?.supply || 0}
          formatter={formatters.ore}
          suffix=" ORE"
          color="text-amber-400"
        />
        <StatCard
          label="Period Change"
          value={totalChange}
          formatter={formatters.ore}
          suffix=" ORE"
          color={totalChange >= 0 ? "text-emerald-400" : "text-red-400"}
          prefix={totalChange >= 0 ? "+" : ""}
        />
      </div>

      {/* Chart */}
      <TimeSeriesChart
        data={data}
        series={[
          { key: "supply", name: "Total Supply", color: colors.primary },
        ]}
        xKey={xKey}
        height={280}
        xFormatter={xFormatter}
        yFormatter={formatters.ore}
        showLegend={false}
        showGrid={true}
      />
    </div>
  );
}

function InflationChart({ data, range }: { data: (InflationHourlyData | InflationDailyData)[]; range: TimeRange }) {
  const isHourly = range === "24h" || range === "7d";
  const xKey = isHourly ? "hour" : "day";
  const xFormatter = isHourly ? formatters.dateTime : formatters.date;
  
  const latest = data[data.length - 1] as InflationHourlyData | InflationDailyData | undefined;
  const totalInflation = data.reduce((sum, d) => sum + d.market_inflation_total, 0);

  if (data.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-slate-500">
        <div className="text-center">
          <span className="text-4xl mb-2 block">📈</span>
          <p>Inflation tracking begins after deployment</p>
        </div>
      </div>
    );
  }

  const circulatingEnd = 'circulating_end' in (latest || {})
    ? (latest as InflationHourlyData).circulating_end
    : (latest as InflationDailyData)?.circulating_end;

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="grid grid-cols-2 gap-3">
        <StatCard
          label="Circulating Supply"
          value={circulatingEnd || 0}
          formatter={formatters.ore}
          suffix=" ORE"
          color="text-amber-400"
        />
        <StatCard
          label="Market Inflation"
          value={totalInflation}
          formatter={formatters.ore}
          suffix=" ORE"
          color={totalInflation >= 0 ? "text-emerald-400" : "text-red-400"}
          prefix={totalInflation >= 0 ? "+" : ""}
        />
      </div>

      {/* Chart */}
      <TimeSeriesChart
        data={data}
        series={[
          { key: "market_inflation_total", name: "Market Inflation", color: colors.positive },
        ]}
        xKey={xKey}
        height={280}
        xFormatter={xFormatter}
        yFormatter={formatters.ore}
        showLegend={false}
        showGrid={true}
      />
    </div>
  );
}

// ============================================================================
// Stat Card Component
// ============================================================================

function StatCard({
  label,
  value,
  formatter,
  color = "text-white",
  suffix = "",
  prefix = "",
}: {
  label: string;
  value: number;
  formatter: (v: number) => string;
  color?: string;
  suffix?: string;
  prefix?: string;
}) {
  return (
    <div className="bg-slate-800/30 rounded-lg px-3 py-2">
      <div className="text-xs text-slate-500 mb-0.5">{label}</div>
      <div className={`text-lg font-semibold font-mono ${color}`}>
        {prefix}{formatter(value)}{suffix}
      </div>
    </div>
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
  onChangeType,
  onChangeRange,
}: {
  config: ChartConfig;
  data: unknown[];
  loading: boolean;
  error: string | null;
  onRemove: () => void;
  onChangeType: (type: ChartType) => void;
  onChangeRange: (range: TimeRange) => void;
}) {
  const chartInfo = CHART_TYPES.find(c => c.value === config.type);

  const renderChart = () => {
    if (loading) {
      return (
        <div className="h-64 flex items-center justify-center">
          <div className="flex items-center gap-2 text-slate-400">
            <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
              <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
              <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
            </svg>
            <span>Loading...</span>
          </div>
        </div>
      );
    }

    if (error) {
      return (
        <div className="h-64 flex items-center justify-center">
          <div className="text-center">
            <span className="text-red-400 text-sm">{error}</span>
            <p className="text-slate-500 text-xs mt-1">This data may not be available yet</p>
          </div>
        </div>
      );
    }

    if (data.length === 0) {
      return (
        <div className="h-64 flex items-center justify-center">
          <span className="text-slate-500 text-sm">No data available</span>
        </div>
      );
    }

    switch (config.type) {
      case "rounds":
        return <RoundsChart data={data as (RoundsHourlyData | RoundsDailyData)[]} range={config.range} />;
      case "treasury":
        return <TreasuryChart data={data as TreasuryHourlyData[]} />;
      case "miners":
        return <MinersChart data={data as MinerActivityDailyData[]} />;
      case "cost_per_ore":
        return <CostPerOreChart data={data as CostPerOreDailyData[]} />;
      case "mint":
        return <MintChart data={data as (MintHourlyData | MintDailyData)[]} range={config.range} />;
      case "inflation":
        return <InflationChart data={data as (InflationHourlyData | InflationDailyData)[]} range={config.range} />;
      default:
        return null;
    }
  };

  return (
    <div className="bg-slate-900/50 border border-slate-800/50 rounded-xl overflow-hidden">
      {/* Chart Header */}
      <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
        <div className="flex items-center gap-3">
          {/* Chart Type Selector */}
          <select
            value={config.type}
            onChange={(e) => onChangeType(e.target.value as ChartType)}
            className="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50 cursor-pointer"
          >
            {CHART_TYPES.map(ct => (
              <option key={ct.value} value={ct.value}>
                {ct.icon} {ct.label}
              </option>
            ))}
          </select>

          {/* Time Range Selector */}
          <TimeRangeSelector
            value={config.range as TimeRange}
            onChange={(range) => onChangeRange(range as TimeRange)}
            options={TIME_RANGES.map(tr => ({ value: tr.value, label: tr.label }))}
          />
        </div>

        {/* Remove Button */}
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

      {/* Chart Content */}
      <div className="p-4">
        {renderChart()}
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

  // Sync charts to URL
  useEffect(() => {
    const urlParam = chartsToUrlParam(charts);
    const currentParam = searchParams.get("c");
    const defaultParam = chartsToUrlParam([
      { id: "1", type: "rounds", range: "7d" },
      { id: "2", type: "treasury", range: "7d" },
    ]);

    if (urlParam !== currentParam) {
      if (urlParam === defaultParam) {
        router.replace(pathname, { scroll: false });
      } else {
        router.replace(`${pathname}?c=${urlParam}`, { scroll: false });
      }
    }
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
    setCharts(prev => [...prev, { id: newId, type: "rounds", range: "7d" }]);
  }, [charts.length]);

  const removeChart = useCallback((id: string) => {
    setCharts(prev => prev.filter(c => c.id !== id));
  }, []);

  const updateChart = useCallback((id: string, updates: Partial<ChartConfig>) => {
    setCharts(prev => prev.map(c => c.id === id ? { ...c, ...updates } : c));
  }, []);

  // Generate share URL
  const shareUrl = useMemo(() => {
    if (typeof window === "undefined") return "";
    const base = window.location.origin + pathname;
    const param = chartsToUrlParam(charts);
    return `${base}?c=${param}`;
  }, [charts, pathname]);

  const copyShareUrl = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(shareUrl);
    } catch {
      // Fallback for older browsers
      console.log("Share URL:", shareUrl);
    }
  }, [shareUrl]);

  return (
    <div className="min-h-screen bg-slate-950">
      <Header />

      <main className="max-w-7xl mx-auto px-4 py-6">
        {/* Page Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-2xl font-bold text-white">Charts</h1>
            <p className="text-slate-400 text-sm mt-1">
              Customize your dashboard with time series charts
            </p>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={copyShareUrl}
              className="flex items-center gap-2 px-3 py-2 bg-slate-800 hover:bg-slate-700 border border-slate-700 rounded-lg text-sm text-slate-300 transition-colors"
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
                onChangeType={(type) => updateChart(config.id, { type })}
                onChangeRange={(range) => updateChart(config.id, { range })}
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
                        setCharts(prev => [...prev, { id: newId, type: ct.value, range: "7d" }]);
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
