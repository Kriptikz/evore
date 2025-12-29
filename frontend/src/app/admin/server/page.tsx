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

  // Calculate chart dimensions (handle empty/short arrays safely)
  const chartHeight = 150;
  const maxRequests = timeseries.length > 0 
    ? Math.max(...timeseries.map(t => t.request_count), 1) 
    : 1;
  const maxLatency = timeseries.length > 0 
    ? Math.max(...timeseries.map(t => t.avg_latency_ms), 1) 
    : 1;

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

        {/* Requests Per Minute Chart (requires at least 2 data points) */}
        {timeseries.length >= 2 && (
          <div className="bg-slate-800/50 border border-slate-700 rounded-lg p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Requests Per Minute</h2>
            <RequestsChart 
              data={timeseries} 
              height={chartHeight} 
              maxRequests={maxRequests}
            />
          </div>
        )}
        {timeseries.length === 1 && (
          <div className="bg-slate-800/50 border border-slate-700 rounded-lg p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Requests Per Minute</h2>
            <div className="text-slate-400 text-center py-8">
              Only 1 data point available. Chart requires at least 2 points.
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

// SVG Line Chart Component
function RequestsChart({ 
  data, 
  height, 
  maxRequests 
}: { 
  data: RequestsPerMinuteRow[]; 
  height: number; 
  maxRequests: number;
}) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  
  // Need at least 2 points to draw a line
  if (data.length < 2) return null;
  
  const width = 800; // Will be scaled by viewBox
  const padding = { top: 20, right: 20, bottom: 30, left: 50 };
  const chartWidth = width - padding.left - padding.right;
  const chartHeight = height - padding.top - padding.bottom;
  
  // Generate path for the line (safe division - data.length >= 2 guaranteed above)
  const getX = (index: number) => padding.left + (index / Math.max(data.length - 1, 1)) * chartWidth;
  const getY = (value: number) => padding.top + chartHeight - (value / Math.max(maxRequests, 1)) * chartHeight;
  
  // Create line path for total requests
  const linePath = data.map((point, i) => {
    const x = getX(i);
    const y = getY(point.request_count);
    return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
  }).join(' ');
  
  // Create area path (filled area under the line)
  const areaPath = `${linePath} L ${getX(data.length - 1)} ${padding.top + chartHeight} L ${getX(0)} ${padding.top + chartHeight} Z`;
  
  // Create line path for errors
  const errorLinePath = data.map((point, i) => {
    const x = getX(i);
    const y = getY(point.error_count);
    return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
  }).join(' ');
  
  // Y-axis labels
  const yLabels = [0, maxRequests / 2, maxRequests].map(v => ({
    value: v,
    y: getY(v),
    label: formatNumber(Math.round(v))
  }));
  
  // X-axis labels (show up to 5 labels, deduplicated for short datasets)
  const xLabelIndices = Array.from(new Set([
    0, 
    Math.floor(data.length / 4), 
    Math.floor(data.length / 2), 
    Math.floor(3 * data.length / 4), 
    data.length - 1
  ].filter(idx => idx >= 0 && idx < data.length)));
  
  const hoveredPoint = hoveredIndex !== null ? data[hoveredIndex] : null;

  return (
    <div className="relative">
      <svg 
        viewBox={`0 0 ${width} ${height}`} 
        className="w-full h-auto"
        preserveAspectRatio="xMidYMid meet"
      >
        {/* Grid lines */}
        {yLabels.map((label, i) => (
          <line 
            key={i}
            x1={padding.left} 
            y1={label.y} 
            x2={width - padding.right} 
            y2={label.y}
            stroke="#334155"
            strokeWidth="1"
            strokeDasharray="4"
          />
        ))}
        
        {/* Y-axis labels */}
        {yLabels.map((label, i) => (
          <text 
            key={i}
            x={padding.left - 8} 
            y={label.y + 4}
            textAnchor="end"
            className="fill-slate-500 text-xs"
            style={{ fontSize: '10px' }}
          >
            {label.label}
          </text>
        ))}
        
        {/* Filled area under the line */}
        <path 
          d={areaPath} 
          fill="url(#gradient)" 
          opacity="0.3"
        />
        
        {/* Gradient definition */}
        <defs>
          <linearGradient id="gradient" x1="0%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="#22c55e" stopOpacity="0.6" />
            <stop offset="100%" stopColor="#22c55e" stopOpacity="0.1" />
          </linearGradient>
        </defs>
        
        {/* Main line (requests) */}
        <path 
          d={linePath} 
          fill="none" 
          stroke="#22c55e" 
          strokeWidth="2"
          strokeLinejoin="round"
          strokeLinecap="round"
        />
        
        {/* Error line */}
        <path 
          d={errorLinePath} 
          fill="none" 
          stroke="#ef4444" 
          strokeWidth="2"
          strokeLinejoin="round"
          strokeLinecap="round"
          opacity="0.8"
        />
        
        {/* Data points for hover detection */}
        {data.map((point, i) => (
          <circle
            key={i}
            cx={getX(i)}
            cy={getY(point.request_count)}
            r={hoveredIndex === i ? 6 : 4}
            fill={hoveredIndex === i ? "#22c55e" : "transparent"}
            stroke={hoveredIndex === i ? "#22c55e" : "transparent"}
            strokeWidth="2"
            className="cursor-pointer"
            onMouseEnter={() => setHoveredIndex(i)}
            onMouseLeave={() => setHoveredIndex(null)}
          />
        ))}
        
        {/* Hover indicator line */}
        {hoveredIndex !== null && (
          <line
            x1={getX(hoveredIndex)}
            y1={padding.top}
            x2={getX(hoveredIndex)}
            y2={padding.top + chartHeight}
            stroke="#64748b"
            strokeWidth="1"
            strokeDasharray="4"
          />
        )}
        
        {/* X-axis labels */}
        {xLabelIndices.map((idx) => (
          <text 
            key={idx}
            x={getX(idx)} 
            y={height - 8}
            textAnchor="middle"
            className="fill-slate-500"
            style={{ fontSize: '10px' }}
          >
            {new Date(data[idx].minute_ts * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
          </text>
        ))}
      </svg>
      
      {/* Tooltip */}
      {hoveredPoint && hoveredIndex !== null && (
        <div 
          className="absolute bg-slate-900 border border-slate-700 rounded-lg px-3 py-2 text-xs shadow-lg pointer-events-none z-10"
          style={{
            left: `${(hoveredIndex / (data.length - 1)) * 100}%`,
            top: '10px',
            transform: 'translateX(-50%)',
          }}
        >
          <div className="text-slate-400 mb-1">
            {new Date(hoveredPoint.minute_ts * 1000).toLocaleString()}
          </div>
          <div className="text-white font-semibold">{hoveredPoint.request_count} requests</div>
          <div className="text-green-400">{hoveredPoint.success_count} success</div>
          <div className="text-red-400">{hoveredPoint.error_count} errors</div>
          <div className="text-yellow-400">{hoveredPoint.avg_latency_ms.toFixed(1)}ms avg</div>
        </div>
      )}
      
      {/* Legend */}
      <div className="flex gap-4 mt-3 text-xs text-slate-400">
        <div className="flex items-center gap-2">
          <div className="w-4 h-0.5 bg-green-500 rounded" />
          <span>Requests</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-4 h-0.5 bg-red-500 rounded" />
          <span>Errors</span>
        </div>
      </div>
    </div>
  );
}
