"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import Link from "next/link";
import { AdminShell } from "@/components/admin/AdminShell";
import { useAdmin } from "@/context/AdminContext";
import { api, RoundStatus, RoundWithData, RoundStatsResponse, FilterMode, BackfillRoundsTaskState } from "@/lib/api";

type WorkflowStep = "meta" | "txns" | "reconstruct" | "verify" | "finalize";
type Tab = "backfill" | "data";

function StepBadge({ done, active, label }: { done: boolean; active?: boolean; label: string }) {
  return (
    <span
      className={`px-2 py-1 text-xs rounded-full ${
        done
          ? "bg-green-500/20 text-green-400"
          : active
          ? "bg-blue-500/20 text-blue-400 animate-pulse"
          : "bg-slate-700 text-slate-500"
      }`}
    >
      {label}
    </span>
  );
}

type RoundAction = WorkflowStep | "reset_txns" | "queue_automation";

function RoundStatusRow({
  round,
  onAction,
  loading,
}: {
  round: RoundStatus;
  onAction: (roundId: number, action: RoundAction) => void;
  loading: number | null;
}) {
  const nextStep = !round.meta_fetched
    ? "meta"
    : !round.transactions_fetched
    ? "txns"
    : !round.reconstructed
    ? "reconstruct"
    : !round.verified
    ? "verify"
    : !round.finalized
    ? "finalize"
    : null;

  const isLoading = loading === round.round_id;

  // Show reset button if txns fetched but not reconstructed (could mean txns weren't actually stored)
  const showResetTxns = round.transactions_fetched && !round.reconstructed;

  return (
    <tr className="border-b border-slate-700/50 last:border-0 hover:bg-slate-700/30">
      <td className="px-4 py-3 text-white font-mono">{round.round_id}</td>
      <td className="px-4 py-3">
        <div className="flex gap-1 flex-wrap">
          <StepBadge done={round.meta_fetched} label="Meta" />
          <StepBadge done={round.transactions_fetched} label="Txns" />
          <StepBadge done={round.reconstructed} label="Rebuild" />
          <StepBadge done={round.verified} label="Verify" />
          <StepBadge done={round.finalized} label="Final" />
        </div>
      </td>
      <td className="px-4 py-3 text-slate-400 text-sm">{round.transaction_count}</td>
      <td className="px-4 py-3 text-slate-400 text-sm">{round.deployment_count}</td>
      <td className="px-4 py-3">
        <div className="flex gap-2 items-center">
          {nextStep && (
            <button
              onClick={() => onAction(round.round_id, nextStep)}
              disabled={isLoading}
              className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                isLoading
                  ? "bg-slate-700 text-slate-400 cursor-not-allowed"
                  : "bg-blue-500 hover:bg-blue-600 text-white"
              }`}
            >
              {isLoading ? (
                <span className="flex items-center gap-2">
                  <span className="w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin" />
                  Running...
                </span>
              ) : (
                {
                  meta: "Fetch Meta",
                  txns: "Fetch Txns",
                  reconstruct: "Reconstruct",
                  verify: "Verify",
                  finalize: "Finalize",
                }[nextStep]
              )}
            </button>
          )}
          {showResetTxns && !isLoading && (
            <button
              onClick={() => onAction(round.round_id, "reset_txns")}
              className="px-3 py-1.5 text-sm rounded-lg bg-amber-500/20 hover:bg-amber-500/30 text-amber-400 transition-colors"
              title="Reset txns status to re-fetch"
            >
              Reset Txns
            </button>
          )}
          {round.transactions_fetched && round.transaction_count > 0 && (
            <>
              <Link
                href={`/admin/transactions?round_id=${round.round_id}`}
                className="px-3 py-1.5 text-sm rounded-lg bg-purple-500/20 hover:bg-purple-500/30 text-purple-400 transition-colors"
              >
                View Txns
              </Link>
              <button
                onClick={() => onAction(round.round_id, "queue_automation")}
                className="px-3 py-1.5 text-sm rounded-lg bg-cyan-500/20 hover:bg-cyan-500/30 text-cyan-400 transition-colors"
                title="Queue automation state fetching for all deployments"
              >
                Queue Auto
              </button>
            </>
          )}
          {!nextStep && (
            <span className="text-green-400 text-sm">✓ Complete</span>
          )}
        </div>
      </td>
    </tr>
  );
}

// Format SOL from lamports
const formatSol = (lamports: number) => (lamports / 1e9).toFixed(4);
const truncate = (s: string) => s.length > 12 ? `${s.slice(0, 6)}...${s.slice(-4)}` : s;

function RoundDataRow({
  round,
  selected,
  onSelect,
  loading,
}: {
  round: RoundWithData;
  selected: boolean;
  onSelect: (roundId: number, selected: boolean) => void;
  loading: boolean;
}) {
  const hasMissingDeployments = round.deployment_count === 0;

  return (
    <tr className={`border-b border-slate-700/50 last:border-0 hover:bg-slate-700/30 ${hasMissingDeployments ? 'bg-red-500/5' : ''} ${selected ? 'bg-blue-500/10' : ''}`}>
      <td className="px-4 py-3">
        <input
          type="checkbox"
          checked={selected}
          onChange={(e) => onSelect(round.round_id, e.target.checked)}
          disabled={loading}
          className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-blue-500"
        />
      </td>
      <td className="px-4 py-3 text-white font-mono">{round.round_id}</td>
      <td className="px-4 py-3 text-slate-400 text-sm">◼ {round.winning_square}</td>
      <td className="px-4 py-3 text-slate-400 text-sm font-mono">{truncate(round.top_miner)}</td>
      <td className="px-4 py-3 text-slate-400 text-sm">{formatSol(round.total_deployed)}</td>
      <td className="px-4 py-3 text-slate-400 text-sm">{formatSol(round.total_winnings)}</td>
      <td className="px-4 py-3">
        <span className={`px-2 py-1 text-xs rounded-full ${
          round.deployment_count > 0 
            ? 'bg-green-500/20 text-green-400' 
            : 'bg-red-500/20 text-red-400'
        }`}>
          {round.deployment_count}
        </span>
      </td>
      <td className="px-4 py-3">
        <span className={`px-2 py-1 text-xs rounded-full ${
          round.source === 'live' 
            ? 'bg-emerald-500/20 text-emerald-400' 
            : 'bg-slate-600 text-slate-300'
        }`}>
          {round.source}
        </span>
      </td>
    </tr>
  );
}

export default function BackfillPage() {
  const { isAuthenticated } = useAdmin();
  const [activeTab, setActiveTab] = useState<Tab>("data");
  
  // Backfill workflow state
  const [pendingRounds, setPendingRounds] = useState<RoundStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  // Backfill form state
  const [stopAtRound, setStopAtRound] = useState<string>("");
  const [maxPages, setMaxPages] = useState<string>("10000");
  const [backfillLoading, setBackfillLoading] = useState(false);
  const [backfillTaskState, setBackfillTaskState] = useState<BackfillRoundsTaskState | null>(null);
  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // Data viewer state
  const [roundsData, setRoundsData] = useState<RoundWithData[]>([]);
  const [missingRoundIds, setMissingRoundIds] = useState<number[]>([]);
  const [dataLoading, setDataLoading] = useState(false);
  const [filterMode, setFilterMode] = useState<FilterMode | "missing_rounds">("all");
  const [selectedRounds, setSelectedRounds] = useState<Set<number>>(new Set());
  const [bulkDeleting, setBulkDeleting] = useState(false);
  const [addingToBackfill, setAddingToBackfill] = useState(false);
  
  // Round stats
  const [stats, setStats] = useState<RoundStatsResponse | null>(null);
  const [statsLoading, setStatsLoading] = useState(false);
  
  // Round ID filter state
  const [startRound, setStartRound] = useState<string>("");
  const [endRound, setEndRound] = useState<string>("");
  
  // Pagination state
  const [hasMore, setHasMore] = useState(false);
  const [nextCursor, setNextCursor] = useState<number | null>(null);
  const [currentPage, setCurrentPage] = useState(1);
  const [loadingMore, setLoadingMore] = useState(false);

  const fetchPendingRounds = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await api.getPendingRounds();
      setPendingRounds(res.pending);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch pending rounds");
    } finally {
      setLoading(false);
    }
  }, [isAuthenticated]);

  const fetchBackfillStatus = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await api.getBackfillRoundsStatus();
      setBackfillTaskState(res);
      // If task is running, keep polling
      if (res.status === "running") {
        if (!pollIntervalRef.current) {
          pollIntervalRef.current = setInterval(fetchBackfillStatus, 1000);
        }
      } else {
        // Task finished, stop polling
        if (pollIntervalRef.current) {
          clearInterval(pollIntervalRef.current);
          pollIntervalRef.current = null;
        }
      }
    } catch (err) {
      console.error("Failed to fetch backfill status:", err);
    }
  }, [isAuthenticated]);

  const fetchStats = useCallback(async () => {
    if (!isAuthenticated) return;
    setStatsLoading(true);
    try {
      const res = await api.getRoundStats({
        roundIdGte: startRound ? parseInt(startRound) : undefined,
        roundIdLte: endRound ? parseInt(endRound) : undefined,
      });
      setStats(res);
    } catch (err) {
      console.error("Failed to fetch stats:", err);
    } finally {
      setStatsLoading(false);
    }
  }, [isAuthenticated, startRound, endRound]);

  const fetchRoundsData = useCallback(async (reset = true) => {
    if (!isAuthenticated) return;
    setDataLoading(true);
    setMissingRoundIds([]);
    setRoundsData([]);
    
    try {
      if (filterMode === "missing_rounds") {
        // Fetch missing round IDs
        const res = await api.getMissingRounds({
          limit: 100,
          page: 1,
          roundIdGte: startRound ? parseInt(startRound) : undefined,
          roundIdLte: endRound ? parseInt(endRound) : undefined,
        });
        setMissingRoundIds(res.missing_round_ids);
        setHasMore(res.has_more);
        setCurrentPage(1);
        setNextCursor(null);
      } else {
        // Fetch rounds with data
        const res = await api.getRoundsWithData({
          limit: 100,
          filterMode: filterMode === "all" ? undefined : filterMode,
          roundIdGte: startRound ? parseInt(startRound) : undefined,
          roundIdLte: endRound ? parseInt(endRound) : undefined,
        });
        setRoundsData(res.rounds);
        setHasMore(res.has_more);
        setNextCursor(res.next_cursor ?? null);
        setCurrentPage(1);
      }
      setError(null);
      if (reset) {
        setSelectedRounds(new Set());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch rounds data");
    } finally {
      setDataLoading(false);
    }
  }, [isAuthenticated, filterMode, startRound, endRound]);
  
  const loadMoreRoundsData = useCallback(async () => {
    if (!isAuthenticated || !hasMore || loadingMore) return;
    setLoadingMore(true);
    try {
      if (filterMode === "missing_rounds") {
        const nextPage = currentPage + 1;
        const res = await api.getMissingRounds({
          limit: 100,
          page: nextPage,
          roundIdGte: startRound ? parseInt(startRound) : undefined,
          roundIdLte: endRound ? parseInt(endRound) : undefined,
        });
        if (res.missing_round_ids.length > 0) {
          setMissingRoundIds(prev => [...prev, ...res.missing_round_ids]);
          setHasMore(res.has_more);
          setCurrentPage(nextPage);
        } else {
          setHasMore(false);
        }
      } else if (nextCursor) {
        const res = await api.getRoundsWithData({
          limit: 100,
          before: nextCursor,
          filterMode: filterMode === "all" ? undefined : filterMode,
          roundIdGte: startRound ? parseInt(startRound) : undefined,
          roundIdLte: endRound ? parseInt(endRound) : undefined,
        });
        if (res.rounds.length > 0) {
          setRoundsData(prev => [...prev, ...res.rounds]);
          setHasMore(res.has_more);
          setNextCursor(res.next_cursor ?? null);
        } else {
          setHasMore(false);
          setNextCursor(null);
        }
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load more rounds");
    } finally {
      setLoadingMore(false);
    }
  }, [isAuthenticated, hasMore, nextCursor, currentPage, loadingMore, filterMode, startRound, endRound]);

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    fetchPendingRounds();
    fetchRoundsData();
    fetchStats();
    fetchBackfillStatus();
    
    // Cleanup polling on unmount
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
        pollIntervalRef.current = null;
      }
    };
  }, [fetchPendingRounds, fetchRoundsData, fetchStats, fetchBackfillStatus, isAuthenticated]);

  useEffect(() => {
    if (isAuthenticated) {
      fetchRoundsData(true);
    }
  }, [filterMode, fetchRoundsData, isAuthenticated]);

  // Separate effect for round ID filters - only triggers on Apply button or Enter
  const handleApplyFilters = useCallback(() => {
    fetchRoundsData(true);
  }, [fetchRoundsData]);

  const handleBackfill = async () => {
    setBackfillLoading(true);
    setMessage(null);
    setError(null);
    try {
      const res = await api.backfillRounds(
        stopAtRound ? parseInt(stopAtRound) : undefined,
        maxPages ? parseInt(maxPages) : undefined
      );
      setMessage(res.message);
      // Start polling for status
      fetchBackfillStatus();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Backfill failed");
    } finally {
      setBackfillLoading(false);
    }
  };

  const handleCancelBackfill = async () => {
    setMessage(null);
    setError(null);
    try {
      const res = await api.cancelBackfillRounds();
      setMessage(res.message);
      // Keep polling to see when it actually cancels
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel backfill");
    }
  };

  // Helper to format duration
  const formatDuration = (ms: number) => {
    if (ms < 1000) return `${ms}ms`;
    const secs = Math.floor(ms / 1000);
    if (secs < 60) return `${secs}s`;
    const mins = Math.floor(secs / 60);
    const remainingSecs = secs % 60;
    if (mins < 60) return `${mins}m ${remainingSecs}s`;
    const hours = Math.floor(mins / 60);
    const remainingMins = mins % 60;
    return `${hours}h ${remainingMins}m`;
  };

  const handleAction = async (roundId: number, action: RoundAction) => {
    setActionLoading(roundId);
    setMessage(null);
    setError(null);
    try {
      switch (action) {
        case "txns":
          const txRes = await api.fetchRoundTransactions(roundId);
          setMessage(`Round ${roundId}: fetched ${txRes.transactions_fetched} transactions`);
          break;
        case "reset_txns":
          const resetRes = await api.resetTxnsStatus(roundId);
          setMessage(`Round ${roundId}: ${resetRes.message}`);
          break;
        case "reconstruct":
          const recRes = await api.reconstructRound(roundId);
          setMessage(`Round ${roundId}: reconstructed ${recRes.deployments_reconstructed} deployments`);
          break;
        case "verify":
          await api.verifyRound(roundId);
          setMessage(`Round ${roundId}: marked as verified`);
          break;
        case "finalize":
          const finRes = await api.finalizeRound(roundId);
          setMessage(`Round ${roundId}: finalized with ${finRes.deployments_stored} deployments`);
          break;
        case "queue_automation":
          // Use new queue-based system - instant, no timeout
          const autoRes = await api.queueRoundForParsing(roundId);
          if (autoRes.success) {
            setMessage(`Round ${roundId}: added to parse queue (background worker will process)`);
          } else {
            setMessage(`Round ${roundId}: ${autoRes.message}`);
          }
          break;
        default:
          break;
      }
      fetchPendingRounds();
      fetchRoundsData();
    } catch (err) {
      setError(err instanceof Error ? err.message : `Action failed for round ${roundId}`);
    } finally {
      setActionLoading(null);
    }
  };

  const handleSelectRound = (roundId: number, selected: boolean) => {
    setSelectedRounds(prev => {
      const next = new Set(prev);
      if (selected) {
        next.add(roundId);
      } else {
        next.delete(roundId);
      }
      return next;
    });
  };

  const handleSelectAll = () => {
    setSelectedRounds(new Set(roundsData.map(r => r.round_id)));
  };

  const handleDeselectAll = () => {
    setSelectedRounds(new Set());
  };

  const handleSelectMissing = () => {
    setSelectedRounds(new Set(roundsData.filter(r => r.deployment_count === 0).map(r => r.round_id)));
  };

  const handleBulkDelete = async (deleteRounds: boolean, deleteDeployments: boolean) => {
    if (selectedRounds.size === 0) return;
    
    setBulkDeleting(true);
    setMessage(null);
    setError(null);
    try {
      const res = await api.bulkDeleteRounds(
        Array.from(selectedRounds),
        deleteRounds,
        deleteDeployments
      );
      setMessage(res.message);
      setSelectedRounds(new Set());
      fetchPendingRounds();
      fetchRoundsData();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Bulk delete failed");
    } finally {
      setBulkDeleting(false);
    }
  };

  const handleAddToBackfill = async () => {
    if (selectedRounds.size === 0) return;
    
    setAddingToBackfill(true);
    setMessage(null);
    setError(null);
    
    try {
      const res = await api.addToBackfillWorkflow(Array.from(selectedRounds));
      setMessage(res.message);
      setSelectedRounds(new Set());
      fetchPendingRounds();
      fetchRoundsData();
      // Switch to backfill tab to show the newly added rounds
      setActiveTab("backfill");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add rounds to backfill");
    } finally {
      setAddingToBackfill(false);
    }
  };

  const missingCount = roundsData.filter(r => r.deployment_count === 0).length;

  return (
    <AdminShell title="Historical Backfill" subtitle="Manage and verify round data">
      <div className="space-y-6">
        {/* Tabs */}
        <div className="flex gap-2 border-b border-slate-700 pb-2">
          <button
            onClick={() => setActiveTab("data")}
            className={`px-4 py-2 rounded-t-lg transition-colors ${
              activeTab === "data"
                ? "bg-slate-800 text-white border-b-2 border-blue-500"
                : "text-slate-400 hover:text-white"
            }`}
          >
            Round Data ({roundsData.length})
            {missingCount > 0 && (
              <span className="ml-2 px-1.5 py-0.5 text-xs bg-red-500/20 text-red-400 rounded">
                {missingCount} missing
              </span>
            )}
          </button>
          <button
            onClick={() => setActiveTab("backfill")}
            className={`px-4 py-2 rounded-t-lg transition-colors ${
              activeTab === "backfill"
                ? "bg-slate-800 text-white border-b-2 border-blue-500"
                : "text-slate-400 hover:text-white"
            }`}
          >
            Backfill Workflow ({pendingRounds.length})
          </button>
        </div>

        {/* Messages */}
        {message && (
          <div className="p-4 bg-green-500/10 border border-green-500/30 rounded-lg text-green-400">
            {message}
          </div>
        )}
        {error && (
          <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400">
            {error}
          </div>
        )}

        {activeTab === "data" && (
          <>
            {/* Data Viewer Controls */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4 space-y-4">
              {/* Round ID Range Filters */}
              <div className="flex flex-wrap items-center gap-4">
                <div className="flex items-center gap-2">
                  <label className="text-sm text-slate-400">From Round:</label>
                  <input
                    type="number"
                    value={startRound}
                    onChange={(e) => setStartRound(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleApplyFilters()}
                    placeholder="Start"
                    className="w-28 px-3 py-1.5 bg-slate-900 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <div className="flex items-center gap-2">
                  <label className="text-sm text-slate-400">To Round:</label>
                  <input
                    type="number"
                    value={endRound}
                    onChange={(e) => setEndRound(e.target.value)}
                    onKeyDown={(e) => e.key === "Enter" && handleApplyFilters()}
                    placeholder="End"
                    className="w-28 px-3 py-1.5 bg-slate-900 border border-slate-600 rounded-lg text-white text-sm focus:outline-none focus:ring-2 focus:ring-blue-500"
                  />
                </div>
                <button
                  onClick={handleApplyFilters}
                  disabled={dataLoading}
                  className="px-4 py-1.5 bg-blue-500 hover:bg-blue-600 text-white text-sm rounded-lg disabled:opacity-50 transition-colors"
                >
                  Apply
                </button>
                {(startRound || endRound) && (
                  <button
                    onClick={() => { setStartRound(""); setEndRound(""); }}
                    className="px-3 py-1.5 bg-slate-700 hover:bg-slate-600 text-white text-sm rounded-lg transition-colors"
                  >
                    Clear
                  </button>
                )}
              </div>
              
              {/* Filter Mode Selector */}
              <div className="flex items-center gap-2">
                <span className="text-sm text-slate-400">Filter:</span>
                <select
                  value={filterMode}
                  onChange={(e) => setFilterMode(e.target.value as FilterMode | "missing_rounds")}
                  className="px-3 py-1.5 bg-slate-700 border border-slate-600 rounded-lg text-sm text-white focus:ring-blue-500 focus:border-blue-500"
                >
                  <option value="all">All Rounds</option>
                  <option value="missing_deployments">Missing Deployments Only {stats ? `(${stats.missing_deployments_count.toLocaleString()})` : ""}</option>
                  <option value="invalid_deployments">Invalid Deployments Only {stats ? `(${stats.invalid_deployments_count.toLocaleString()})` : ""}</option>
                  <option value="missing_rounds">Missing Rounds (Gaps) {stats ? `(${stats.missing_rounds_count.toLocaleString()})` : ""}</option>
                </select>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-sm text-slate-400">
                  {filterMode === "missing_rounds" 
                    ? `${missingRoundIds.length} round IDs loaded`
                    : `${roundsData.length} rounds loaded`
                  }
                  {hasMore && " (more available)"}
                </span>
                <button
                  onClick={() => { fetchRoundsData(true); fetchStats(); }}
                  className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                >
                  Refresh
                </button>
              </div>
            </div>
            
            {/* Stats Summary */}
            {stats && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
                  <p className="text-xs text-slate-400 mb-1">Total Rounds</p>
                  <p className="text-xl font-bold text-white">{stats.total_rounds.toLocaleString()}</p>
                  <p className="text-xs text-slate-500">
                    {stats.min_stored_round.toLocaleString()} - {stats.max_stored_round.toLocaleString()}
                  </p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-red-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Missing Deployments</p>
                  <p className={`text-xl font-bold ${stats.missing_deployments_count > 0 ? 'text-red-400' : 'text-green-400'}`}>
                    {stats.missing_deployments_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">rounds with no deployment data</p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-orange-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Invalid Deployments</p>
                  <p className={`text-xl font-bold ${stats.invalid_deployments_count > 0 ? 'text-orange-400' : 'text-green-400'}`}>
                    {stats.invalid_deployments_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">rounds with mismatched totals</p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-yellow-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Missing Rounds</p>
                  <p className={`text-xl font-bold ${stats.missing_rounds_count > 0 ? 'text-yellow-400' : 'text-green-400'}`}>
                    {stats.missing_rounds_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">gaps in round sequence</p>
                </div>
              </div>
            )}

            {/* Rounds Data Table */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
              <div className="px-6 py-4 border-b border-slate-700">
                <h2 className="text-lg font-semibold text-white">
                  {filterMode === "missing_rounds" ? "Missing Round IDs" : 
                   filterMode === "missing_deployments" ? "Rounds Missing Deployments" :
                   filterMode === "invalid_deployments" ? "Rounds with Invalid Deployments" :
                   "Stored Rounds"}
                </h2>
              </div>

              {dataLoading ? (
                <div className="flex items-center justify-center h-48">
                  <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : filterMode === "missing_rounds" ? (
                missingRoundIds.length === 0 ? (
                  <div className="p-8 text-center text-slate-400">
                    No missing rounds! Sequence is complete.
                  </div>
                ) : (
                  <div className="p-6">
                    <div className="flex flex-wrap gap-2 mb-4">
                      {missingRoundIds.map(id => (
                        <span 
                          key={id} 
                          className="px-3 py-1.5 bg-yellow-500/20 text-yellow-400 text-sm font-mono rounded-lg"
                        >
                          {id.toLocaleString()}
                        </span>
                      ))}
                    </div>
                    {hasMore && (
                      <button
                        onClick={loadMoreRoundsData}
                        disabled={loadingMore}
                        className="w-full py-3 text-sm bg-slate-700 hover:bg-slate-600 text-slate-300 rounded-lg transition-colors disabled:opacity-50"
                      >
                        {loadingMore ? "Loading..." : "Load More Missing Rounds"}
                      </button>
                    )}
                  </div>
                )
              ) : roundsData.length === 0 ? (
                <div className="p-8 text-center text-slate-400">
                  {filterMode === "missing_deployments" ? "All rounds have deployment data!" : 
                   filterMode === "invalid_deployments" ? "All rounds have valid deployment totals!" :
                   "No rounds found. Start a backfill."}
                </div>
              ) : (
                <>
                  {/* Selection Actions Bar */}
                  {selectedRounds.size > 0 && (
                    <div className="mx-4 mt-4 p-4 bg-blue-500/10 border border-blue-500/30 rounded-lg flex items-center justify-between">
                      <span className="text-blue-400">
                        {selectedRounds.size} round{selectedRounds.size > 1 ? 's' : ''} selected
                      </span>
                      <div className="flex gap-2">
                        <button
                          onClick={handleAddToBackfill}
                          disabled={addingToBackfill || bulkDeleting}
                          className="px-3 py-1.5 text-sm bg-emerald-500 hover:bg-emerald-600 text-white rounded-lg disabled:opacity-50 flex items-center gap-2"
                        >
                          {addingToBackfill ? (
                            <>
                              <span className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                              Adding...
                            </>
                          ) : (
                            "Add to Backfill Workflow"
                          )}
                        </button>
                        <button
                          onClick={() => handleBulkDelete(false, true)}
                          disabled={bulkDeleting || addingToBackfill}
                          className="px-3 py-1.5 text-sm bg-orange-500 hover:bg-orange-600 text-white rounded-lg disabled:opacity-50"
                        >
                          {bulkDeleting ? "Deleting..." : "Delete Deployments Only"}
                        </button>
                        <button
                          onClick={() => handleBulkDelete(true, true)}
                          disabled={bulkDeleting || addingToBackfill}
                          className="px-3 py-1.5 text-sm bg-red-500 hover:bg-red-600 text-white rounded-lg disabled:opacity-50"
                        >
                          {bulkDeleting ? "Deleting..." : "Delete All Data"}
                        </button>
                        <button
                          onClick={handleDeselectAll}
                          disabled={bulkDeleting || addingToBackfill}
                          className="px-3 py-1.5 text-sm bg-slate-600 hover:bg-slate-500 text-white rounded-lg disabled:opacity-50"
                        >
                          Clear
                        </button>
                      </div>
                    </div>
                  )}

                  <div className="overflow-x-auto">
                    <table className="w-full">
                      <thead>
                        <tr className="border-b border-slate-700">
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400 w-12">
                            <input
                              type="checkbox"
                              checked={selectedRounds.size === roundsData.length && roundsData.length > 0}
                              onChange={(e) => e.target.checked ? handleSelectAll() : handleDeselectAll()}
                              className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-blue-500"
                            />
                          </th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Round</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Win</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Top Miner</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Deployed</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Winnings</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Deploys</th>
                          <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Source</th>
                        </tr>
                      </thead>
                      <tbody>
                        {roundsData.map((round) => (
                          <RoundDataRow
                            key={round.round_id}
                            round={round}
                            selected={selectedRounds.has(round.round_id)}
                            onSelect={handleSelectRound}
                            loading={bulkDeleting}
                          />
                        ))}
                      </tbody>
                    </table>
                  </div>

                  {/* Quick Selection Buttons */}
                  <div className="px-4 py-3 border-t border-slate-700 flex gap-2">
                    <button
                      onClick={handleSelectAll}
                      disabled={bulkDeleting || roundsData.length === 0}
                      className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg disabled:opacity-50"
                    >
                      Select All
                    </button>
                    <button
                      onClick={handleSelectMissing}
                      disabled={bulkDeleting || missingCount === 0}
                      className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg disabled:opacity-50"
                    >
                      Select Missing ({missingCount})
                    </button>
                    <button
                      onClick={handleDeselectAll}
                      disabled={bulkDeleting || selectedRounds.size === 0}
                      className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg disabled:opacity-50"
                    >
                      Deselect All
                    </button>
                  </div>
                  
                  {/* Load More Button */}
                  {hasMore && (
                    <div className="mt-4">
                      <button
                        onClick={loadMoreRoundsData}
                        disabled={loadingMore}
                        className="w-full py-3 bg-slate-700 hover:bg-slate-600 text-white rounded-lg disabled:opacity-50 transition-colors flex items-center justify-center gap-2"
                      >
                        {loadingMore ? (
                          <>
                            <span className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                            Loading...
                          </>
                        ) : (
                          "Load More Rounds"
                        )}
                      </button>
                    </div>
                  )}
                </>
              )}
            </div>
          </>
        )}

        {activeTab === "backfill" && (
          <>
            {/* Backfill Task Status */}
            {backfillTaskState && backfillTaskState.status === "running" && (
              <div className="bg-blue-500/10 rounded-lg border border-blue-500/30 p-6">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-lg font-semibold text-blue-400 flex items-center gap-2">
                    <span className="w-3 h-3 bg-blue-500 rounded-full animate-pulse" />
                    Backfill In Progress
                  </h2>
                  <button
                    onClick={handleCancelBackfill}
                    className="px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition-colors"
                  >
                    Cancel
                  </button>
                </div>
                
                {/* Progress Stats Grid */}
                <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-4">
                  <div className="bg-slate-800/50 rounded-lg p-3">
                    <p className="text-xs text-slate-400">Current Page</p>
                    <p className="text-xl font-bold text-white">
                      {backfillTaskState.current_page.toLocaleString()}
                      <span className="text-sm text-slate-500"> / {backfillTaskState.max_pages.toLocaleString()}</span>
                    </p>
                  </div>
                  <div className="bg-slate-800/50 rounded-lg p-3">
                    <p className="text-xs text-slate-400">Rounds Fetched</p>
                    <p className="text-xl font-bold text-emerald-400">{backfillTaskState.rounds_fetched.toLocaleString()}</p>
                  </div>
                  <div className="bg-slate-800/50 rounded-lg p-3">
                    <p className="text-xs text-slate-400">Rounds Skipped</p>
                    <p className="text-xl font-bold text-slate-300">{backfillTaskState.rounds_skipped.toLocaleString()}</p>
                  </div>
                  <div className="bg-slate-800/50 rounded-lg p-3">
                    <p className="text-xs text-slate-400">Missing Deployments</p>
                    <p className="text-xl font-bold text-yellow-400">{backfillTaskState.rounds_missing_deployments.toLocaleString()}</p>
                  </div>
                </div>
                
                {/* Progress Bar */}
                {backfillTaskState.estimated_total_rounds && backfillTaskState.first_round_id_seen && (
                  <div className="mb-4">
                    <div className="flex justify-between text-xs text-slate-400 mb-1">
                      <span>Round {backfillTaskState.first_round_id_seen?.toLocaleString()} → {backfillTaskState.stop_at_round.toLocaleString()}</span>
                      <span>
                        {backfillTaskState.last_round_id_processed?.toLocaleString() ?? "..."}
                        {backfillTaskState.estimated_total_rounds && (
                          <span className="text-slate-500"> (~{Math.round(((backfillTaskState.first_round_id_seen - (backfillTaskState.last_round_id_processed ?? backfillTaskState.first_round_id_seen)) / backfillTaskState.estimated_total_rounds) * 100)}% done)</span>
                        )}
                      </span>
                    </div>
                    <div className="w-full bg-slate-700 rounded-full h-2">
                      <div 
                        className="bg-blue-500 h-2 rounded-full transition-all duration-500"
                        style={{ 
                          width: `${Math.min(100, Math.round(((backfillTaskState.first_round_id_seen - (backfillTaskState.last_round_id_processed ?? backfillTaskState.first_round_id_seen)) / backfillTaskState.estimated_total_rounds) * 100))}%`
                        }}
                      />
                    </div>
                  </div>
                )}
                
                {/* Time Stats */}
                <div className="flex items-center gap-6 text-sm">
                  <div>
                    <span className="text-slate-400">Elapsed: </span>
                    <span className="text-white font-mono">{formatDuration(backfillTaskState.elapsed_ms)}</span>
                  </div>
                  {backfillTaskState.estimated_remaining_ms && backfillTaskState.estimated_remaining_ms > 0 && (
                    <div>
                      <span className="text-slate-400">ETA: </span>
                      <span className="text-white font-mono">{formatDuration(backfillTaskState.estimated_remaining_ms)}</span>
                    </div>
                  )}
                  <div>
                    <span className="text-slate-400">Last Round: </span>
                    <span className="text-white font-mono">{backfillTaskState.last_round_id_processed?.toLocaleString() ?? "—"}</span>
                  </div>
                </div>
              </div>
            )}

            {/* Completed/Failed/Cancelled Status */}
            {backfillTaskState && backfillTaskState.status !== "running" && backfillTaskState.status !== "idle" && (
              <div className={`rounded-lg border p-4 ${
                backfillTaskState.status === "completed" 
                  ? "bg-green-500/10 border-green-500/30" 
                  : backfillTaskState.status === "cancelled"
                  ? "bg-yellow-500/10 border-yellow-500/30"
                  : "bg-red-500/10 border-red-500/30"
              }`}>
                <div className="flex items-center justify-between">
                  <div>
                    <h3 className={`font-semibold ${
                      backfillTaskState.status === "completed" ? "text-green-400" :
                      backfillTaskState.status === "cancelled" ? "text-yellow-400" : "text-red-400"
                    }`}>
                      Backfill {backfillTaskState.status.charAt(0).toUpperCase() + backfillTaskState.status.slice(1)}
                    </h3>
                    <p className="text-sm text-slate-400 mt-1">
                      {backfillTaskState.rounds_fetched.toLocaleString()} fetched, {backfillTaskState.rounds_skipped.toLocaleString()} skipped, {backfillTaskState.rounds_missing_deployments.toLocaleString()} missing deployments
                      {" • "}{formatDuration(backfillTaskState.elapsed_ms)}
                      {backfillTaskState.error && <span className="text-red-400"> • Error: {backfillTaskState.error}</span>}
                    </p>
                  </div>
                  <button
                    onClick={() => { fetchRoundsData(); fetchStats(); }}
                    className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg"
                  >
                    Refresh Data
                  </button>
                </div>
              </div>
            )}

            {/* Backfill Form - only show if not running */}
            {(!backfillTaskState || backfillTaskState.status !== "running") && (
              <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-6">
                <h2 className="text-lg font-semibold text-white mb-4">Fetch Round Metadata</h2>
                <p className="text-sm text-slate-400 mb-4">
                  Fetch round metadata from the external API. Runs as a background task (1 page/second). Also checks for rounds missing deployments.
                </p>
                <div className="flex flex-wrap gap-4 items-end">
                  <div>
                    <label className="block text-sm text-slate-400 mb-1">Stop at Round</label>
                    <input
                      type="number"
                      value={stopAtRound}
                      onChange={(e) => setStopAtRound(e.target.value)}
                      placeholder="Optional (0)"
                      className="px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white w-32"
                    />
                  </div>
                  <div>
                    <label className="block text-sm text-slate-400 mb-1">Max Pages</label>
                    <input
                      type="number"
                      value={maxPages}
                      onChange={(e) => setMaxPages(e.target.value)}
                      className="px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white w-28"
                    />
                  </div>
                  <button
                    onClick={handleBackfill}
                    disabled={backfillLoading}
                    className={`px-4 py-2 rounded-lg transition-colors ${
                      backfillLoading
                        ? "bg-slate-700 text-slate-400 cursor-not-allowed"
                        : "bg-emerald-500 hover:bg-emerald-600 text-white"
                    }`}
                  >
                    {backfillLoading ? (
                      <span className="flex items-center gap-2">
                        <span className="w-4 h-4 border-2 border-white border-t-transparent rounded-full animate-spin" />
                        Starting...
                      </span>
                    ) : (
                      "Start Backfill"
                    )}
                  </button>
                </div>
              </div>
            )}

            {/* Pending Rounds Table */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
              <div className="px-6 py-4 border-b border-slate-700 flex justify-between items-center">
                <h2 className="text-lg font-semibold text-white">
                  Pending Rounds ({pendingRounds.length})
                </h2>
                <button
                  onClick={fetchPendingRounds}
                  className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                >
                  Refresh
                </button>
              </div>

              {loading ? (
                <div className="flex items-center justify-center h-48">
                  <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : pendingRounds.length === 0 ? (
                <div className="p-8 text-center text-slate-400">
                  No pending rounds. Start a backfill to fetch historical data.
                </div>
              ) : (
                <table className="w-full">
                  <thead>
                    <tr className="border-b border-slate-700">
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Round ID</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Status</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Txns</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Deploys</th>
                      <th className="text-left px-4 py-3 text-sm font-medium text-slate-400">Action</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pendingRounds.map((round) => (
                      <RoundStatusRow
                        key={round.round_id}
                        round={round}
                        onAction={handleAction}
                        loading={actionLoading}
                      />
                    ))}
                  </tbody>
                </table>
              )}
            </div>

            {/* Workflow Legend */}
            <div className="bg-slate-800/30 rounded-lg border border-slate-700/50 p-4">
              <h3 className="text-sm font-medium text-slate-300 mb-3">Workflow Steps</h3>
              <div className="grid grid-cols-1 md:grid-cols-5 gap-4 text-sm">
                <div>
                  <span className="text-emerald-400 font-medium">1. Meta</span>
                  <p className="text-slate-500 text-xs mt-1">Fetch round metadata from external API</p>
                </div>
                <div>
                  <span className="text-emerald-400 font-medium">2. Txns</span>
                  <p className="text-slate-500 text-xs mt-1">Fetch all transactions via Helius</p>
                </div>
                <div>
                  <span className="text-emerald-400 font-medium">3. Rebuild</span>
                  <p className="text-slate-500 text-xs mt-1">Reconstruct deployments from txns</p>
                </div>
                <div>
                  <span className="text-emerald-400 font-medium">4. Verify</span>
                  <p className="text-slate-500 text-xs mt-1">Manually verify against external data</p>
                </div>
                <div>
                  <span className="text-emerald-400 font-medium">5. Final</span>
                  <p className="text-slate-500 text-xs mt-1">Store deployments to ClickHouse</p>
                </div>
              </div>
            </div>
          </>
        )}
      </div>
    </AdminShell>
  );
}
