"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { api, RequestLogRow, EndpointSummaryRow, RateLimitEventRow, IpActivityRow } from "@/lib/api";
import { useAdmin } from "@/context/AdminContext";

type TabType = "logs" | "endpoints" | "rate-limits" | "ips";

type StatusFilter = "all" | "success" | "redirect" | "client_error" | "server_error" | "errors";

export default function RequestLogsPage() {
  const { isAuthenticated } = useAdmin();
  const [activeTab, setActiveTab] = useState<TabType>("endpoints");
  const [logs, setLogs] = useState<RequestLogRow[]>([]);
  const [endpoints, setEndpoints] = useState<EndpointSummaryRow[]>([]);
  const [rateLimits, setRateLimits] = useState<RateLimitEventRow[]>([]);
  const [ipActivity, setIpActivity] = useState<IpActivityRow[]>([]);
  const [hours, setHours] = useState(24);
  const [limit, setLimit] = useState(500);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  
  // Filters for logs
  const [selectedIp, setSelectedIp] = useState<string | null>(null);
  const [endpointFilter, setEndpointFilter] = useState<string>("");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [ipLogsLoading, setIpLogsLoading] = useState(false);

  // Convert status filter to API params
  const getStatusParams = (filter: StatusFilter): { statusGte?: number; statusLte?: number } => {
    switch (filter) {
      case "success": return { statusGte: 200, statusLte: 299 };
      case "redirect": return { statusGte: 300, statusLte: 399 };
      case "client_error": return { statusGte: 400, statusLte: 499 };
      case "server_error": return { statusGte: 500, statusLte: 599 };
      case "errors": return { statusGte: 400 };
      default: return {};
    }
  };

  const fetchData = useCallback(async (ipFilter?: string | null) => {
    if (!isAuthenticated) return;
    
    // Use the provided ipFilter, or fall back to current selectedIp state
    const ipToFilter = ipFilter !== undefined ? ipFilter : selectedIp;
    const statusParams = getStatusParams(statusFilter);
    
    try {
      setLoading(true);
      const [logsRes, endpointsRes, rateLimitsRes, ipRes] = await Promise.all([
        api.getRequestLogs({
          hours,
          limit,
          ipHash: ipToFilter || undefined,
          endpoint: endpointFilter || undefined,
          ...statusParams,
        }),
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
  }, [isAuthenticated, hours, limit, selectedIp, endpointFilter, statusFilter]);

  // Fetch logs for a specific IP
  const fetchLogsForIp = useCallback(async (ipHash: string) => {
    if (!isAuthenticated) return;
    
    setSelectedIp(ipHash);
    setActiveTab("logs");
    setIpLogsLoading(true);
    
    try {
      const statusParams = getStatusParams(statusFilter);
      const res = await api.getRequestLogs({
        hours,
        limit,
        ipHash,
        endpoint: endpointFilter || undefined,
        ...statusParams,
      });
      setLogs(res.logs);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to fetch logs for IP");
    } finally {
      setIpLogsLoading(false);
    }
  }, [isAuthenticated, hours, limit, endpointFilter, statusFilter]);

  // Clear IP filter and reload all logs
  const clearIpFilter = useCallback(async () => {
    setSelectedIp(null);
    setIpLogsLoading(true);
    try {
      const statusParams = getStatusParams(statusFilter);
      const res = await api.getRequestLogs({
        hours,
        limit,
        endpoint: endpointFilter || undefined,
        ...statusParams,
      });
      setLogs(res.logs);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to fetch logs");
    } finally {
      setIpLogsLoading(false);
    }
  }, [hours, limit, endpointFilter, statusFilter]);

  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData]);

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
    <AdminShell title="Request Logs" subtitle="Server request analytics and monitoring">
      <div className="space-y-6">
        {/* Controls Row */}
        <div className="flex flex-col gap-4">
          <div className="flex flex-wrap justify-between items-center gap-4">
            <div className="flex flex-wrap items-center gap-3">
              {/* Time Range */}
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
              
              {/* Limit */}
              <select
                value={limit}
                onChange={(e) => setLimit(parseInt(e.target.value))}
                className="bg-slate-800 text-white px-3 py-2 rounded-lg border border-slate-700 focus:ring-blue-500 focus:border-blue-500"
              >
                <option value="100">100 logs</option>
                <option value="250">250 logs</option>
                <option value="500">500 logs</option>
                <option value="1000">1,000 logs</option>
                <option value="2000">2,000 logs</option>
              </select>
              
              {/* Status Filter */}
              <select
                value={statusFilter}
                onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
                className="bg-slate-800 text-white px-3 py-2 rounded-lg border border-slate-700 focus:ring-blue-500 focus:border-blue-500"
              >
                <option value="all">All Statuses</option>
                <option value="success">2xx Success</option>
                <option value="redirect">3xx Redirect</option>
                <option value="client_error">4xx Client Error</option>
                <option value="server_error">5xx Server Error</option>
                <option value="errors">All Errors (4xx+5xx)</option>
              </select>
              
              {/* Endpoint Filter */}
              <div className="relative">
                <input
                  type="text"
                  placeholder="Filter endpoint..."
                  value={endpointFilter}
                  onChange={(e) => setEndpointFilter(e.target.value)}
                  className="bg-slate-800 text-white px-3 py-2 rounded-lg border border-slate-700 focus:ring-blue-500 focus:border-blue-500 w-48"
                />
                {endpointFilter && (
                  <button
                    onClick={() => setEndpointFilter("")}
                    className="absolute right-2 top-1/2 -translate-y-1/2 text-slate-400 hover:text-white"
                  >
                    ✕
                  </button>
                )}
              </div>
            </div>
            
            <button
              onClick={() => fetchData()}
              disabled={loading}
              className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors disabled:opacity-50"
            >
              {loading ? "Refreshing..." : "Refresh"}
            </button>
          </div>
          
          {/* Active Filters Display */}
          {(selectedIp || endpointFilter || statusFilter !== "all") && (
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm text-slate-400">Active filters:</span>
              {selectedIp && (
                <div className="flex items-center gap-1 bg-blue-500/20 border border-blue-500/40 text-blue-400 px-2 py-1 rounded text-sm">
                  <span>IP: {selectedIp}</span>
                  <button
                    onClick={clearIpFilter}
                    className="ml-1 text-blue-300 hover:text-white transition-colors"
                  >
                    ✕
                  </button>
                </div>
              )}
              {endpointFilter && (
                <div className="flex items-center gap-1 bg-purple-500/20 border border-purple-500/40 text-purple-400 px-2 py-1 rounded text-sm">
                  <span>Endpoint: {endpointFilter}</span>
                  <button
                    onClick={() => setEndpointFilter("")}
                    className="ml-1 text-purple-300 hover:text-white transition-colors"
                  >
                    ✕
                  </button>
                </div>
              )}
              {statusFilter !== "all" && (
                <div className="flex items-center gap-1 bg-yellow-500/20 border border-yellow-500/40 text-yellow-400 px-2 py-1 rounded text-sm">
                  <span>Status: {statusFilter.replace("_", " ")}</span>
                  <button
                    onClick={() => setStatusFilter("all")}
                    className="ml-1 text-yellow-300 hover:text-white transition-colors"
                  >
                    ✕
                  </button>
                </div>
              )}
              <button
                onClick={() => {
                  setSelectedIp(null);
                  setEndpointFilter("");
                  setStatusFilter("all");
                }}
                className="text-sm text-slate-400 hover:text-white transition-colors ml-2"
              >
                Clear all
              </button>
            </div>
          )}
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
            <div className="text-slate-400 text-sm mb-1">Error Requests</div>
            <div className="text-2xl font-bold text-red-400">{totalErrors.toLocaleString()}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Rate Limit Events</div>
            <div className="text-2xl font-bold text-yellow-400">{rateLimits.length}</div>
          </div>
          <div className="bg-slate-800/50 border border-slate-700 p-4 rounded-lg">
            <div className="text-slate-400 text-sm mb-1">Avg Latency</div>
            <div className="text-2xl font-bold text-blue-400">{avgLatency.toFixed(1)}ms</div>
          </div>
        </div>

        {/* Tabs */}
        <div className="border-b border-slate-700">
          <div className="flex gap-1">
            {tabs.map((tab) => (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`px-4 py-3 font-medium transition-colors rounded-t-lg ${
                  activeTab === tab.id
                    ? "text-white bg-slate-800 border-t border-l border-r border-slate-700"
                    : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/50"
                }`}
              >
                {tab.label}
                <span className="ml-2 text-xs bg-slate-700 px-2 py-0.5 rounded">
                  {tab.count}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Tab Content */}
        <div className="bg-slate-800/50 border border-slate-700 rounded-lg overflow-hidden">
          {activeTab === "endpoints" && <EndpointsTab endpoints={endpoints} />}
          {activeTab === "logs" && (
            <LogsTab 
              logs={logs} 
              loading={ipLogsLoading} 
              selectedIp={selectedIp}
              onClearFilter={clearIpFilter}
            />
          )}
          {activeTab === "rate-limits" && <RateLimitsTab events={rateLimits} />}
          {activeTab === "ips" && (
            <IpActivityTab 
              activity={ipActivity} 
              onIpClick={fetchLogsForIp}
            />
          )}
        </div>
      </div>
    </AdminShell>
  );
}

function EndpointsTab({ endpoints }: { endpoints: EndpointSummaryRow[] }) {
  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead className="bg-slate-900/50">
          <tr>
            <th className="px-4 py-3 text-left text-slate-400 font-medium">Endpoint</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Requests</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Success</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Errors</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Avg (ms)</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">P95 (ms)</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Max (ms)</th>
          </tr>
        </thead>
        <tbody>
          {endpoints.map((e, i) => (
            <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
              <td className="px-4 py-3 text-white font-mono">{e.endpoint}</td>
              <td className="px-4 py-3 text-right text-white">
                {Number(e.total_requests).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-green-400">
                {Number(e.success_count).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-red-400">
                {Number(e.error_count).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-slate-300">
                {e.avg_duration_ms.toFixed(1)}
              </td>
              <td className="px-4 py-3 text-right text-yellow-400">
                {e.p95_duration_ms.toFixed(1)}
              </td>
              <td className="px-4 py-3 text-right text-orange-400">
                {e.max_duration_ms}
              </td>
            </tr>
          ))}
          {endpoints.length === 0 && (
            <tr>
              <td colSpan={7} className="px-4 py-8 text-center text-slate-500">
                No endpoint data available.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

type LogSortColumn = "timestamp" | "method" | "endpoint" | "status_code" | "duration_ms" | "ip_hash";
type SortDirection = "asc" | "desc";

function LogsTab({ 
  logs, 
  loading, 
  selectedIp,
  onClearFilter 
}: { 
  logs: RequestLogRow[]; 
  loading: boolean;
  selectedIp: string | null;
  onClearFilter: () => void;
}) {
  const [sortColumn, setSortColumn] = useState<LogSortColumn>("timestamp");
  const [sortDirection, setSortDirection] = useState<SortDirection>("desc");

  const handleSort = (column: LogSortColumn) => {
    if (sortColumn === column) {
      // Toggle direction if same column
      setSortDirection(sortDirection === "asc" ? "desc" : "asc");
    } else {
      // New column, default to desc for duration/timestamp, asc for others
      setSortColumn(column);
      setSortDirection(column === "duration_ms" || column === "timestamp" ? "desc" : "asc");
    }
  };

  const sortedLogs = [...logs].sort((a, b) => {
    let aVal: number | string;
    let bVal: number | string;

    switch (sortColumn) {
      case "timestamp":
        aVal = a.timestamp;
        bVal = b.timestamp;
        break;
      case "duration_ms":
        aVal = a.duration_ms;
        bVal = b.duration_ms;
        break;
      case "status_code":
        aVal = a.status_code;
        bVal = b.status_code;
        break;
      case "method":
        aVal = a.method;
        bVal = b.method;
        break;
      case "endpoint":
        aVal = a.endpoint;
        bVal = b.endpoint;
        break;
      case "ip_hash":
        aVal = a.ip_hash;
        bVal = b.ip_hash;
        break;
      default:
        return 0;
    }

    if (typeof aVal === "string" && typeof bVal === "string") {
      return sortDirection === "asc" 
        ? aVal.localeCompare(bVal) 
        : bVal.localeCompare(aVal);
    }

    return sortDirection === "asc" 
      ? (aVal as number) - (bVal as number) 
      : (bVal as number) - (aVal as number);
  });

  const SortIcon = ({ column }: { column: LogSortColumn }) => {
    if (sortColumn !== column) {
      return <span className="ml-1 text-slate-600">↕</span>;
    }
    return <span className="ml-1 text-blue-400">{sortDirection === "asc" ? "↑" : "↓"}</span>;
  };

  const SortableHeader = ({ 
    column, 
    children, 
    align = "left" 
  }: { 
    column: LogSortColumn; 
    children: React.ReactNode; 
    align?: "left" | "center" | "right";
  }) => (
    <th 
      className={`px-4 py-3 text-${align} text-slate-400 font-medium cursor-pointer hover:text-white hover:bg-slate-800/50 transition-colors select-none`}
      onClick={() => handleSort(column)}
    >
      <span className="inline-flex items-center">
        {children}
        <SortIcon column={column} />
      </span>
    </th>
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <div className="overflow-x-auto max-h-[500px]">
      {selectedIp && (
        <div className="px-4 py-3 bg-blue-500/10 border-b border-blue-500/30 flex items-center justify-between">
          <span className="text-blue-400 text-sm">
            Showing logs for IP: <code className="font-mono bg-blue-500/20 px-2 py-0.5 rounded">{selectedIp}</code>
          </span>
          <button
            onClick={onClearFilter}
            className="text-sm text-blue-400 hover:text-blue-300 transition-colors"
          >
            Show all logs
          </button>
        </div>
      )}
      <table className="w-full text-sm">
        <thead className="bg-slate-900/50 sticky top-0">
          <tr>
            <SortableHeader column="timestamp">Time</SortableHeader>
            <SortableHeader column="method">Method</SortableHeader>
            <SortableHeader column="endpoint">Endpoint</SortableHeader>
            <SortableHeader column="status_code" align="center">Status</SortableHeader>
            <SortableHeader column="duration_ms" align="right">Duration</SortableHeader>
            <SortableHeader column="ip_hash">IP</SortableHeader>
          </tr>
        </thead>
        <tbody>
          {sortedLogs.map((log, i) => (
            <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
              <td className="px-4 py-2 text-slate-400 font-mono text-xs">
                {new Date(log.timestamp).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-white">{log.method}</td>
              <td className="px-4 py-2 text-white font-mono text-xs">{log.endpoint}</td>
              <td className="px-4 py-2 text-center">
                <span className={`px-2 py-0.5 rounded text-xs font-medium ${getStatusColor(log.status_code)}`}>
                  {log.status_code}
                </span>
              </td>
              <td className={`px-4 py-2 text-right font-mono ${getDurationColor(log.duration_ms)}`}>
                {log.duration_ms}ms
              </td>
              <td className="px-4 py-2 text-slate-400 font-mono text-xs">{log.ip_hash}</td>
            </tr>
          ))}
          {sortedLogs.length === 0 && (
            <tr>
              <td colSpan={6} className="px-4 py-8 text-center text-slate-500">
                No request logs available.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function getDurationColor(ms: number): string {
  if (ms < 50) return "text-green-400";
  if (ms < 200) return "text-slate-300";
  if (ms < 500) return "text-yellow-400";
  if (ms < 1000) return "text-orange-400";
  return "text-red-400";
}

function RateLimitsTab({ events }: { events: RateLimitEventRow[] }) {
  return (
    <div className="overflow-x-auto max-h-[500px]">
      <table className="w-full text-sm">
        <thead className="bg-slate-900/50 sticky top-0">
          <tr>
            <th className="px-4 py-3 text-left text-slate-400 font-medium">Time</th>
            <th className="px-4 py-3 text-left text-slate-400 font-medium">IP</th>
            <th className="px-4 py-3 text-left text-slate-400 font-medium">Endpoint</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Requests</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Window</th>
          </tr>
        </thead>
        <tbody>
          {events.map((e, i) => (
            <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
              <td className="px-4 py-2 text-slate-400 font-mono text-xs">
                {new Date(e.timestamp).toLocaleString()}
              </td>
              <td className="px-4 py-2 text-white font-mono">{e.ip_hash}</td>
              <td className="px-4 py-2 text-white font-mono">{e.endpoint}</td>
              <td className="px-4 py-2 text-right text-red-400">{e.requests_in_window}</td>
              <td className="px-4 py-2 text-right text-slate-300">{e.window_seconds}s</td>
            </tr>
          ))}
          {events.length === 0 && (
            <tr>
              <td colSpan={5} className="px-4 py-8 text-center text-slate-500">
                No rate limit events in the selected time range.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}

function IpActivityTab({ 
  activity,
  onIpClick 
}: { 
  activity: IpActivityRow[];
  onIpClick: (ipHash: string) => void;
}) {
  return (
    <div className="overflow-x-auto max-h-[500px]">
      <table className="w-full text-sm">
        <thead className="bg-slate-900/50 sticky top-0">
          <tr>
            <th className="px-4 py-3 text-left text-slate-400 font-medium">IP (hashed)</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Requests</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Errors</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Rate Limits</th>
            <th className="px-4 py-3 text-right text-slate-400 font-medium">Avg Latency</th>
            <th className="px-4 py-3 text-center text-slate-400 font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {activity.map((a, i) => (
            <tr key={i} className="border-t border-slate-700/50 hover:bg-slate-700/30">
              <td className="px-4 py-3 text-white font-mono">{a.ip_hash}</td>
              <td className="px-4 py-3 text-right text-white">
                {Number(a.total_requests).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-red-400">
                {Number(a.error_count).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-yellow-400">
                {Number(a.rate_limit_count).toLocaleString()}
              </td>
              <td className="px-4 py-3 text-right text-slate-300">
                {a.avg_duration_ms.toFixed(1)}ms
              </td>
              <td className="px-4 py-3 text-center">
                <button
                  onClick={() => onIpClick(a.ip_hash)}
                  className="px-3 py-1 text-xs bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 rounded-lg transition-colors"
                >
                  View Logs
                </button>
              </td>
            </tr>
          ))}
          {activity.length === 0 && (
            <tr>
              <td colSpan={6} className="px-4 py-8 text-center text-slate-500">
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
  if (status < 200) return "bg-slate-700 text-slate-300";
  if (status < 300) return "bg-green-500/20 text-green-400";
  if (status < 400) return "bg-blue-500/20 text-blue-400";
  if (status < 500) return "bg-yellow-500/20 text-yellow-400";
  return "bg-red-500/20 text-red-400";
}
