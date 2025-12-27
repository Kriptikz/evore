"use client";

import { useEffect, useState } from "react";
import { api, RequestLogRow, EndpointSummaryRow, RateLimitEventRow, IpActivityRow } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

type TabType = "logs" | "endpoints" | "rate-limits" | "ips";

export default function RequestLogsPage() {
  const { isAuthenticated } = useAdmin();
  const [activeTab, setActiveTab] = useState<TabType>("endpoints");
  const [logs, setLogs] = useState<RequestLogRow[]>([]);
  const [endpoints, setEndpoints] = useState<EndpointSummaryRow[]>([]);
  const [rateLimits, setRateLimits] = useState<RateLimitEventRow[]>([]);
  const [ipActivity, setIpActivity] = useState<IpActivityRow[]>([]);
  const [hours, setHours] = useState(24);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isAuthenticated) return;
    
    async function fetchData() {
      try {
        setLoading(true);
        const [logsRes, endpointsRes, rateLimitsRes, ipRes] = await Promise.all([
          api.getRequestLogs(hours, 200),
          api.getEndpointSummary(hours),
          api.getRateLimitEvents(hours, 100),
          api.getIpActivity(hours, 50),
        ]);
        setLogs(logsRes.logs);
        setEndpoints(endpointsRes.endpoints);
        setRateLimits(rateLimitsRes.events);
        setIpActivity(ipRes.activity);
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
        Please login to view request logs.
      </div>
    );
  }

  if (loading && endpoints.length === 0) {
    return (
      <div className="p-6 text-center text-gray-400">
        Loading request logs...
      </div>
    );
  }

  const totalRequests = endpoints.reduce((sum, e) => sum + Number(e.total_requests), 0);
  const totalErrors = endpoints.reduce((sum, e) => sum + Number(e.error_count), 0);
  const avgLatency = endpoints.length > 0 
    ? endpoints.reduce((sum, e) => sum + e.avg_duration_ms, 0) / endpoints.length 
    : 0;

  const tabs = [
    { id: "endpoints" as TabType, label: "Endpoints", count: endpoints.length },
    { id: "logs" as TabType, label: "Recent Logs", count: logs.length },
    { id: "rate-limits" as TabType, label: "Rate Limits", count: rateLimits.length },
    { id: "ips" as TabType, label: "IP Activity", count: ipActivity.length },
  ];

  return (
    <div className="p-6 space-y-6">
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold text-white">Request Logs</h1>
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
          <div className="text-gray-400 text-sm">Error Requests</div>
          <div className="text-2xl font-bold text-red-400">{totalErrors.toLocaleString()}</div>
        </div>
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Rate Limit Events</div>
          <div className="text-2xl font-bold text-yellow-400">{rateLimits.length}</div>
        </div>
        <div className="bg-gray-800 p-4 rounded">
          <div className="text-gray-400 text-sm">Avg Latency</div>
          <div className="text-2xl font-bold text-blue-400">{avgLatency.toFixed(1)}ms</div>
        </div>
      </div>

      {/* Tabs */}
      <div className="border-b border-gray-700">
        <div className="flex gap-4">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-4 py-2 font-medium transition-colors ${
                activeTab === tab.id
                  ? "text-white border-b-2 border-blue-500"
                  : "text-gray-400 hover:text-gray-200"
              }`}
            >
              {tab.label}
              <span className="ml-2 text-xs bg-gray-700 px-2 py-0.5 rounded">
                {tab.count}
              </span>
            </button>
          ))}
        </div>
      </div>

      {/* Tab Content */}
      <div className="bg-gray-800 rounded overflow-hidden">
        {activeTab === "endpoints" && <EndpointsTab endpoints={endpoints} />}
        {activeTab === "logs" && <LogsTab logs={logs} />}
        {activeTab === "rate-limits" && <RateLimitsTab events={rateLimits} />}
        {activeTab === "ips" && <IpActivityTab activity={ipActivity} />}
      </div>
    </div>
  );
}

function EndpointsTab({ endpoints }: { endpoints: EndpointSummaryRow[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="bg-gray-900/50">
          <tr>
            <th className="px-4 py-2 text-left text-gray-400">Endpoint</th>
            <th className="px-4 py-2 text-right text-gray-400">Requests</th>
            <th className="px-4 py-2 text-right text-gray-400">Success</th>
            <th className="px-4 py-2 text-right text-gray-400">Errors</th>
            <th className="px-4 py-2 text-right text-gray-400">Avg (ms)</th>
            <th className="px-4 py-2 text-right text-gray-400">P95 (ms)</th>
            <th className="px-4 py-2 text-right text-gray-400">Max (ms)</th>
          </tr>
        </thead>
        <tbody>
          {endpoints.map((e, i) => (
            <tr key={i} className="border-t border-gray-700/50 hover:bg-gray-700/30">
              <td className="px-4 py-2 text-white font-mono">{e.endpoint}</td>
              <td className="px-4 py-2 text-right text-white">
                {Number(e.total_requests).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-green-400">
                {Number(e.success_count).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-red-400">
                {Number(e.error_count).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-gray-300">
                {e.avg_duration_ms.toFixed(1)}
              </td>
              <td className="px-4 py-2 text-right text-yellow-400">
                {e.p95_duration_ms.toFixed(1)}
              </td>
              <td className="px-4 py-2 text-right text-orange-400">
                {e.max_duration_ms}
              </td>
            </tr>
          ))}
          {endpoints.length === 0 && (
            <tr>
              <td colSpan={7} className="px-4 py-8 text-center text-gray-500">
                No endpoint data available.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function LogsTab({ logs }: { logs: RequestLogRow[] }) {
  return (
    <div className="overflow-x-auto max-h-96">
      <table className="w-full text-sm">
        <thead className="bg-gray-900/50 sticky top-0">
          <tr>
            <th className="px-4 py-2 text-left text-gray-400">Time</th>
            <th className="px-4 py-2 text-left text-gray-400">Method</th>
            <th className="px-4 py-2 text-left text-gray-400">Endpoint</th>
            <th className="px-4 py-2 text-center text-gray-400">Status</th>
            <th className="px-4 py-2 text-right text-gray-400">Duration</th>
            <th className="px-4 py-2 text-left text-gray-400">IP (hashed)</th>
          </tr>
        </thead>
        <tbody>
          {logs.map((log, i) => (
            <tr key={i} className="border-t border-gray-700/50 hover:bg-gray-700/30">
              <td className="px-4 py-2 text-gray-400 font-mono text-xs">
                {new Date(log.timestamp).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-white">{log.method}</td>
              <td className="px-4 py-2 text-white font-mono">{log.endpoint}</td>
              <td className="px-4 py-2 text-center">
                <span className={`px-2 py-0.5 rounded text-xs font-medium ${getStatusColor(log.status_code)}`}>
                  {log.status_code}
                </span>
              </td>
              <td className="px-4 py-2 text-right text-gray-300">{log.duration_ms}ms</td>
              <td className="px-4 py-2 text-gray-400 font-mono text-xs">{log.ip_hash}</td>
            </tr>
          ))}
          {logs.length === 0 && (
            <tr>
              <td colSpan={6} className="px-4 py-8 text-center text-gray-500">
                No request logs available.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function RateLimitsTab({ events }: { events: RateLimitEventRow[] }) {
  return (
    <div className="overflow-x-auto max-h-96">
      <table className="w-full text-sm">
        <thead className="bg-gray-900/50 sticky top-0">
          <tr>
            <th className="px-4 py-2 text-left text-gray-400">Time</th>
            <th className="px-4 py-2 text-left text-gray-400">IP (hashed)</th>
            <th className="px-4 py-2 text-left text-gray-400">Endpoint</th>
            <th className="px-4 py-2 text-right text-gray-400">Requests</th>
            <th className="px-4 py-2 text-right text-gray-400">Window</th>
          </tr>
        </thead>
        <tbody>
          {events.map((e, i) => (
            <tr key={i} className="border-t border-gray-700/50 hover:bg-gray-700/30">
              <td className="px-4 py-2 text-gray-400 font-mono text-xs">
                {new Date(e.timestamp).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-white font-mono">{e.ip_hash}</td>
              <td className="px-4 py-2 text-white font-mono">{e.endpoint}</td>
              <td className="px-4 py-2 text-right text-red-400">{e.requests_in_window}</td>
              <td className="px-4 py-2 text-right text-gray-300">{e.window_seconds}s</td>
            </tr>
          ))}
          {events.length === 0 && (
            <tr>
              <td colSpan={5} className="px-4 py-8 text-center text-gray-500">
                No rate limit events in the selected time range.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function IpActivityTab({ activity }: { activity: IpActivityRow[] }) {
  return (
    <div className="overflow-x-auto max-h-96">
      <table className="w-full text-sm">
        <thead className="bg-gray-900/50 sticky top-0">
          <tr>
            <th className="px-4 py-2 text-left text-gray-400">IP (hashed)</th>
            <th className="px-4 py-2 text-right text-gray-400">Requests</th>
            <th className="px-4 py-2 text-right text-gray-400">Errors</th>
            <th className="px-4 py-2 text-right text-gray-400">Rate Limits</th>
            <th className="px-4 py-2 text-right text-gray-400">Avg Latency</th>
          </tr>
        </thead>
        <tbody>
          {activity.map((a, i) => (
            <tr key={i} className="border-t border-gray-700/50 hover:bg-gray-700/30">
              <td className="px-4 py-2 text-white font-mono">{a.ip_hash}</td>
              <td className="px-4 py-2 text-right text-white">
                {Number(a.total_requests).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-red-400">
                {Number(a.error_count).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-yellow-400">
                {Number(a.rate_limit_count).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-right text-gray-300">
                {a.avg_duration_ms.toFixed(1)}ms
              </td>
            </tr>
          ))}
          {activity.length === 0 && (
            <tr>
              <td colSpan={5} className="px-4 py-8 text-center text-gray-500">
                No IP activity data available.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function getStatusColor(status: number): string {
  if (status < 200) return "bg-gray-700 text-gray-300";
  if (status < 300) return "bg-green-900 text-green-300";
  if (status < 400) return "bg-blue-900 text-blue-300";
  if (status < 500) return "bg-yellow-900 text-yellow-300";
  return "bg-red-900 text-red-300";
}

