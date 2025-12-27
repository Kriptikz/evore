"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { MetricCard } from "@/components/admin/MetricCard";
import { useAdmin } from "@/context/AdminContext";
import { api, AdminMetrics, RpcProviderRow } from "@/lib/api";

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  
  if (days > 0) {
    return `${days}d ${hours}h ${minutes}m`;
  }
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  return `${minutes}m`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) {
    return (n / 1_000_000).toFixed(1) + "M";
  }
  if (n >= 1_000) {
    return (n / 1_000).toFixed(1) + "K";
  }
  return n.toLocaleString();
}

export default function AdminDashboard() {
  const { isAuthenticated } = useAdmin();
  const [metrics, setMetrics] = useState<AdminMetrics | null>(null);
  const [providers, setProviders] = useState<RpcProviderRow[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) return;
    
    try {
      const [metricsData, providersData] = await Promise.all([
        api.getAdminMetrics(),
        api.getRpcProviders(24),
      ]);
      setMetrics(metricsData);
      setProviders(providersData.providers);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated]);

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    
    fetchData();
    // Refresh every 30 seconds
    const interval = setInterval(fetchData, 30000);
    return () => clearInterval(interval);
  }, [fetchData, isAuthenticated]);

  return (
    <AdminShell title="Dashboard" subtitle="Server overview and quick stats">
      {loading && !metrics ? (
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
        </div>
      ) : error ? (
        <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400">
          {error}
        </div>
      ) : metrics && (
        <div className="space-y-8">
          {/* Server Stats */}
          <section>
            <h2 className="text-lg font-semibold text-white mb-4">Server Status</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              <MetricCard
                title="Uptime"
                value={formatUptime(metrics.uptime_seconds)}
                color="green"
                icon={
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 12h.01M12 12h.01M19 12h.01M6 12a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0zm7 0a1 1 0 11-2 0 1 1 0 012 0z" />
                  </svg>
                }
              />
              <MetricCard
                title="Current Slot"
                value={formatNumber(metrics.current_slot)}
                color="blue"
                icon={
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                }
              />
              <MetricCard
                title="Miners Cached"
                value={formatNumber(metrics.miners_cached)}
                color="amber"
                icon={
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M17 20h5v-2a3 3 0 00-5.356-1.857M17 20H7m10 0v-2c0-.656-.126-1.283-.356-1.857M7 20H2v-2a3 3 0 015.356-1.857M7 20v-2c0-.656.126-1.283.356-1.857m0 0a5.002 5.002 0 019.288 0M15 7a3 3 0 11-6 0 3 3 0 016 0zm6 3a2 2 0 11-4 0 2 2 0 014 0zM7 10a2 2 0 11-4 0 2 2 0 014 0z" />
                  </svg>
                }
              />
              <MetricCard
                title="ORE Holders"
                value={formatNumber(metrics.ore_holders_cached)}
                color="slate"
                icon={
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8c-1.657 0-3 .895-3 2s1.343 2 3 2 3 .895 3 2-1.343 2-3 2m0-8c1.11 0 2.08.402 2.599 1M12 8V7m0 1v8m0 0v1m0-1c-1.11 0-2.08-.402-2.599-1M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                  </svg>
                }
              />
            </div>
          </section>

          {/* Round Status */}
          <section>
            <h2 className="text-lg font-semibold text-white mb-4">Round Status</h2>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <MetricCard
                title="Pending Round ID"
                value={metrics.pending_round_id > 0 ? metrics.pending_round_id : "None"}
                subtitle={metrics.pending_round_id > 0 ? "Awaiting finalization" : "No pending round"}
                color={metrics.pending_round_id > 0 ? "amber" : "slate"}
              />
              <MetricCard
                title="Pending Deployments"
                value={metrics.pending_deployments}
                subtitle="From WebSocket tracking"
                color={metrics.pending_deployments > 0 ? "blue" : "slate"}
              />
            </div>
          </section>

          {/* RPC Provider Overview */}
          <section>
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-lg font-semibold text-white">RPC Providers (24h)</h2>
              <a href="/admin/rpc" className="text-sm text-blue-400 hover:text-blue-300 transition-colors">
                View Details →
              </a>
            </div>
            
            {providers.length === 0 ? (
              <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-8 text-center">
                <p className="text-slate-400">No RPC data available yet</p>
              </div>
            ) : (
              <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-slate-700">
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Provider</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Program</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Requests</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Success</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Errors</th>
                      <th className="text-right px-4 py-3 text-sm font-medium text-slate-400">Avg Latency</th>
                    </tr>
                  </thead>
                  <tbody>
                    {providers.map((p, i) => {
                      const successRate = p.total_requests > 0 
                        ? ((p.success_count / p.total_requests) * 100).toFixed(1)
                        : "0";
                      return (
                        <tr key={i} className="border-b border-slate-700/50 last:border-0">
                          <td className="px-4 py-3 text-white font-medium">{p.provider}</td>
                          <td className="px-4 py-3 text-slate-400">{p.program}</td>
                          <td className="px-4 py-3 text-right text-white">{formatNumber(p.total_requests)}</td>
                          <td className="px-4 py-3 text-right">
                            <span className={Number(successRate) >= 99 ? "text-green-400" : Number(successRate) >= 95 ? "text-amber-400" : "text-red-400"}>
                              {successRate}%
                            </span>
                          </td>
                          <td className="px-4 py-3 text-right text-red-400">{p.error_count}</td>
                          <td className="px-4 py-3 text-right text-slate-300">{p.avg_duration_ms.toFixed(0)}ms</td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </section>

          {/* Quick Actions */}
          <section>
            <h2 className="text-lg font-semibold text-white mb-4">Quick Actions</h2>
            <div className="flex gap-4">
              <button
                onClick={async () => {
                  try {
                    const result = await api.cleanupSessions();
                    alert(result.message);
                  } catch (err) {
                    alert(err instanceof Error ? err.message : "Failed to cleanup sessions");
                  }
                }}
                className="px-4 py-2 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors text-sm"
              >
                Cleanup Expired Sessions
              </button>
              <button
                onClick={fetchData}
                className="px-4 py-2 bg-blue-500/20 hover:bg-blue-500/30 text-blue-400 rounded-lg transition-colors text-sm"
              >
                Refresh Data
              </button>
            </div>
          </section>
        </div>
      )}
    </AdminShell>
  );
}

