"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { api, WsEventRow, WsThroughputSummary } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

export default function WebSocketMetricsPage() {
  const { isAuthenticated } = useAdmin();
  const [events, setEvents] = useState<WsEventRow[]>([]);
  const [throughput, setThroughput] = useState<WsThroughputSummary[]>([]);
  const [hours, setHours] = useState(24);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    
    try {
      setLoading(true);
      const [eventsRes, throughputRes] = await Promise.all([
        api.getWsEvents(hours, 100),
        api.getWsThroughput(hours),
      ]);
      setEvents(eventsRes.events);
      setThroughput(throughputRes.throughput);
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

  const eventsByType: Record<string, number> = {};
  const errorCount = events.filter(e => e.event === "error").length;
  const disconnectCount = events.filter(e => e.event === "disconnected").length;
  
  events.forEach(e => {
    eventsByType[e.event] = (eventsByType[e.event] || 0) + 1;
  });

  return (
    <AdminShell title="WebSocket Metrics" subtitle="Real-time connection and throughput monitoring">
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
            <div className="text-slate-400 text-sm mb-1">Total Events</div>
            <div className="text-2xl font-bold text-white">{events.length}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Errors</div>
            <div className="text-2xl font-bold text-red-400">{errorCount}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Disconnections</div>
            <div className="text-2xl font-bold text-yellow-400">{disconnectCount}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Active Subscriptions</div>
            <div className="text-2xl font-bold text-green-400">{throughput.length}</div>
          </div>
        </div>

        {/* Throughput Summary */}
        {throughput.length > 0 && (
          <div className="bg-slate-800/50 border border-slate-700 rounded-lg overflow-hidden">
            <div className="px-4 py-3 border-b border-slate-700">
              <h2 className="text-lg font-semibold text-white">Throughput by Subscription</h2>
            </div>
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-slate-900/50">
                  <tr>
                    <th className="px-4 py-3 text-left text-slate-400 font-medium">Type</th>
                    <th className="px-4 py-3 text-left text-slate-400 font-medium">Provider</th>
                    <th className="px-4 py-3 text-right text-slate-400 font-medium">Messages</th>
                    <th className="px-4 py-3 text-right text-slate-400 font-medium">Bytes</th>
                    <th className="px-4 py-3 text-right text-slate-400 font-medium">Avg Process (μs)</th>
                  </tr>
                </thead>
                <tbody>
                  {throughput.map((t, i) => (
                    <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                      <td className="px-4 py-3 text-white font-mono">{t.subscription_type}</td>
                      <td className="px-4 py-3 text-slate-300">{t.provider}</td>
                      <td className="px-4 py-3 text-right text-white">
                        {t.total_messages.toLocaleString()}
                      </td>
                      <td className="px-4 py-3 text-right text-slate-300">
                        {formatBytes(t.total_bytes)}
                      </td>
                      <td className="px-4 py-3 text-right text-slate-300">
                        {t.avg_process_time_us.toFixed(0)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}

        {/* Recent Events */}
        <div className="bg-slate-800/50 border border-slate-700 rounded-lg overflow-hidden">
          <div className="px-4 py-3 border-b border-slate-700">
            <h2 className="text-lg font-semibold text-white">Recent Events</h2>
          </div>
          <div className="overflow-x-auto max-h-[500px]">
            <table className="w-full text-sm">
              <thead className="bg-slate-900/50 sticky top-0">
                <tr>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Time</th>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Event</th>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Type</th>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Provider</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Uptime</th>
                  <th className="px-4 py-3 text-right text-slate-400 font-medium">Messages</th>
                  <th className="px-4 py-3 text-left text-slate-400 font-medium">Reason/Error</th>
                </tr>
              </thead>
              <tbody>
                {events.map((event, i) => (
                  <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                    <td className="px-4 py-2 text-slate-400 font-mono text-xs">
                      {new Date(event.timestamp).toLocaleString()}
                    </td>
                    <td className="px-4 py-2">
                      <span className={`px-2 py-0.5 rounded text-xs font-medium ${getEventColor(event.event)}`}>
                        {event.event}
                      </span>
                    </td>
                    <td className="px-4 py-2 text-white font-mono">{event.subscription_type}</td>
                    <td className="px-4 py-2 text-slate-300">{event.provider}</td>
                    <td className="px-4 py-2 text-right text-slate-300">
                      {formatDuration(event.uptime_seconds)}
                    </td>
                    <td className="px-4 py-2 text-right text-slate-300">
                      {event.messages_received.toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-slate-400 text-xs truncate max-w-xs">
                      {event.error_message || event.disconnect_reason || "-"}
                    </td>
                  </tr>
                ))}
                {events.length === 0 && (
                  <tr>
                    <td colSpan={7} className="px-4 py-8 text-center text-slate-500">
                      No WebSocket events in the selected time range.
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

function getEventColor(event: string): string {
  switch (event) {
    case "connected":
      return "bg-green-500/20 text-green-400";
    case "connecting":
      return "bg-blue-500/20 text-blue-400";
    case "disconnected":
      return "bg-yellow-500/20 text-yellow-400";
    case "error":
      return "bg-red-500/20 text-red-400";
    case "reconnecting":
      return "bg-orange-500/20 text-orange-400";
    default:
      return "bg-slate-700 text-slate-300";
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
  const hours = Math.floor(seconds / 3600);
  const mins = Math.floor((seconds % 3600) / 60);
  return `${hours}h ${mins}m`;
}
