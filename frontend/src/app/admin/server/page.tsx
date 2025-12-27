"use client";

import { useEffect, useState } from "react";
import { api, ServerMetricsRow } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

export default function ServerMetricsPage() {
  const { isAuthenticated } = useAdmin();
  const [metrics, setMetrics] = useState<ServerMetricsRow[]>([]);
  const [hours, setHours] = useState(24);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isAuthenticated) return;
    
    async function fetchData() {
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
    }

    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [isAuthenticated, hours]);

  if (!isAuthenticated) {
    return (
      <div className="p-6 text-center text-gray-400">
        Please login to view server metrics.
      </div>
    );
  }

  if (loading && metrics.length === 0) {
    return (
      <div className="p-6 text-center text-gray-400">
        Loading server metrics...
      </div>
    );
  }

  // Calculate aggregates
  const latest = metrics[0];
  const totalRequests = metrics.reduce((sum, m) => sum + Number(m.requests_total), 0);
  const totalSuccess = metrics.reduce((sum, m) => sum + Number(m.requests_success), 0);
  const totalErrors = metrics.reduce((sum, m) => sum + Number(m.requests_error), 0);
  const avgLatency = metrics.length > 0 
    ? metrics.reduce((sum, m) => sum + m.latency_avg, 0) / metrics.length 
    : 0;

  return (
    <div className="p-6 space-y-6">
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold text-white">Server Metrics</h1>
        <select
          value={hours}
          onChange={(e) => setHours(parseInt(e.target.value))}
          className="bg-gray-800 text-white px-3 py-2 rounded border border-gray-700"
        >
          <option value="1">Last 1 hour</option>
          <option value="6">Last 6 hours</option>
          <option value="24">Last 24 hours</option>
          <option value="48">Last 48 hours</option>
          <option value="168">Last 7 days</option>
        </select>
      </div>

      {error && (
        <div className="bg-red-900/50 text-red-200 p-4 rounded">
          {error}
        </div>
      )}

      {/* Summary Cards */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Total Requests</div>
          <div className="text-2xl font-bold text-white">{totalRequests.toLocaleString()}</div>
        </div>
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Success Rate</div>
          <div className="text-2xl font-bold text-green-400">
            {totalRequests > 0 ? ((totalSuccess / totalRequests) * 100).toFixed(1) : 0}%
          </div>
        </div>
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Errors</div>
          <div className="text-2xl font-bold text-red-400">{totalErrors.toLocaleString()}</div>
        </div>
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Avg Latency</div>
          <div className="text-2xl font-bold text-yellow-400">{avgLatency.toFixed(1)}ms</div>
        </div>
      </div>

      {/* Latest Snapshot */}
      {latest && (
        <div className="bg-gray-800 rounded p-4">
          <h2 className="text-lg font-semibold text-white mb-4">Latest Snapshot</h2>
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
            <div>
              <div className="text-gray-400 text-sm">Active Connections</div>
              <div className="text-xl font-bold text-white">{latest.active_connections}</div>
            </div>
            <div>
              <div className="text-gray-400 text-sm">Memory Used</div>
              <div className="text-xl font-bold text-white">{formatBytes(Number(latest.memory_used))}</div>
            </div>
            <div>
              <div className="text-gray-400 text-sm">P50 Latency</div>
              <div className="text-xl font-bold text-white">{latest.latency_p50.toFixed(1)}ms</div>
            </div>
            <div>
              <div className="text-gray-400 text-sm">P95 Latency</div>
              <div className="text-xl font-bold text-yellow-400">{latest.latency_p95.toFixed(1)}ms</div>
            </div>
            <div>
              <div className="text-gray-400 text-sm">P99 Latency</div>
              <div className="text-xl font-bold text-orange-400">{latest.latency_p99.toFixed(1)}ms</div>
            </div>
            <div>
              <div className="text-gray-400 text-sm">Cache Hit Rate</div>
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
      <div className="bg-gray-800 rounded overflow-hidden">
        <div className="px-4 py-3 border-b border-gray-700">
          <h2 className="text-lg font-semibold text-white">Historical Snapshots</h2>
        </div>
        <div className="overflow-x-auto max-h-96">
          <table className="w-full text-sm">
            <thead className="bg-gray-900/50 sticky top-0">
              <tr>
                <th className="px-4 py-2 text-left text-gray-400">Time</th>
                <th className="px-4 py-2 text-right text-gray-400">Requests</th>
                <th className="px-4 py-2 text-right text-gray-400">Success</th>
                <th className="px-4 py-2 text-right text-gray-400">Errors</th>
                <th className="px-4 py-2 text-right text-gray-400">Avg Latency</th>
                <th className="px-4 py-2 text-right text-gray-400">P95</th>
                <th className="px-4 py-2 text-right text-gray-400">Connections</th>
                <th className="px-4 py-2 text-right text-gray-400">Memory</th>
              </tr>
            </thead>
            <tbody>
              {metrics.map((m, i) => (
                <tr key={i} className="border-t border-gray-700/50 hover:bg-gray-700/30">
                  <td className="px-4 py-2 text-gray-400 font-mono text-xs">
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
                  <td className="px-4 py-2 text-right text-gray-300">
                    {m.latency_avg.toFixed(1)}ms
                  </td>
                  <td className="px-4 py-2 text-right text-yellow-400">
                    {m.latency_p95.toFixed(1)}ms
                  </td>
                  <td className="px-4 py-2 text-right text-gray-300">
                    {m.active_connections}
                  </td>
                  <td className="px-4 py-2 text-right text-gray-300">
                    {formatBytes(Number(m.memory_used))}
                  </td>
                </tr>
              ))}
              {metrics.length === 0 && (
                <tr>
                  <td colSpan={8} className="px-4 py-8 text-center text-gray-500">
                    No server metrics in the selected time range.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

