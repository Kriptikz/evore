"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { useAdmin } from "@/context/AdminContext";
import { api, RoundStatus, RoundWithData } from "@/lib/api";

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

function RoundStatusRow({
  round,
  onAction,
  loading,
}: {
  round: RoundStatus;
  onAction: (roundId: number, action: WorkflowStep) => void;
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
        {!nextStep && (
          <span className="text-green-400 text-sm">✓ Complete</span>
        )}
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
  const [maxPages, setMaxPages] = useState<string>("10");
  const [backfillLoading, setBackfillLoading] = useState(false);

  // Data viewer state
  const [roundsData, setRoundsData] = useState<RoundWithData[]>([]);
  const [dataLoading, setDataLoading] = useState(false);
  const [showMissingOnly, setShowMissingOnly] = useState(false);
  const [showInvalidOnly, setShowInvalidOnly] = useState(false);
  const [selectedRounds, setSelectedRounds] = useState<Set<number>>(new Set());
  const [bulkDeleting, setBulkDeleting] = useState(false);
  
  // Round ID filter state
  const [startRound, setStartRound] = useState<string>("");
  const [endRound, setEndRound] = useState<string>("");
  
  // Pagination state
  const [hasMore, setHasMore] = useState(false);
  const [nextCursor, setNextCursor] = useState<number | null>(null);
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

  const fetchRoundsData = useCallback(async (reset = true) => {
    if (!isAuthenticated) return;
    setDataLoading(true);
    try {
      const res = await api.getRoundsWithData({
        limit: 100,
        missingDeploymentsOnly: showMissingOnly,
        invalidOnly: showInvalidOnly,
        roundIdGte: startRound ? parseInt(startRound) : undefined,
        roundIdLte: endRound ? parseInt(endRound) : undefined,
      });
      setRoundsData(res.rounds);
      setHasMore(res.has_more);
      setNextCursor(res.next_cursor ?? null);
      setError(null);
      if (reset) {
        setSelectedRounds(new Set());
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch rounds data");
    } finally {
      setDataLoading(false);
    }
  }, [isAuthenticated, showMissingOnly, showInvalidOnly, startRound, endRound]);
  
  const loadMoreRoundsData = useCallback(async () => {
    if (!isAuthenticated || !hasMore || !nextCursor || loadingMore) return;
    setLoadingMore(true);
    try {
      const res = await api.getRoundsWithData({
        limit: 100,
        before: nextCursor,
        missingDeploymentsOnly: showMissingOnly,
        invalidOnly: showInvalidOnly,
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
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load more rounds");
    } finally {
      setLoadingMore(false);
    }
  }, [isAuthenticated, hasMore, nextCursor, loadingMore, showMissingOnly, showInvalidOnly, startRound, endRound]);

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    fetchPendingRounds();
    fetchRoundsData();
  }, [fetchPendingRounds, fetchRoundsData, isAuthenticated]);

  useEffect(() => {
    if (isAuthenticated) {
      fetchRoundsData(true);
    }
  }, [showMissingOnly, showInvalidOnly, fetchRoundsData, isAuthenticated]);

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
      setMessage(
        `Backfill complete: ${res.rounds_fetched} fetched, ${res.rounds_skipped} skipped, ${res.rounds_missing_deployments} missing deployments` +
          (res.stopped_at_round ? `, stopped at round ${res.stopped_at_round}` : "")
      );
      fetchPendingRounds();
      fetchRoundsData();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Backfill failed");
    } finally {
      setBackfillLoading(false);
    }
  };

  const handleAction = async (roundId: number, action: WorkflowStep) => {
    setActionLoading(roundId);
    setMessage(null);
    setError(null);
    try {
      switch (action) {
        case "txns":
          const txRes = await api.fetchRoundTransactions(roundId);
          setMessage(`Round ${roundId}: fetched ${txRes.transactions_fetched} transactions`);
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
              
              {/* Checkbox Filters */}
              <div className="flex items-center gap-4">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={showMissingOnly}
                    onChange={(e) => {
                      setShowMissingOnly(e.target.checked);
                      if (e.target.checked) setShowInvalidOnly(false);
                    }}
                    className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-blue-500"
                  />
                  <span className="text-sm text-slate-300">Missing deployments only</span>
                </label>
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={showInvalidOnly}
                    onChange={(e) => {
                      setShowInvalidOnly(e.target.checked);
                      if (e.target.checked) setShowMissingOnly(false);
                    }}
                    className="w-4 h-4 rounded border-slate-600 bg-slate-700 text-orange-500"
                  />
                  <span className="text-sm text-slate-300">Invalid data only</span>
                </label>
              </div>
              <div className="flex items-center gap-2">
                <span className="text-sm text-slate-400">
                  {roundsData.length} rounds loaded
                  {hasMore && " (more available)"}
                </span>
                <button
                  onClick={() => fetchRoundsData(true)}
                  className="px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors"
                >
                  Refresh
                </button>
              </div>
            </div>

            {/* Rounds Data Table */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
              <div className="px-6 py-4 border-b border-slate-700">
                <h2 className="text-lg font-semibold text-white">
                  Stored Rounds
                  {showMissingOnly && ` (${roundsData.length} missing deployments)`}
                </h2>
              </div>

              {dataLoading ? (
                <div className="flex items-center justify-center h-48">
                  <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
                </div>
              ) : roundsData.length === 0 ? (
                <div className="p-8 text-center text-slate-400">
                  {showMissingOnly ? "All rounds have deployment data!" : "No rounds found. Start a backfill."}
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
                          onClick={() => handleBulkDelete(false, true)}
                          disabled={bulkDeleting}
                          className="px-3 py-1.5 text-sm bg-orange-500 hover:bg-orange-600 text-white rounded-lg disabled:opacity-50"
                        >
                          {bulkDeleting ? "Deleting..." : "Delete Deployments Only"}
                        </button>
                        <button
                          onClick={() => handleBulkDelete(true, true)}
                          disabled={bulkDeleting}
                          className="px-3 py-1.5 text-sm bg-red-500 hover:bg-red-600 text-white rounded-lg disabled:opacity-50"
                        >
                          {bulkDeleting ? "Deleting..." : "Delete All Data"}
                        </button>
                        <button
                          onClick={handleDeselectAll}
                          disabled={bulkDeleting}
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
            {/* Backfill Form */}
            <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-6">
              <h2 className="text-lg font-semibold text-white mb-4">Fetch Round Metadata</h2>
              <p className="text-sm text-slate-400 mb-4">
                Fetch round metadata from the external API. Also checks for rounds missing deployments.
              </p>
              <div className="flex flex-wrap gap-4 items-end">
                <div>
                  <label className="block text-sm text-slate-400 mb-1">Stop at Round</label>
                  <input
                    type="number"
                    value={stopAtRound}
                    onChange={(e) => setStopAtRound(e.target.value)}
                    placeholder="Optional"
                    className="px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white w-32"
                  />
                </div>
                <div>
                  <label className="block text-sm text-slate-400 mb-1">Max Pages</label>
                  <input
                    type="number"
                    value={maxPages}
                    onChange={(e) => setMaxPages(e.target.value)}
                    className="px-3 py-2 bg-slate-900 border border-slate-700 rounded-lg text-white w-24"
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
                      Fetching...
                    </span>
                  ) : (
                    "Start Backfill"
                  )}
                </button>
              </div>
            </div>

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
