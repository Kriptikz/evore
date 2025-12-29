"use client";

import { useState, useEffect, useCallback, Suspense } from "react";
import { useSearchParams } from "next/navigation";
import { 
  api,
  AutomationQueueStats,
  AutomationFetchStats,
  AutomationQueueItem,
  AutomationLiveStats,
  AutomationProcessResult,
} from "@/lib/api";
import { AdminShell } from "@/components/admin/AdminShell";

// ============================================================================
// Components
// ============================================================================

function StatCard({ title, value, subtitle, color = "blue" }: {
  title: string;
  value: string | number;
  subtitle?: string;
  color?: "blue" | "green" | "amber" | "red" | "purple";
}) {
  const colors = {
    blue: "text-blue-400",
    green: "text-green-400",
    amber: "text-amber-400",
    red: "text-red-400",
    purple: "text-purple-400",
  };
  
  return (
    <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
      <div className="text-xs text-slate-500 uppercase tracking-wide mb-1">{title}</div>
      <div className={`text-2xl font-bold ${colors[color]}`}>{value}</div>
      {subtitle && <div className="text-xs text-slate-500 mt-1">{subtitle}</div>}
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    pending: "bg-slate-500/20 text-slate-400",
    processing: "bg-blue-500/20 text-blue-400 animate-pulse",
    completed: "bg-green-500/20 text-green-400",
    failed: "bg-red-500/20 text-red-400",
  };
  
  return (
    <span className={`px-2 py-1 rounded text-xs font-medium ${styles[status] || styles.pending}`}>
      {status}
    </span>
  );
}

function Pubkey({ address, short = true }: { address: string; short?: boolean }) {
  const display = short ? `${address.slice(0, 4)}...${address.slice(-4)}` : address;
  return (
    <span 
      className="font-mono text-slate-300 hover:text-white cursor-pointer"
      onClick={() => navigator.clipboard.writeText(address)}
      title={address}
    >
      {display}
    </span>
  );
}

function formatDuration(ms: number | null): string {
  if (ms === null) return "-";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

function formatTime(iso: string | null): string {
  if (!iso) return "-";
  return new Date(iso).toLocaleString();
}

// ============================================================================
// Main Content
// ============================================================================

function AutomationPageContent() {
  const searchParams = useSearchParams();
  
  const [queueStats, setQueueStats] = useState<AutomationQueueStats | null>(null);
  const [fetchStats, setFetchStats] = useState<AutomationFetchStats | null>(null);
  const [liveStats, setLiveStats] = useState<AutomationLiveStats | null>(null);
  const [queueItems, setQueueItems] = useState<AutomationQueueItem[]>([]);
  const [totalItems, setTotalItems] = useState(0);
  const [loading, setLoading] = useState(true);
  const [processing, setProcessing] = useState(false);
  const [processResults, setProcessResults] = useState<AutomationProcessResult | null>(null);
  
  // Filters
  const [statusFilter, setStatusFilter] = useState<string>(searchParams.get("status") || "");
  const [roundFilter, setRoundFilter] = useState<string>(searchParams.get("round_id") || "");
  const [page, setPage] = useState(1);
  const limit = 20;
  
  // Tabs
  const [tab, setTab] = useState<"queue" | "stats" | "live">("queue");
  
  const fetchData = useCallback(async () => {
    try {
      // Fetch stats
      const [statsRes, fetchStatsRes, liveStatsRes] = await Promise.all([
        api.getAutomationQueueStats(),
        api.getAutomationFetchStats(),
        api.getAutomationLiveStats(),
      ]);
      
      setQueueStats(statsRes);
      setFetchStats(fetchStatsRes);
      setLiveStats(liveStatsRes);
      
      // Fetch queue items
      const queueRes = await api.getAutomationQueue({
        status: statusFilter || undefined,
        round_id: roundFilter ? parseInt(roundFilter) : undefined,
        page,
        limit,
      });
      setQueueItems(queueRes.items);
      setTotalItems(queueRes.total);
    } catch (err) {
      console.error("Failed to fetch automation data:", err);
    } finally {
      setLoading(false);
    }
  }, [statusFilter, roundFilter, page]);
  
  useEffect(() => {
    fetchData();
    const interval = setInterval(fetchData, 5000); // Auto-refresh every 5s
    return () => clearInterval(interval);
  }, [fetchData]);
  
  const handleProcess = async (count: number) => {
    setProcessing(true);
    setProcessResults(null);
    try {
      const result = await api.processAutomationQueue(count);
      setProcessResults(result);
      fetchData();
    } catch (err) {
      console.error("Failed to process queue:", err);
    } finally {
      setProcessing(false);
    }
  };
  
  const handleRetryFailed = async () => {
    try {
      await api.retryFailedAutomation();
      fetchData();
    } catch (err) {
      console.error("Failed to retry:", err);
    }
  };
  
  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
      </div>
    );
  }
  
  return (
    <div className="space-y-6">
      {/* Actions */}
      <div className="flex items-center justify-end">
        <div className="flex gap-2">
          <button
            onClick={() => handleProcess(5)}
            disabled={processing || (queueStats?.pending || 0) === 0}
            className="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors"
          >
            {processing ? "Processing..." : "Process 5"}
          </button>
          <button
            onClick={() => handleProcess(20)}
            disabled={processing || (queueStats?.pending || 0) === 0}
            className="px-4 py-2 rounded-lg bg-blue-600 hover:bg-blue-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors"
          >
            {processing ? "..." : "Process 20"}
          </button>
          <button
            onClick={handleRetryFailed}
            disabled={(queueStats?.failed || 0) === 0}
            className="px-4 py-2 rounded-lg bg-amber-600 hover:bg-amber-500 disabled:opacity-50 disabled:cursor-not-allowed text-white text-sm font-medium transition-colors"
          >
            Retry Failed
          </button>
        </div>
      </div>
      
      {/* Stats Cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-4">
        <StatCard title="Pending" value={queueStats?.pending || 0} color="blue" />
        <StatCard title="Processing" value={queueStats?.processing || 0} color="purple" />
        <StatCard title="Completed" value={queueStats?.completed || 0} color="green" />
        <StatCard title="Failed" value={queueStats?.failed || 0} color="red" />
        <StatCard 
          title="Avg Duration" 
          value={formatDuration(queueStats?.avg_fetch_duration_ms || null)} 
          color="amber" 
        />
        <StatCard 
          title="Avg Txns Searched" 
          value={queueStats?.avg_txns_searched?.toFixed(1) || "-"} 
          color="amber" 
        />
      </div>
      
      {/* Process Results */}
      {processResults && (
        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
          <div className="flex items-center justify-between mb-3">
            <h3 className="font-semibold text-white">Process Results</h3>
            <button
              onClick={() => setProcessResults(null)}
              className="text-slate-500 hover:text-white"
            >
              ✕
            </button>
          </div>
          <div className="flex gap-4 mb-3 text-sm">
            <span className="text-slate-400">Processed: {processResults.processed}</span>
            <span className="text-green-400">Success: {processResults.success}</span>
            <span className="text-red-400">Failed: {processResults.failed}</span>
          </div>
          <div className="space-y-2 max-h-48 overflow-auto">
            {processResults.details.map((d) => (
              <div 
                key={d.id}
                className={`flex items-center justify-between p-2 rounded text-sm ${
                  d.success ? "bg-green-500/10" : "bg-red-500/10"
                }`}
              >
                <div className="flex items-center gap-3">
                  <span className={d.success ? "text-green-400" : "text-red-400"}>
                    {d.success ? "✓" : "✗"}
                  </span>
                  <Pubkey address={d.deploy_signature} />
                  <span className="text-slate-500">
                    {d.automation_active ? "Active" : "Inactive"}
                  </span>
                  {d.used_cache && (
                    <span className="px-1.5 py-0.5 rounded bg-cyan-500/20 text-cyan-400 text-xs">
                      Cached
                    </span>
                  )}
                </div>
                <div className="text-slate-400 text-xs">
                  {d.used_cache && d.cache_slot ? `from slot ${d.cache_slot} · ` : ""}
                  {d.txns_searched} txns / {formatDuration(d.duration_ms)}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
      
      {/* Tabs */}
      <div className="flex gap-2 border-b border-slate-700">
        {(["queue", "stats", "live"] as const).map((t) => (
          <button
            key={t}
            onClick={() => setTab(t)}
            className={`px-4 py-2 text-sm font-medium transition-colors ${
              tab === t
                ? "text-blue-400 border-b-2 border-blue-400"
                : "text-slate-400 hover:text-white"
            }`}
          >
            {t.charAt(0).toUpperCase() + t.slice(1)}
          </button>
        ))}
      </div>
      
      {/* Queue Tab */}
      {tab === "queue" && (
        <div className="space-y-4">
          {/* Filters */}
          <div className="flex gap-4">
            <select
              value={statusFilter}
              onChange={(e) => { setStatusFilter(e.target.value); setPage(1); }}
              className="px-3 py-2 rounded-lg bg-slate-800 border border-slate-700 text-white text-sm"
            >
              <option value="">All Status</option>
              <option value="pending">Pending</option>
              <option value="processing">Processing</option>
              <option value="completed">Completed</option>
              <option value="failed">Failed</option>
            </select>
            <input
              type="text"
              value={roundFilter}
              onChange={(e) => { setRoundFilter(e.target.value); setPage(1); }}
              placeholder="Round ID"
              className="px-3 py-2 rounded-lg bg-slate-800 border border-slate-700 text-white text-sm w-32"
            />
          </div>
          
          {/* Queue Table */}
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
            <table className="w-full text-sm">
              <thead className="bg-slate-900/50">
                <tr className="text-left text-slate-400">
                  <th className="px-4 py-3">ID</th>
                  <th className="px-4 py-3">Round</th>
                  <th className="px-4 py-3">Authority</th>
                  <th className="px-4 py-3">Deploy Sig</th>
                  <th className="px-4 py-3">Status</th>
                  <th className="px-4 py-3">Attempts</th>
                  <th className="px-4 py-3">Duration</th>
                  <th className="px-4 py-3">Created</th>
                </tr>
              </thead>
              <tbody>
                {queueItems.map((item) => (
                  <tr key={item.id} className="border-t border-slate-700/50 hover:bg-slate-700/20">
                    <td className="px-4 py-3 text-slate-300">{item.id}</td>
                    <td className="px-4 py-3 text-blue-400">{item.round_id}</td>
                    <td className="px-4 py-3"><Pubkey address={item.authority_pubkey} /></td>
                    <td className="px-4 py-3"><Pubkey address={item.deploy_signature} /></td>
                    <td className="px-4 py-3"><StatusBadge status={item.status} /></td>
                    <td className="px-4 py-3 text-slate-400">{item.attempts}</td>
                    <td className="px-4 py-3 text-slate-400">
                      {formatDuration(item.fetch_duration_ms)}
                    </td>
                    <td className="px-4 py-3 text-slate-500 text-xs">
                      {formatTime(item.created_at)}
                    </td>
                  </tr>
                ))}
                {queueItems.length === 0 && (
                  <tr>
                    <td colSpan={8} className="px-4 py-8 text-center text-slate-500">
                      No items in queue
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </div>
          
          {/* Pagination */}
          {totalItems > limit && (
            <div className="flex items-center justify-between">
              <span className="text-sm text-slate-400">
                Showing {((page - 1) * limit) + 1}-{Math.min(page * limit, totalItems)} of {totalItems}
              </span>
              <div className="flex gap-2">
                <button
                  onClick={() => setPage((p) => Math.max(1, p - 1))}
                  disabled={page === 1}
                  className="px-3 py-1 rounded bg-slate-700 hover:bg-slate-600 disabled:opacity-50 text-white text-sm"
                >
                  Prev
                </button>
                <button
                  onClick={() => setPage((p) => p + 1)}
                  disabled={page * limit >= totalItems}
                  className="px-3 py-1 rounded bg-slate-700 hover:bg-slate-600 disabled:opacity-50 text-white text-sm"
                >
                  Next
                </button>
              </div>
            </div>
          )}
        </div>
      )}
      
      {/* Stats Tab */}
      {tab === "stats" && fetchStats && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <StatCard title="Total Fetched" value={fetchStats.total_fetched} color="blue" />
          <StatCard 
            title="Found Rate" 
            value={`${((fetchStats.found_count / fetchStats.total_fetched) * 100 || 0).toFixed(1)}%`}
            subtitle={`${fetchStats.found_count} found`}
            color="green"
          />
          <StatCard 
            title="Active Rate" 
            value={`${((fetchStats.active_count / fetchStats.total_fetched) * 100 || 0).toFixed(1)}%`}
            subtitle={`${fetchStats.active_count} active`}
            color="amber"
          />
          <StatCard 
            title="Avg Txns Searched" 
            value={fetchStats.avg_txns_searched.toFixed(1)}
            subtitle={`Max: ${fetchStats.max_txns_searched}`}
            color="purple"
          />
          <StatCard 
            title="Avg Duration" 
            value={formatDuration(fetchStats.avg_duration_ms)}
            subtitle={`Max: ${formatDuration(fetchStats.max_duration_ms)}`}
            color="purple"
          />
          <StatCard 
            title="Partial Deploys" 
            value={fetchStats.partial_deploy_count}
            subtitle="Balance ran out early"
            color="red"
          />
          <StatCard 
            title="SOL Tracked" 
            value={(fetchStats.total_sol_tracked / 1e9).toFixed(4)}
            subtitle="Total SOL in deployments"
            color="green"
          />
        </div>
      )}
      
      {/* Live Tab */}
      {tab === "live" && (
        <div className="space-y-4">
          {liveStats ? (
            <>
              <div className="flex items-center gap-3">
                <div className={`w-3 h-3 rounded-full ${liveStats.is_running ? "bg-green-500 animate-pulse" : "bg-slate-500"}`} />
                <span className="text-white font-medium">
                  {liveStats.is_running ? "Background Task Running" : "Background Task Idle"}
                </span>
              </div>
              
              {liveStats.is_running && (
                <div className="bg-slate-800/50 rounded-xl p-4 border border-blue-500/30">
                  <h3 className="font-semibold text-blue-400 mb-3">Currently Processing</h3>
                  <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-sm">
                    <div>
                      <span className="text-slate-500">Item ID:</span>{" "}
                      <span className="text-white">{liveStats.current_item_id}</span>
                    </div>
                    <div>
                      <span className="text-slate-500">Authority:</span>{" "}
                      {liveStats.current_authority && <Pubkey address={liveStats.current_authority} />}
                    </div>
                    <div>
                      <span className="text-slate-500">Txns Searched:</span>{" "}
                      <span className="text-amber-400">{liveStats.txns_searched_so_far}</span>
                    </div>
                    <div>
                      <span className="text-slate-500">Elapsed:</span>{" "}
                      <span className="text-purple-400">{formatDuration(liveStats.elapsed_ms)}</span>
                    </div>
                  </div>
                </div>
              )}
              
              <div className="grid grid-cols-3 gap-4">
                <StatCard 
                  title="Processed (Session)" 
                  value={liveStats.items_processed_this_session} 
                  color="blue" 
                />
                <StatCard 
                  title="Succeeded (Session)" 
                  value={liveStats.items_succeeded_this_session} 
                  color="green" 
                />
                <StatCard 
                  title="Failed (Session)" 
                  value={liveStats.items_failed_this_session} 
                  color="red" 
                />
              </div>
              
              <div className="text-xs text-slate-500">
                Last updated: {formatTime(liveStats.last_updated)}
              </div>
            </>
          ) : (
            <div className="text-slate-400">
              Live stats not available (background task may not be running)
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Page Export
// ============================================================================

export default function AutomationPage() {
  return (
    <AdminShell title="Automation State Reconstruction" subtitle="Track automation state at time of each deployment">
      <Suspense fallback={
        <div className="flex items-center justify-center h-64">
          <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-blue-500"></div>
        </div>
      }>
        <AutomationPageContent />
      </Suspense>
    </AdminShell>
  );
}

