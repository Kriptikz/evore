"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { useAdmin } from "@/context/AdminContext";
import { api, DatabaseSizesResponse, DetailedTable, PostgresTableSize, TableEngineRow } from "@/lib/api";

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB", "PB"];
  const i = Math.floor(Math.log(Math.abs(bytes)) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

function formatNumber(num: number): string {
  if (num >= 1_000_000_000) return (num / 1_000_000_000).toFixed(2) + "B";
  if (num >= 1_000_000) return (num / 1_000_000).toFixed(2) + "M";
  if (num >= 1_000) return (num / 1_000).toFixed(1) + "K";
  return num.toLocaleString();
}

function CompressionBadge({ ratio }: { ratio: number }) {
  const color = ratio >= 10 ? "text-green-400" : ratio >= 5 ? "text-yellow-400" : "text-orange-400";
  return (
    <span className={`${color} font-semibold`}>
      {ratio.toFixed(1)}x
    </span>
  );
}

function EngineTag({ engine }: { engine: string }) {
  const colors: Record<string, string> = {
    "MergeTree": "bg-blue-500/20 text-blue-400 border-blue-500/30",
    "ReplacingMergeTree": "bg-purple-500/20 text-purple-400 border-purple-500/30",
    "SummingMergeTree": "bg-green-500/20 text-green-400 border-green-500/30",
    "AggregatingMergeTree": "bg-cyan-500/20 text-cyan-400 border-cyan-500/30",
  };
  const colorClass = colors[engine] || "bg-slate-500/20 text-slate-400 border-slate-500/30";
  return (
    <span className={`text-xs px-2 py-0.5 rounded border ${colorClass}`}>
      {engine}
    </span>
  );
}

export default function DatabasePage() {
  const { isAuthenticated } = useAdmin();
  const [data, setData] = useState<DatabaseSizesResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedDb, setSelectedDb] = useState<string>("all");
  const [showEngines, setShowEngines] = useState(false);

  const fetchData = useCallback(async () => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    
    try {
      setLoading(true);
      const sizes = await api.getDatabaseSizes();
      setData(sizes);
      setError(null);
    } catch (e) {
      console.error("Failed to fetch database sizes:", e);
      setError(e instanceof Error ? e.message : "Failed to fetch data");
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated]);

  useEffect(() => {
    fetchData();
  }, [fetchData]);

  const filteredTables = data && selectedDb === "all" 
    ? data.clickhouse.tables 
    : data?.clickhouse.tables.filter(t => t.database === selectedDb) || [];

  const engineMap = new Map<string, TableEngineRow>();
  data?.clickhouse.engines.forEach(e => {
    engineMap.set(`${e.database}.${e.table}`, e);
  });

  return (
    <AdminShell title="Database Storage" subtitle="Production storage monitoring for ClickHouse & PostgreSQL">
      <div className="space-y-6">
        {/* Controls Row */}
        <div className="flex justify-end">
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

        {data && (
          <>
            {/* Summary Cards */}
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-4 gap-4">
              {/* Total Storage */}
              <div className="bg-gradient-to-br from-indigo-500/20 to-purple-600/10 border border-indigo-500/30 rounded-xl p-5">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-10 h-10 bg-indigo-500/20 rounded-lg flex items-center justify-center">
                    <svg className="w-5 h-5 text-indigo-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 7v10c0 2.21 3.582 4 8 4s8-1.79 8-4V7M4 7c0 2.21 3.582 4 8 4s8-1.79 8-4M4 7c0-2.21 3.582-4 8-4s8 1.79 8 4" />
                    </svg>
                  </div>
                  <div>
                    <p className="text-indigo-400 text-xs font-medium uppercase tracking-wider">Total Storage</p>
                    <p className="text-2xl font-bold text-white">{formatBytes(data.summary.total_bytes)}</p>
                  </div>
                </div>
                <p className="text-slate-400 text-sm">{formatNumber(data.summary.total_rows)} total rows</p>
              </div>

              {/* ClickHouse */}
              <div className="bg-gradient-to-br from-orange-500/20 to-orange-600/10 border border-orange-500/30 rounded-xl p-5">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-10 h-10 bg-orange-500/20 rounded-lg flex items-center justify-center">
                    <span className="text-orange-400 font-bold text-sm">CH</span>
                  </div>
                  <div>
                    <p className="text-orange-400 text-xs font-medium uppercase tracking-wider">ClickHouse</p>
                    <p className="text-2xl font-bold text-white">{formatBytes(data.clickhouse.total_bytes)}</p>
                  </div>
                </div>
                <p className="text-slate-400 text-sm">{formatNumber(data.clickhouse.total_rows)} rows</p>
              </div>

              {/* PostgreSQL */}
              <div className="bg-gradient-to-br from-blue-500/20 to-blue-600/10 border border-blue-500/30 rounded-xl p-5">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-10 h-10 bg-blue-500/20 rounded-lg flex items-center justify-center">
                    <span className="text-blue-400 font-bold text-sm">PG</span>
                  </div>
                  <div>
                    <p className="text-blue-400 text-xs font-medium uppercase tracking-wider">PostgreSQL</p>
                    <p className="text-2xl font-bold text-white">{formatBytes(data.postgres.database_size_bytes)}</p>
                  </div>
                </div>
                <p className="text-slate-400 text-sm">{formatNumber(data.postgres.total_rows)} rows</p>
              </div>

              {/* Compression */}
              <div className="bg-gradient-to-br from-green-500/20 to-green-600/10 border border-green-500/30 rounded-xl p-5">
                <div className="flex items-center gap-3 mb-3">
                  <div className="w-10 h-10 bg-green-500/20 rounded-lg flex items-center justify-center">
                    <svg className="w-5 h-5 text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 14l-7 7m0 0l-7-7m7 7V3" />
                    </svg>
                  </div>
                  <div>
                    <p className="text-green-400 text-xs font-medium uppercase tracking-wider">Compression</p>
                    <p className="text-2xl font-bold text-white">{data.summary.compression_ratio.toFixed(1)}x</p>
                  </div>
                </div>
                <p className="text-slate-400 text-sm">
                  {formatBytes(data.clickhouse.total_bytes_uncompressed)} → {formatBytes(data.clickhouse.total_bytes)}
                </p>
              </div>
            </div>

            {/* ClickHouse Databases */}
            <div className="bg-slate-800/50 border border-slate-700 rounded-xl p-6">
              <h2 className="text-xl font-bold text-white mb-4 flex items-center gap-2">
                <span className="text-orange-400">ClickHouse</span> Databases
              </h2>
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {data.clickhouse.databases.map((db) => (
                  <div 
                    key={db.database} 
                    className={`bg-slate-900/50 border rounded-lg p-4 cursor-pointer transition-all hover:bg-slate-900 ${
                      selectedDb === db.database ? 'border-orange-500' : 'border-slate-600'
                    }`}
                    onClick={() => setSelectedDb(selectedDb === db.database ? "all" : db.database)}
                  >
                    <div className="flex justify-between items-start mb-2">
                      <h3 className="font-mono text-white font-semibold">{db.database}</h3>
                      <span className="text-orange-400 font-bold">{formatBytes(db.bytes_on_disk)}</span>
                    </div>
                    <div className="flex justify-between text-sm text-slate-400">
                      <span>{formatNumber(db.total_rows)} rows</span>
                      <span>{db.table_count} tables</span>
                    </div>
                  </div>
                ))}
              </div>
            </div>

            {/* ClickHouse Tables */}
            <div className="bg-slate-800/50 border border-slate-700 rounded-xl p-6">
              <div className="flex justify-between items-center mb-4">
                <h2 className="text-xl font-bold text-white flex items-center gap-2">
                  <span className="text-orange-400">ClickHouse</span> Tables
                  {selectedDb !== "all" && (
                    <span className="text-sm font-normal text-slate-400">
                      (filtered to {selectedDb})
                    </span>
                  )}
                </h2>
                <button
                  onClick={() => setShowEngines(!showEngines)}
                  className={`text-sm px-3 py-1 rounded transition-colors ${
                    showEngines 
                      ? 'bg-orange-500/20 text-orange-400 border border-orange-500/30' 
                      : 'bg-slate-700 text-slate-300 hover:bg-slate-600'
                  }`}
                >
                  {showEngines ? "Hide Engines" : "Show Engines"}
                </button>
              </div>
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="text-left text-slate-400 text-sm border-b border-slate-700">
                      <th className="pb-3 font-medium">Database</th>
                      <th className="pb-3 font-medium">Table</th>
                      {showEngines && <th className="pb-3 font-medium">Engine</th>}
                      <th className="pb-3 font-medium text-right">Size</th>
                      <th className="pb-3 font-medium text-right">Compression</th>
                      <th className="pb-3 font-medium text-right">Rows</th>
                      <th className="pb-3 font-medium text-right">Avg Row</th>
                      <th className="pb-3 font-medium text-right">Parts</th>
                      <th className="pb-3 font-medium">Last Modified</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredTables.map((tbl) => {
                      const engine = engineMap.get(`${tbl.database}.${tbl.table}`);
                      return (
                        <tr key={`${tbl.database}.${tbl.table}`} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                          <td className="py-3 text-slate-400 font-mono text-sm">{tbl.database}</td>
                          <td className="py-3 text-white font-mono">{tbl.table}</td>
                          {showEngines && (
                            <td className="py-3">
                              {engine && <EngineTag engine={engine.engine} />}
                            </td>
                          )}
                          <td className="py-3 text-right text-orange-400 font-semibold">
                            {formatBytes(tbl.bytes_on_disk)}
                          </td>
                          <td className="py-3 text-right">
                            <CompressionBadge ratio={tbl.compression_ratio} />
                          </td>
                          <td className="py-3 text-right text-slate-300">
                            {formatNumber(tbl.total_rows)}
                          </td>
                          <td className="py-3 text-right text-slate-400 text-sm">
                            {formatBytes(tbl.avg_row_size)}
                          </td>
                          <td className="py-3 text-right text-slate-400">
                            {tbl.parts_count}
                          </td>
                          <td className="py-3 text-slate-400 text-sm">
                            {tbl.last_modified}
                          </td>
                        </tr>
                      );
                    })}
                    {filteredTables.length === 0 && (
                      <tr>
                        <td colSpan={showEngines ? 9 : 8} className="py-4 text-center text-slate-500">
                          No tables found
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>

            {/* PostgreSQL Tables */}
            <div className="bg-slate-800/50 border border-slate-700 rounded-xl p-6">
              <h2 className="text-xl font-bold text-white mb-4 flex items-center gap-2">
                <span className="text-blue-400">PostgreSQL</span> Tables
                <span className="text-sm font-normal text-slate-400">
                  ({data.postgres.database_name})
                </span>
              </h2>
              <div className="overflow-x-auto">
                <table className="w-full">
                  <thead>
                    <tr className="text-left text-slate-400 text-sm border-b border-slate-700">
                      <th className="pb-3 font-medium">Table</th>
                      <th className="pb-3 font-medium text-right">Total Size</th>
                      <th className="pb-3 font-medium text-right">Data</th>
                      <th className="pb-3 font-medium text-right">Indexes</th>
                      <th className="pb-3 font-medium text-right">Rows</th>
                      <th className="pb-3 font-medium text-right">Avg Row</th>
                      <th className="pb-3 font-medium text-right">Dead Tuples</th>
                      <th className="pb-3 font-medium">Last Vacuum</th>
                      <th className="pb-3 font-medium">Last Analyze</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.postgres.table_sizes.map((tbl) => (
                      <tr key={tbl.table_name} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                        <td className="py-3 text-white font-mono">{tbl.table_name}</td>
                        <td className="py-3 text-right text-blue-400 font-semibold">
                          {formatBytes(tbl.total_size_bytes)}
                        </td>
                        <td className="py-3 text-right text-slate-300">
                          {formatBytes(tbl.table_size_bytes)}
                        </td>
                        <td className="py-3 text-right text-slate-400">
                          {formatBytes(tbl.index_size_bytes)}
                        </td>
                        <td className="py-3 text-right text-slate-300">
                          {formatNumber(tbl.row_count)}
                        </td>
                        <td className="py-3 text-right text-slate-400 text-sm">
                          {formatBytes(tbl.avg_row_size)}
                        </td>
                        <td className="py-3 text-right">
                          <span className={tbl.dead_tuples > 1000 ? "text-yellow-400" : "text-slate-400"}>
                            {formatNumber(tbl.dead_tuples)}
                          </span>
                        </td>
                        <td className="py-3 text-slate-400 text-sm">
                          {tbl.last_vacuum || <span className="text-yellow-400">Never</span>}
                        </td>
                        <td className="py-3 text-slate-400 text-sm">
                          {tbl.last_analyze || <span className="text-yellow-400">Never</span>}
                        </td>
                      </tr>
                    ))}
                    {data.postgres.table_sizes.length === 0 && (
                      <tr>
                        <td colSpan={9} className="py-4 text-center text-slate-500">
                          No tables found
                        </td>
                      </tr>
                    )}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Table Engines (Collapsible) */}
            {showEngines && (
              <div className="bg-slate-800/50 border border-slate-700 rounded-xl p-6">
                <h2 className="text-xl font-bold text-white mb-4 flex items-center gap-2">
                  <span className="text-orange-400">ClickHouse</span> Table Engines & Keys
                </h2>
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <thead>
                      <tr className="text-left text-slate-400 border-b border-slate-700">
                        <th className="pb-3 font-medium">Database</th>
                        <th className="pb-3 font-medium">Table</th>
                        <th className="pb-3 font-medium">Engine</th>
                        <th className="pb-3 font-medium">Partition Key</th>
                        <th className="pb-3 font-medium">Sorting Key</th>
                        <th className="pb-3 font-medium">Primary Key</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.clickhouse.engines.map((e) => (
                        <tr key={`${e.database}.${e.table}`} className="border-t border-slate-700/50 hover:bg-slate-700/30">
                          <td className="py-2 text-slate-400 font-mono">{e.database}</td>
                          <td className="py-2 text-white font-mono">{e.table}</td>
                          <td className="py-2"><EngineTag engine={e.engine} /></td>
                          <td className="py-2 text-slate-300 font-mono text-xs">
                            {e.partition_key || <span className="text-slate-500">—</span>}
                          </td>
                          <td className="py-2 text-slate-300 font-mono text-xs max-w-xs truncate" title={e.sorting_key}>
                            {e.sorting_key || <span className="text-slate-500">—</span>}
                          </td>
                          <td className="py-2 text-slate-300 font-mono text-xs max-w-xs truncate" title={e.primary_key}>
                            {e.primary_key || <span className="text-slate-500">—</span>}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </>
        )}
      </div>
    </AdminShell>
  );
}
