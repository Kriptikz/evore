"use client";

import { useState, useEffect, useCallback, Suspense, useMemo } from "react";
import { useSearchParams, useRouter, usePathname } from "next/navigation";
import { Header } from "@/components/Header";
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
import { formatSol, formatOre } from "@/lib/format";

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

const CHART_TYPES: { value: ChartType; label: string; description: string }[] = [
  { value: "rounds", label: "Round Activity", description: "Deployments, miners, and winnings per time period" },
  { value: "treasury", label: "Treasury", description: "Balance, motherlode, staked, and unclaimed ORE" },
  { value: "miners", label: "Miner Activity", description: "Active miners and deployment volume" },
  { value: "cost_per_ore", label: "Cost per ORE", description: "Daily cost per ORE mined in SOL" },
  { value: "mint", label: "Mint Supply", description: "Total ORE supply changes" },
  { value: "inflation", label: "Market Inflation", description: "Circulating supply and market inflation" },
];

const TIME_RANGES: { value: TimeRange; label: string; hours?: number; days?: number }[] = [
  { value: "24h", label: "24 Hours", hours: 24 },
  { value: "7d", label: "7 Days", days: 7 },
  { value: "30d", label: "30 Days", days: 30 },
  { value: "90d", label: "90 Days", days: 90 },
  { value: "1y", label: "1 Year", days: 365 },
];

const MAX_CHARTS = 6;

// ============================================================================
// URL State Management
// ============================================================================

function parseChartsFromUrl(searchParams: URLSearchParams): ChartConfig[] {
  const chartsParam = searchParams.get("c");
  if (!chartsParam) {
    // Default charts
    return [
      { id: "1", type: "rounds", range: "24h" },
      { id: "2", type: "treasury", range: "24h" },
    ];
  }

  try {
    // Format: "type:range,type:range,..."
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
    return configs.length > 0 ? configs : [{ id: "1", type: "rounds", range: "24h" }];
  } catch {
    return [{ id: "1", type: "rounds", range: "24h" }];
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
      if (hours <= 168) { // 7 days
        return api.getChartRoundsHourly(hours);
      }
      return api.getChartRoundsDaily(days);
    case "treasury":
      return api.getChartTreasuryHourly(hours);
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
// Chart Display Component
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

  return (
    <div className="bg-slate-900/50 border border-slate-800/50 rounded-xl overflow-hidden">
      {/* Chart Header */}
      <div className="px-4 py-3 border-b border-slate-800/50 flex items-center justify-between">
        <div className="flex items-center gap-3">
          <select
            value={config.type}
            onChange={(e) => onChangeType(e.target.value as ChartType)}
            className="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50"
          >
            {CHART_TYPES.map(ct => (
              <option key={ct.value} value={ct.value}>{ct.label}</option>
            ))}
          </select>
          <select
            value={config.range}
            onChange={(e) => onChangeRange(e.target.value as TimeRange)}
            className="bg-slate-800 border border-slate-700 rounded-lg px-3 py-1.5 text-sm text-white focus:outline-none focus:ring-2 focus:ring-amber-500/50"
          >
            {TIME_RANGES.map(tr => (
              <option key={tr.value} value={tr.value}>{tr.label}</option>
            ))}
          </select>
        </div>
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
      <div className="p-4 min-h-[300px]">
        {loading ? (
          <div className="h-full flex items-center justify-center">
            <div className="flex items-center gap-2 text-slate-400">
              <svg className="animate-spin h-5 w-5" viewBox="0 0 24 24">
                <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
              </svg>
              <span>Loading...</span>
            </div>
          </div>
        ) : error ? (
          <div className="h-full flex items-center justify-center">
            <div className="text-center">
              <span className="text-red-400 text-sm">{error}</span>
              <p className="text-slate-500 text-xs mt-1">This data may not be available yet</p>
            </div>
          </div>
        ) : data.length === 0 ? (
          <div className="h-full flex items-center justify-center">
            <span className="text-slate-500 text-sm">No data available</span>
          </div>
        ) : (
          <ChartVisualization type={config.type} range={config.range} data={data} />
        )}
      </div>
    </div>
  );
}

// ============================================================================
// Chart Visualization (temporary table view until charting library is added)
// ============================================================================

function ChartVisualization({
  type,
  range,
  data,
}: {
  type: ChartType;
  range: TimeRange;
  data: unknown[];
}) {
  // For now, render a simple summary + mini bar chart using CSS
  // This can be replaced with a proper charting library later

  if (type === "rounds") {
    const roundsData = data as (RoundsHourlyData | RoundsDailyData)[];
    const totalDeployed = roundsData.reduce((sum, d) => sum + d.total_deployed, 0);
    const totalWinnings = roundsData.reduce((sum, d) => sum + d.total_winnings, 0);
    const totalRounds = roundsData.reduce((sum, d) => sum + d.rounds_count, 0);
    const maxDeployed = Math.max(...roundsData.map(d => d.total_deployed));

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-3 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{totalRounds.toLocaleString()}</div>
            <div className="text-xs text-slate-500">Rounds</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-emerald-400">{formatSol(totalDeployed)}</div>
            <div className="text-xs text-slate-500">Total Deployed</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-purple-400">{formatSol(totalWinnings)}</div>
            <div className="text-xs text-slate-500">Total Won</div>
          </div>
        </div>

        {/* Mini Bar Chart */}
        <div className="h-40 flex items-end gap-0.5 px-2">
          {roundsData.slice(-50).map((d, i) => {
            const height = maxDeployed > 0 ? (d.total_deployed / maxDeployed) * 100 : 0;
            const timestamp = 'hour' in d ? d.hour : d.day;
            return (
              <div
                key={i}
                className="flex-1 bg-gradient-to-t from-amber-600 to-amber-400 rounded-t opacity-80 hover:opacity-100 transition-opacity cursor-pointer group relative"
                style={{ height: `${Math.max(height, 2)}%` }}
                title={`${new Date(timestamp * 1000).toLocaleString()}: ${formatSol(d.total_deployed)} deployed`}
              />
            );
          })}
        </div>
        <div className="text-xs text-slate-500 text-center">
          SOL Deployed over time (last {roundsData.slice(-50).length} periods)
        </div>
      </div>
    );
  }

  if (type === "treasury") {
    const treasuryData = data as TreasuryHourlyData[];
    const latest = treasuryData[treasuryData.length - 1];
    const maxBalance = Math.max(...treasuryData.map(d => d.balance));

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-3 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{formatSol(latest?.balance || 0)}</div>
            <div className="text-xs text-slate-500">Current Balance</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-purple-400">{formatOre(latest?.motherlode || 0)}</div>
            <div className="text-xs text-slate-500">Motherlode</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-slate-300">{formatOre(latest?.total_unclaimed || 0)}</div>
            <div className="text-xs text-slate-500">Unclaimed</div>
          </div>
        </div>

        {/* Mini Line Chart approximation */}
        <div className="h-40 flex items-end gap-0.5 px-2">
          {treasuryData.slice(-50).map((d, i) => {
            const height = maxBalance > 0 ? (d.balance / maxBalance) * 100 : 0;
            return (
              <div
                key={i}
                className="flex-1 bg-gradient-to-t from-emerald-600 to-emerald-400 rounded-t opacity-80 hover:opacity-100 transition-opacity cursor-pointer"
                style={{ height: `${Math.max(height, 2)}%` }}
                title={`${new Date(d.hour * 1000).toLocaleString()}: ${formatSol(d.balance)}`}
              />
            );
          })}
        </div>
        <div className="text-xs text-slate-500 text-center">
          Treasury Balance over time
        </div>
      </div>
    );
  }

  if (type === "miners") {
    const minersData = data as MinerActivityDailyData[];
    const latest = minersData[minersData.length - 1];
    const maxMiners = Math.max(...minersData.map(d => d.active_miners));

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-3 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{(latest?.active_miners || 0).toLocaleString()}</div>
            <div className="text-xs text-slate-500">Active Miners (Today)</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-emerald-400">{(latest?.total_deployments || 0).toLocaleString()}</div>
            <div className="text-xs text-slate-500">Deployments (Today)</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-purple-400">{formatSol(latest?.total_won || 0)}</div>
            <div className="text-xs text-slate-500">Won (Today)</div>
          </div>
        </div>

        {/* Mini Bar Chart */}
        <div className="h-40 flex items-end gap-0.5 px-2">
          {minersData.slice(-50).map((d, i) => {
            const height = maxMiners > 0 ? (d.active_miners / maxMiners) * 100 : 0;
            return (
              <div
                key={i}
                className="flex-1 bg-gradient-to-t from-blue-600 to-blue-400 rounded-t opacity-80 hover:opacity-100 transition-opacity cursor-pointer"
                style={{ height: `${Math.max(height, 2)}%` }}
                title={`${new Date(d.day * 1000).toLocaleDateString()}: ${d.active_miners} miners`}
              />
            );
          })}
        </div>
        <div className="text-xs text-slate-500 text-center">
          Daily Active Miners
        </div>
      </div>
    );
  }

  if (type === "cost_per_ore") {
    const costData = data as CostPerOreDailyData[];
    const latest = costData[costData.length - 1];
    const avgCost = costData.length > 0 
      ? costData.reduce((sum, d) => sum + d.cost_per_ore_lamports, 0) / costData.length 
      : 0;
    const maxCost = Math.max(...costData.map(d => d.cost_per_ore_lamports));

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-3 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{formatSol(latest?.cost_per_ore_lamports || 0)}</div>
            <div className="text-xs text-slate-500">Today's Cost/ORE</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-slate-300">{formatSol(avgCost)}</div>
            <div className="text-xs text-slate-500">Avg Cost/ORE</div>
          </div>
          <div className="text-center">
            <div className="text-2xl font-bold text-emerald-400">{formatSol(latest?.cumulative_cost_per_ore || 0)}</div>
            <div className="text-xs text-slate-500">All-Time Cost/ORE</div>
          </div>
        </div>

        {/* Mini Bar Chart */}
        <div className="h-40 flex items-end gap-0.5 px-2">
          {costData.slice(-50).map((d, i) => {
            const height = maxCost > 0 ? (d.cost_per_ore_lamports / maxCost) * 100 : 0;
            return (
              <div
                key={i}
                className="flex-1 bg-gradient-to-t from-orange-600 to-orange-400 rounded-t opacity-80 hover:opacity-100 transition-opacity cursor-pointer"
                style={{ height: `${Math.max(height, 2)}%` }}
                title={`${new Date(d.day * 1000).toLocaleDateString()}: ${formatSol(d.cost_per_ore_lamports)} SOL/ORE`}
              />
            );
          })}
        </div>
        <div className="text-xs text-slate-500 text-center">
          Daily Cost per ORE (in SOL)
        </div>
      </div>
    );
  }

  if (type === "mint") {
    const mintData = data as (MintHourlyData | MintDailyData)[];
    const latest = mintData[mintData.length - 1];
    const totalChange = mintData.reduce((sum, d) => sum + d.supply_change_total, 0);

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-2 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{formatOre(latest?.supply || 0)}</div>
            <div className="text-xs text-slate-500">Current Supply</div>
          </div>
          <div className="text-center">
            <div className={`text-2xl font-bold ${totalChange >= 0 ? 'text-emerald-400' : 'text-red-400'}`}>
              {totalChange >= 0 ? '+' : ''}{formatOre(totalChange)}
            </div>
            <div className="text-xs text-slate-500">Period Change</div>
          </div>
        </div>

        <div className="text-center text-slate-500 text-sm py-8">
          📊 Mint supply tracking begins after deployment
        </div>
      </div>
    );
  }

  if (type === "inflation") {
    const inflationData = data as (InflationHourlyData | InflationDailyData)[];
    const latest = inflationData[inflationData.length - 1];
    const circulatingEnd = 'circulating_end' in (latest || {}) 
      ? (latest as InflationHourlyData).circulating_end 
      : (latest as InflationDailyData)?.circulating_end;
    const totalInflation = inflationData.reduce((sum, d) => sum + d.market_inflation_total, 0);

    return (
      <div className="space-y-4">
        {/* Summary Stats */}
        <div className="grid grid-cols-2 gap-4">
          <div className="text-center">
            <div className="text-2xl font-bold text-amber-400">{formatOre(circulatingEnd || 0)}</div>
            <div className="text-xs text-slate-500">Circulating Supply</div>
          </div>
          <div className="text-center">
            <div className={`text-2xl font-bold ${totalInflation >= 0 ? 'text-emerald-400' : 'text-red-400'}`}>
              {totalInflation >= 0 ? '+' : ''}{formatOre(totalInflation)}
            </div>
            <div className="text-xs text-slate-500">Market Inflation</div>
          </div>
        </div>

        <div className="text-center text-slate-500 text-sm py-8">
          📊 Inflation tracking begins after deployment
        </div>
      </div>
    );
  }

  return (
    <div className="text-center text-slate-500 py-8">
      Chart visualization coming soon
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
      { id: "1", type: "rounds", range: "24h" },
      { id: "2", type: "treasury", range: "24h" },
    ]);

    if (urlParam !== currentParam) {
      if (urlParam === defaultParam) {
        // Remove param if it's the default
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
      
      // Skip if already loading or has data
      const existing = chartData.get(key);
      if (existing && (existing.loading || existing.data.length > 0)) {
        return;
      }

      // Set loading state
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
    setCharts(prev => [...prev, { id: newId, type: "rounds", range: "24h" }]);
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

  const copyShareUrl = useCallback(() => {
    navigator.clipboard.writeText(shareUrl);
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
              Customize your dashboard with time series charts. URL updates automatically for sharing.
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

        {/* Info Footer */}
        <div className="mt-8 p-4 bg-slate-900/30 border border-slate-800/50 rounded-xl">
          <div className="flex items-start gap-3">
            <span className="text-xl">💡</span>
            <div className="text-sm text-slate-400">
              <p className="font-medium text-slate-300 mb-1">Tips</p>
              <ul className="list-disc list-inside space-y-1">
                <li>Charts are saved in the URL - bookmark or share to save your layout</li>
                <li>You can add up to {MAX_CHARTS} charts</li>
                <li>Mint and Inflation data will appear after the mint snapshot feature is deployed</li>
              </ul>
            </div>
          </div>
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

