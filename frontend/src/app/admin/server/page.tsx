"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { api, ServerMetricsRow, RequestsPerMinuteRow } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

export default function ServerMetricsPage() {
  const { isAuthenticated } = useAdmin();
  const [metrics, setMetrics] = useState<ServerMetricsRow[]>([]);
  const [timeseries, setTimeseries] = useState<RequestsPerMinuteRow[]>([]);
  const [rps, setRps] = useState<number>(0);
  const [hours, setHours] = useState(24);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    
    try {
      setLoading(true);
      const [metricsRes, timeseriesRes] = await Promise.all([
        api.getServerMetrics(hours, 200),
        api.getRequestsTimeseries(hours),
      ]);
      setMetrics(metricsRes.metrics);
      setTimeseries(timeseriesRes.timeseries);
      setRps(timeseriesRes.rps);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated, hours]);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

  // Calculate aggregates - filter out negative/invalid latency values
  const validMetrics = useMemo(() => 
    metrics.filter(m => m.latency_avg >= 0), 
    [metrics]
  );
  
  const latest = metrics[0];
  const totalRequests = metrics.reduce((sum, m) => sum + Number(m.requests_total), 0);
  const totalSuccess = metrics.reduce((sum, m) => sum + Number(m.requests_success), 0);
  const totalErrors = metrics.reduce((sum, m) => sum + Number(m.requests_error), 0);
  const avgLatency = validMetrics.length > 0 
    ? validMetrics.reduce((sum, m) => sum + m.latency_avg, 0) / validMetrics.length 
    : 0;

  // Calculate chart dimensions
  const chartHeight = 150;
  const maxRequests = Math.max(...timeseries.map(t => t.request_count), 1);
  const maxLatency = Math.max(...timeseries.map(t => t.avg_latency_ms), 1);

  return (
    <AdminShell title="Server Metrics" subtitle="Performance and resource monitoring">
      <div className="space-y-6">
        {/* Controls Row */}
        <div className="flex justify-between items-center">
          <select
            value={hours}
            onChange={(e) => setHours(parseInt(e.target.value))}
            className="bg-slate-800 text-white px-3 py-2 rounded-lg border border-slate-700 focus:ring-blue-500 focus:border-blue-500"
          >
            <option value="1">Last 1 hour</option>
            <option value="6">Last 6 hours</option>
            <option value="24">Last 24 hours</option>
            <option value="48">Last 48 hours</option>
            <option value="168">Last 7 days</option>
          </select>
          <button
            onClick={fetchData}
            disabled={loading}
            className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors disabled:opacity-50"
          >
            {loading ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        {error && (
          <div className="bg-red-500/20 border border-red-500/40 text-red-400 p-4 rounded-lg">
            {error}
          </div>
        )}

        {/* Summary Cards */}
        <div className="grid grid-cols-1 md:grid-cols-5 gap-4">
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Current RPS</div>
            <div className="text-2xl font-bold text-cyan-400">{rps.toFixed(2)}</div>
            <div className="text-xs text-slate-500">requests/second</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Total Requests</div>
            <div className="text-2xl font-bold text-white">{totalRequests.toLocaleString()}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Success Rate</div>
            <div className="text-2xl font-bold text-green-400">
              {totalRequests > 0 ? ((totalSuccess / totalRequests) * 100).toFixed(1) : 0}%
            </div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Errors</div>
            <div className="text-2xl font-bold text-red-400">{totalErrors.toLocaleString()}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Avg Latency</div>
            <div className="text-2xl font-bold text-yellow-400">
              {avgLatency >= 0 ? avgLatency.toFixed(1) : 0}ms
            </div>
          </div>
        </div>

        {/* Requests Per Minute Chart */}
        {timeseries.length > 0 && (
          <div className="bg-slate-800/50 border border-slate-700 rounded-lg p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Requests Per Minute</h2>
            <div className="relative" style={{ height: chartHeight + 40 }}>
              {/* Y-axis labels */}
              <div className="absolute left-0 top-0 h-full w-12 flex flex-col justify-between text-xs text-slate-500">
                <span>{formatNumber(maxRequests)}</span>
                <span>{formatNumber(Math.round(maxRequests / 2))}</span>
                <span>0</span>
              </div>
              
              {/* Chart area */}
              <div className="ml-14 relative" style={{ height: chartHeight }}>
                {/* Grid lines */}
                <div className="absolute inset-0 flex flex-col justify-between pointer-events-none">
                  <div className="border-b border-slate-700/50" />
                  <div className="border-b border-slate-700/50" />
                  <div className="border-b border-slate-700/50" />
                </div>
                
                {/* Bars */}
                <div className="flex items-end h-full gap-px">
                  {timeseries.map((point, i) => {
                    const height = (point.request_count / maxRequests) * 100;
                    const successHeight = (point.success_count / maxRequests) * 100;
                    const errorHeight = (point.error_count / maxRequests) * 100;
                    
                    return (
                      <div 
                        key={i} 
                        className="flex-1 flex flex-col justify-end group relative"
                        style={{ minWidth: 2, maxWidth: 8 }}
                      >
                        {/* Tooltip */}
                        <div className="absolute bottom-full mb-2 left-1/2 -translate-x-1/2 hidden group-hover:block z-10">
                          <div className="bg-slate-900 border border-slate-700 rounded px-2 py-1 text-xs whitespace-nowrap">
                            <div className="text-slate-400">
                              {new Date(point.minute_ts * 1000).toLocaleTimeString()}
                            </div>
                            <div className="text-white font-semibold">{point.request_count} req</div>
                            <div className="text-green-400">{point.success_count} success</div>
                            <div className="text-red-400">{point.error_count} errors</div>
                            <div className="text-yellow-400">{point.avg_latency_ms.toFixed(1)}ms</div>
                          </div>
                        </div>
                        
                        {/* Bar with error portion on top */}
                        <div 
                          className="w-full bg-green-500/60 rounded-t-sm"
                          style={{ height: `${successHeight}%` }}
                        />
                        {errorHeight > 0 && (
                          <div 
                            className="w-full bg-red-500"
                            style={{ height: `${errorHeight}%` }}
                          />
                        )}
                      </div>
                    );
                  })}
                </div>
              </div>
              
              {/* X-axis labels */}
              <div className="ml-14 flex justify-between text-xs text-slate-500 mt-2">
                {timeseries.length > 0 && (
                  <>
                    <span>{new Date(timeseries[0].minute_ts * 1000).toLocaleTimeString()}</span>
                    <span>{new Date(timeseries[Math.floor(timeseries.length / 2)]?.minute_ts * 1000).toLocaleTimeString()}</span>
                    <span>{new Date(timeseries[timeseries.length - 1].minute_ts * 1000).toLocaleTimeString()}</span>
                  </>
                )}
              </div>
            </div>
            
            {/* Legend */}
            <div className="flex gap-4 mt-3 text-xs text-slate-400">
              <div className="flex items-center gap-1">
                <div className="w-3 h-3 bg-green-500/60 rounded-sm" />
                <span>Success</span>
              </div>
              <div className="flex items-center gap-1">
                <div className="w-3 h-3 bg-red-500 rounded-sm" />
                <span>Errors</span>
              </div>
            </div>
          </div>
        )}

        {/* Latest Snapshot */}
        {latest && (
          <div className="bg-slate-800/50 border border-slate-700 rounded-lg p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Latest Snapshot</h2>
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
              <div>
                <div className="text-slate-400 text-sm">Active Connections</div>
                <div className="text-xl font-bold text-white">{latest.active_connections}</div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">Memory Used</div>
                <div className="text-xl font-bold text-white">{formatBytes(Number(latest.memory_used))}</div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">P50 Latency</div>
                <div className="text-xl font-bold text-white">
                  {latest.latency_p50 >= 0 ? latest.latency_p50.toFixed(1) : 0}ms
                </div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">P95 Latency</div>
                <div className="text-xl font-bold text-yellow-400">
                  {latest.latency_p95 >= 0 ? latest.latency_p95.toFixed(1) : 0}ms
                </div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">P99 Latency</div>
                <div className="text-xl font-bold text-orange-400">
                  {latest.latency_p99 >= 0 ? latest.latency_p99.toFixed(1) : 0}ms
                </div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">Cache Hit Rate</div>
                <div className="text-xl font-bold text-green-400">
                  {latest.cache_hits + latest.cache_misses > 0 
                    ? ((Number(latest.cache_hits) / (Number(latest.cache_hits) + Number(latest.cache_misses))) * 100).toFixed(1)
                    : 0}%
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Historical Data */}
        <div className="bg-slate-800/50 border border-slate-700 rounded-lg overflow-hidden">
          <div className="px-4 py-3 border-b border-slate-700">
            <h2 className="text-lg font-semibold text-white">Historical Snapshots</h2>
          </div>
          <div className="overflow-x-auto max-h-[500px]">
            <table className="w-full text-sm">
              <thead className="bg-slate-900/50 sticky top-0">
                <tr>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Time</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Requests</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Success</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Errors</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Avg Latency</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">P95</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Connections</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Memory</th>
                </tr>
              </thead>
              <tbody>
                {metrics.map((m, i) => (
                  <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                    <td className="px-4 py-2 text-slate-400 font-mono text-xs">
                      {new Date(m.timestamp * 1000).toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-right text-white">
                      {Number(m.requests_total).toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-right text-green-400">
                      {Number(m.requests_success).toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-right text-red-400">
                      {Number(m.requests_error).toLocaleString()}
                    </td>
                    <td className={`px-4 py-2 text-right ${m.latency_avg < 0 ? 'text-slate-500' : 'text-slate-300'}`}>
                      {m.latency_avg >= 0 ? `${m.latency_avg.toFixed(1)}ms` : 'N/A'}
                    </td>
                    <td className={`px-4 py-2 text-right ${m.latency_p95 < 0 ? 'text-slate-500' : 'text-yellow-400'}`}>
                      {m.latency_p95 >= 0 ? `${m.latency_p95.toFixed(1)}ms` : 'N/A'}
                    </td>
                    <td className="px-4 py-2 text-right text-slate-300">
                      {m.active_connections}
                    </td>
                    <td className="px-4 py-2 text-right text-slate-300">
                      {formatBytes(Number(m.memory_used))}
                    </td>
                  </tr>
                ))}
                {metrics.length === 0 && (
                  <tr>
                    <td colSpan={8} className="px-4 py-8 text-center text-slate-500">
                      No server metrics in the selected time range.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
        </div>
      </div>
    </AdminShell>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatNumber(num: number): string {
  if (num >= 1_000_000) return (num / 1_000_000).toFixed(1) + "M";
  if (num >= 1_000) return (num / 1_000).toFixed(1) + "K";
  return num.toString();
}
