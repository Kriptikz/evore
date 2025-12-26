"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { api, BlacklistEntry } from "@/lib/api";

export default function BlacklistPage() {
  const [entries, setEntries] = useState<BlacklistEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newIp, setNewIp] = useState("");
  const [newReason, setNewReason] = useState("");
  const [isPermanent, setIsPermanent] = useState(false);
  const [adding, setAdding] = useState(false);

  const fetchData = useCallback(async () => {
    try {
      setLoading(true);
      const data = await api.getBlacklist();
      setEntries(data.entries);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const handleAdd = async (e: React.FormEvent) => {
    e.preventDefault();
    setAdding(true);
    try {
      await api.addToBlacklist(newIp, newReason, isPermanent);
      setNewIp("");
      setNewReason("");
      setIsPermanent(false);
      setShowAddForm(false);
      await fetchData();
    } catch (err) {
      alert(err instanceof Error ? err.message : "Failed to add to blacklist");
    } finally {
      setAdding(false);
    }
  };

  const handleRemove = async (ip: string) => {
    if (!confirm(`Remove ${ip} from blacklist?`)) return;
    
    try {
      await api.removeFromBlacklist(ip);
      await fetchData();
    } catch (err) {
      alert(err instanceof Error ? err.message : "Failed to remove from blacklist");
    }
  };

  return (
    <AdminShell title="IP Blacklist" subtitle="Manage blocked IP addresses">
      <div className="space-y-6">
        {/* Header */}
        <div className="flex items-center justify-between">
          <p className="text-slate-400 text-sm">
            {entries.length} IP{entries.length !== 1 ? "s" : ""} currently blacklisted
          </p>
          <button
            onClick={() => setShowAddForm(!showAddForm)}
            className="px-4 py-2 bg-blue-500 hover:bg-blue-600 text-white rounded-lg transition-colors text-sm"
          >
            {showAddForm ? "Cancel" : "Add IP"}
          </button>
        </div>

        {/* Add Form */}
        {showAddForm && (
          <form onSubmit={handleAdd} className="bg-slate-800/50 rounded-lg border border-slate-700 p-4 space-y-4">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div>
                <label className="block text-sm font-medium text-slate-300 mb-2">
                  IP Address
                </label>
                <input
                  type="text"
                  value={newIp}
                  onChange={(e) => setNewIp(e.target.value)}
                  placeholder="192.168.1.1"
                  className="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-lg text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
                  required
                />
              </div>
              <div>
                <label className="block text-sm font-medium text-slate-300 mb-2">
                  Reason
                </label>
                <input
                  type="text"
                  value={newReason}
                  onChange={(e) => setNewReason(e.target.value)}
                  placeholder="Reason for blocking"
                  className="w-full px-3 py-2 bg-slate-700 border border-slate-600 rounded-lg text-white placeholder-slate-400 focus:outline-none focus:ring-2 focus:ring-blue-500"
                  required
                />
              </div>
            </div>
            <div className="flex items-center justify-between">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="checkbox"
                  checked={isPermanent}
                  onChange={(e) => setIsPermanent(e.target.checked)}
                  className="w-4 h-4 rounded bg-slate-700 border-slate-600 text-blue-500 focus:ring-blue-500"
                />
                <span className="text-sm text-slate-300">Permanent ban (no expiry)</span>
              </label>
              <button
                type="submit"
                disabled={adding}
                className="px-4 py-2 bg-red-500 hover:bg-red-600 disabled:bg-slate-600 text-white rounded-lg transition-colors text-sm"
              >
                {adding ? "Adding..." : "Add to Blacklist"}
              </button>
            </div>
          </form>
        )}

        {/* Error */}
        {error && (
          <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400">
            {error}
          </div>
        )}

        {/* Table */}
        {loading ? (
          <div className="flex items-center justify-center h-64">
            <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
          </div>
        ) : (
          <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
            <table className="w-full">
              <thead>
                <tr className="border-b border-slate-700">
                  <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">IP Address</th>
                  <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Reason</th>
                  <th className="text-center px-4 py-3 text-sm font-medium text-slate-400">Attempts</th>
                  <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Blocked At</th>
                  <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Expires</th>
                  <th className="text-center px-4 py-3 text-sm font-medium text-slate-400">Actions</th>
                </tr>
              </thead>
              <tbody>
                {entries.length === 0 ? (
                  <tr>
                    <td colSpan={6} className="px-4 py-8 text-center text-slate-400">
                      No blacklisted IPs
                    </td>
                  </tr>
                ) : (
                  entries.map((entry, i) => (
                    <tr key={i} className="border-b border-slate-700/50 last:border-0 hover:bg-slate-700/30">
                      <td className="px-4 py-3 text-white font-mono">{entry.ip_address}</td>
                      <td className="px-4 py-3 text-slate-300">{entry.reason}</td>
                      <td className="px-4 py-3 text-center">
                        <span className={`px-2 py-1 text-xs rounded ${
                          entry.failed_attempts >= 3 ? "bg-red-500/20 text-red-400" : "bg-slate-600 text-slate-300"
                        }`}>
                          {entry.failed_attempts}
                        </span>
                      </td>
                      <td className="px-4 py-3 text-slate-400 text-sm">
                        {new Date(entry.blocked_at).toLocaleString()}
                      </td>
                      <td className="px-4 py-3 text-sm">
                        {entry.expires_at ? (
                          <span className={
                            new Date(entry.expires_at) < new Date() 
                              ? "text-green-400" 
                              : "text-amber-400"
                          }>
                            {new Date(entry.expires_at).toLocaleString()}
                          </span>
                        ) : (
                          <span className="text-red-400">Permanent</span>
                        )}
                      </td>
                      <td className="px-4 py-3 text-center">
                        <button
                          onClick={() => handleRemove(entry.ip_address)}
                          className="px-3 py-1 text-xs bg-slate-600 hover:bg-slate-500 text-white rounded transition-colors"
                        >
                          Remove
                        </button>
                      </td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        )}

        {/* Info */}
        <div className="bg-slate-800/30 rounded-lg border border-slate-700 p-4">
          <h3 className="text-sm font-medium text-white mb-2">About IP Blacklist</h3>
          <ul className="text-sm text-slate-400 space-y-1">
            <li>• IPs are automatically blacklisted after 3 failed login attempts</li>
            <li>• Auto-blacklists expire after 1 hour</li>
            <li>• Manual blacklists can be permanent or expire</li>
            <li>• Blacklisted IPs receive 403 Forbidden on all admin routes</li>
          </ul>
        </div>
      </div>
    </AdminShell>
  );
}

