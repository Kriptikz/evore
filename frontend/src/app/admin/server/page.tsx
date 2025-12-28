"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { api, ServerMetricsRow } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

export default function ServerMetricsPage() {
  const { isAuthenticated } = useAdmin();
  const [metrics, setMetrics] = useState<ServerMetricsRow[]>([]);
  const [hours, setHours] = useState(24);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    
    try {
      setLoading(true);
      const res = await api.getServerMetrics(hours, 200);
      setMetrics(res.metrics);
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

  // Calculate aggregates
  const latest = metrics[0];
  const totalRequests = metrics.reduce((sum, m) => sum + Number(m.requests_total), 0);
  const totalSuccess = metrics.reduce((sum, m) => sum + Number(m.requests_success), 0);
  const totalErrors = metrics.reduce((sum, m) => sum + Number(m.requests_error), 0);
  const avgLatency = metrics.length > 0 
    ? metrics.reduce((sum, m) => sum + m.latency_avg, 0) / metrics.length 
    : 0;

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
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
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
            <div className="text-2xl font-bold text-yellow-400">{avgLatency.toFixed(1)}ms</div>
          </div>
        </div>

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
                <div className="text-xl font-bold text-white">{latest.latency_p50.toFixed(1)}ms</div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">P95 Latency</div>
                <div className="text-xl font-bold text-yellow-400">{latest.latency_p95.toFixed(1)}ms</div>
              </div>
              <div>
                <div className="text-slate-400 text-sm">P99 Latency</div>
                <div className="text-xl font-bold text-orange-400">{latest.latency_p99.toFixed(1)}ms</div>
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
                    <td className="px-4 py-2 text-right text-slate-300">
                      {m.latency_avg.toFixed(1)}ms
                    </td>
                    <td className="px-4 py-2 text-right text-yellow-400">
                      {m.latency_p95.toFixed(1)}ms
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
