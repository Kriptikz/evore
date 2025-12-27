"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { MetricCard } from "@/components/admin/MetricCard";
import { useAdmin } from "@/context/AdminContext";
import { api, RpcSummaryRow, RpcErrorRow, RpcTimeseriesRow } from "@/lib/api";

type TimeRange = 1 | 6 | 24 | 168; // hours

function formatNumber(n: number): string {
  if (n >= 1_000_000) {
    return (n / 1_000_000).toFixed(1) + "M";
  }
  if (n >= 1_000) {
    return (n / 1_000).toFixed(1) + "K";
  }
  return n.toLocaleString();
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000) {
    return (bytes / 1_000_000_000).toFixed(2) + " GB";
  }
  if (bytes >= 1_000_000) {
    return (bytes / 1_000_000).toFixed(2) + " MB";
  }
  if (bytes >= 1_000) {
    return (bytes / 1_000).toFixed(2) + " KB";
  }
  return bytes + " B";
}

export default function RpcMetricsPage() {
  const { isAuthenticated } = useAdmin();
  const [timeRange, setTimeRange] = useState<TimeRange>(24);
  const [summary, setSummary] = useState<RpcSummaryRow[]>([]);
  const [errors, setErrors] = useState<RpcErrorRow[]>([]);
  const [timeseries, setTimeseries] = useState<RpcTimeseriesRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<"summary" | "errors" | "timeseries">("summary");

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    
    try {
      setLoading(true);
      const [summaryData, errorsData, timeseriesData] = await Promise.all([
        api.getRpcSummary(timeRange),
        api.getRpcErrors(timeRange, 50),
        api.getRpcTimeseries(timeRange > 24 ? 24 : timeRange), // Limit timeseries to 24h
      ]);
      setSummary(summaryData.data);
      setErrors(errorsData.errors);
      setTimeseries(timeseriesData.timeseries);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, [timeRange, isAuthenticated]);

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    fetchData();
  }, [fetchData, isAuthenticated]);

  // Calculate totals
  const totals = summary.reduce(
    (acc, row) => ({
      requests: acc.requests + row.total_requests,
      success: acc.success + row.success_count,
      errors: acc.errors + row.error_count,
      timeouts: acc.timeouts + row.timeout_count,
      rateLimited: acc.rateLimited + row.rate_limited_count,
      requestBytes: acc.requestBytes + row.total_request_bytes,
      responseBytes: acc.responseBytes + row.total_response_bytes,
    }),
    { requests: 0, success: 0, errors: 0, timeouts: 0, rateLimited: 0, requestBytes: 0, responseBytes: 0 }
  );

  const successRate = totals.requests > 0 
    ? ((totals.success / totals.requests) * 100).toFixed(2) 
    : "0";

  return (
    <AdminShell title="RPC Metrics" subtitle="Monitor RPC usage and performance">
      <div className="space-y-6">
        {/* Time Range Selector */}
        <div className="flex items-center gap-4">
          <span className="text-sm text-slate-400">Time Range:</span>
          <div className="flex gap-2">
            {([1, 6, 24, 168] as TimeRange[]).map((hours) => (
              <button
                key={hours}
                onClick={() => setTimeRange(hours)}
                className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                  timeRange === hours
                    ? "bg-blue-500 text-white"
                    : "bg-slate-700 text-slate-300 hover:bg-slate-600"
                }`}
              >
                {hours === 168 ? "7d" : hours === 24 ? "24h" : `${hours}h`}
              </button>
            ))}
          </div>
          <button
            onClick={fetchData}
            className="ml-auto px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
          >
            Refresh
          </button>
        </div>

        {loading && summary.length === 0 ? (
          <div className="flex items-center justify-center h-64">
            <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
          </div>
        ) : error ? (
          <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400">
            {error}
          </div>
        ) : (
          <>
            {/* Overview Cards */}
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
              <MetricCard
                title="Total Requests"
                value={formatNumber(totals.requests)}
                color="blue"
              />
              <MetricCard
                title="Success Rate"
                value={`${successRate}%`}
                color={Number(successRate) >= 99 ? "green" : Number(successRate) >= 95 ? "amber" : "red"}
              />
              <MetricCard
                title="Errors"
                value={formatNumber(totals.errors)}
                color={totals.errors > 0 ? "red" : "slate"}
              />
              <MetricCard
                title="Timeouts"
                value={formatNumber(totals.timeouts)}
                color={totals.timeouts > 0 ? "amber" : "slate"}
              />
              <MetricCard
                title="Data Sent"
                value={formatBytes(totals.requestBytes)}
                color="slate"
              />
              <MetricCard
                title="Data Received"
                value={formatBytes(totals.responseBytes)}
                color="slate"
              />
            </div>

            {/* Tabs */}
            <div className="border-b border-slate-700">
              <nav className="flex gap-4">
                {(["summary", "errors", "timeseries"] as const).map((tab) => (
                  <button
                    key={tab}
                    onClick={() => setActiveTab(tab)}
                    className={`px-4 py-3 text-sm font-medium border-b-2 transition-colors ${
                      activeTab === tab
                        ? "text-blue-400 border-blue-400"
                        : "text-slate-400 border-transparent hover:text-white"
                    }`}
                  >
                    {tab === "summary" && "By Method"}
                    {tab === "errors" && `Errors (${errors.length})`}
                    {tab === "timeseries" && "Time Series"}
                  </button>
                ))}
              </nav>
            </div>

            {/* Tab Content */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
              {activeTab === "summary" && (
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-slate-700">
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Method</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Provider</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Program</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Requests</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Success</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Errors</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Avg</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Max</th>
                    </tr>
                  </thead>
                  <tbody>
                    {summary.length === 0 ? (
                      <tr>
                        <td colSpan={8} className="px-4 py-8 text-center text-slate-400">
                          No RPC data available for this time range
                        </td>
                      </tr>
                    ) : (
                      summary.map((row, i) => (
                        <tr key={i} className="border-b border-slate-700/50 last:border-0 hover:bg-slate-700/30">
                          <td className="px-4 py-3 text-white font-mono text-sm">{row.method}</td>
                          <td className="px-4 py-3 text-slate-300">{row.provider}</td>
                          <td className="px-4 py-3 text-slate-400">{row.program}</td>
                          <td className="px-4 py-3 text-right text-white">{formatNumber(row.total_requests)}</td>
                          <td className="px-4 py-3 text-right text-green-400">{formatNumber(row.success_count)}</td>
                          <td className="px-4 py-3 text-right text-red-400">{row.error_count}</td>
                          <td className="px-4 py-3 text-right text-slate-300">{row.avg_duration_ms.toFixed(0)}ms</td>
                          <td className="px-4 py-3 text-right text-slate-400">{row.max_duration_ms}ms</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              )}

              {activeTab === "errors" && (
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-slate-700">
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Time</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Method</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Provider</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Status</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Error</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Duration</th>
                    </tr>
                  </thead>
                  <tbody>
                    {errors.length === 0 ? (
                      <tr>
                        <td colSpan={6} className="px-4 py-8 text-center text-slate-400">
                          No errors in this time range 🎉
                        </td>
                      </tr>
                    ) : (
                      errors.map((row, i) => (
                        <tr key={i} className="border-b border-slate-700/50 last:border-0 hover:bg-slate-700/30">
                          <td className="px-4 py-3 text-slate-400 text-sm font-mono">
                            {new Date(row.timestamp).toLocaleTimeString()}
                          </td>
                          <td className="px-4 py-3 text-white font-mono text-sm">{row.method}</td>
                          <td className="px-4 py-3 text-slate-300">{row.provider}</td>
                          <td className="px-4 py-3">
                            <span className={`px-2 py-1 text-xs rounded ${
                              row.status === "timeout" ? "bg-amber-500/20 text-amber-400" :
                              row.status === "rate_limited" ? "bg-purple-500/20 text-purple-400" :
                              "bg-red-500/20 text-red-400"
                            }`}>
                              {row.status}
                            </span>
                          </td>
                          <td className="px-4 py-3 text-red-300 text-sm truncate max-w-xs" title={row.error_message}>
                            {row.error_code ? `[${row.error_code}] ` : ""}{row.error_message || "-"}
                          </td>
                          <td className="px-4 py-3 text-right text-slate-400">{row.duration_ms}ms</td>
                        </tr>
                      ))
                    )}
                  </tbody>
                </table>
              )}

              {activeTab === "timeseries" && (
                <div className="p-4">
                  {timeseries.length === 0 ? (
                    <p className="text-center text-slate-400 py-8">No time series data available</p>
                  ) : (
                    <div className="space-y-4">
                      {/* Simple bar chart representation */}
                      <div className="h-48 flex items-end gap-0.5">
                        {timeseries.slice(-60).map((row, i) => {
                          const maxRequests = Math.max(...timeseries.map(r => r.total_requests));
                          const height = maxRequests > 0 ? (row.total_requests / maxRequests) * 100 : 0;
                          const errorRate = row.total_requests > 0 
                            ? (row.error_count / row.total_requests) * 100 
                            : 0;
                          
                          return (
                            <div
                              key={i}
                              className="flex-1 min-w-1 group relative"
                              title={`${new Date(row.minute).toLocaleTimeString()}: ${row.total_requests} requests`}
                            >
                              <div
                                className={`w-full rounded-t transition-all ${
                                  errorRate > 5 ? "bg-red-500" :
                                  errorRate > 0 ? "bg-amber-500" :
                                  "bg-blue-500"
                                }`}
                                style={{ height: `${height}%` }}
                              />
                            </div>
                          );
                        })}
                      </div>
                      <div className="flex justify-between text-xs text-slate-500">
                        <span>
                          {timeseries.length > 0 && new Date(timeseries[Math.max(0, timeseries.length - 60)].minute).toLocaleTimeString()}
                        </span>
                        <span>
                          {timeseries.length > 0 && new Date(timeseries[timeseries.length - 1].minute).toLocaleTimeString()}
                        </span>
                      </div>
                      <p className="text-xs text-slate-500 text-center">
                        Requests per minute (last 60 minutes shown)
                      </p>
                    </div>
                  )}
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </AdminShell>
  );
}

