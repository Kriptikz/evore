"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import Link from "next/link";
import { AdminShell } from "@/components/admin/AdminShell";
import { useAdmin } from "@/context/AdminContext";
import { api, RoundStatus, RoundWithData, RoundStatsResponse, FilterMode, BackfillRoundsTaskState } from "@/lib/api";

// ============================================================================
// Types
// ============================================================================

interface QueuedAction {
  id: number;
  round_id: number;
  action: string;
  status: string;
  queued_at: string;
  started_at: string | null;
  completed_at: string | null;
  error: string | null;
}

interface QueueStatus {
  paused: boolean;
  pending_count: number;
  processing: QueuedAction | null;
  total_processed: number;
  total_failed: number;
  processing_rate: number;
  recent_completed: QueuedAction[];
  recent_failed: QueuedAction[];
}

interface PipelineStats {
  not_in_workflow: number;
  pending_txns: number;
  pending_reconstruct: number;
  pending_verify: number;
  pending_finalize: number;
  complete: number;
}

interface MemoryUsage {
  memory_bytes: number;
  memory_human: string;
  queue_cache_items: number;
}

// ============================================================================
// Helper Components
// ============================================================================

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

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    pending: "bg-yellow-500/20 text-yellow-400",
    processing: "bg-blue-500/20 text-blue-400 animate-pulse",
    completed: "bg-green-500/20 text-green-400",
    failed: "bg-red-500/20 text-red-400",
  };
  return (
    <span className={`px-2 py-1 text-xs rounded-full ${colors[status] || "bg-slate-700 text-slate-400"}`}>
      {status}
    </span>
  );
}

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

const formatSol = (lamports: number) => (lamports / 1e9).toFixed(4);
const truncate = (s: string) => s.length > 12 ? `${s.slice(0, 6)}...${s.slice(-4)}` : s;

// ============================================================================
// Section Components
// ============================================================================

function RoundBackfillingSection({ 
  taskState, 
  onStart, 
  onCancel,
  loading 
}: { 
  taskState: BackfillRoundsTaskState | null; 
  onStart: (stopAt: number, maxPages: number) => void;
  onCancel: () => void;
  loading: boolean;
}) {
  const [stopAtRound, setStopAtRound] = useState("");
  const [maxPages, setMaxPages] = useState("10000");
  
  const isRunning = taskState?.status === "running";

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-white flex items-center gap-2">
          {isRunning && <span className="w-2 h-2 bg-blue-500 rounded-full animate-pulse" />}
          Round Backfilling (External API)
        </h3>
        <StatusBadge status={taskState?.status || "idle"} />
        </div>
      
      {isRunning && taskState ? (
        <div className="space-y-3">
          {/* Current Round */}
          {taskState.last_round_id_processed && (
            <div className="bg-blue-500/10 rounded p-2 text-xs">
              <span className="text-slate-400">Current Round: </span>
              <span className="text-blue-400 font-mono font-bold">{taskState.last_round_id_processed.toLocaleString()}</span>
              {taskState.first_round_id_seen && (
                <span className="text-slate-500 ml-2">
                  (started at {taskState.first_round_id_seen.toLocaleString()})
                </span>
              )}
            </div>
          )}
          
          <div className="grid grid-cols-2 md:grid-cols-4 gap-2 text-xs">
            <div>
              <span className="text-slate-400">Page: </span>
              <span className="text-white">{taskState.current_page.toLocaleString()}</span>
            </div>
            <div>
              <span className="text-slate-400">Fetched: </span>
              <span className="text-green-400">{taskState.rounds_fetched.toLocaleString()}</span>
            </div>
            <div>
              <span className="text-slate-400">Skipped: </span>
              <span className="text-slate-300">{taskState.rounds_skipped.toLocaleString()}</span>
            </div>
            <div>
              <span className="text-slate-400">Missing Deps: </span>
              <span className="text-yellow-400">{taskState.rounds_missing_deployments.toLocaleString()}</span>
            </div>
          </div>
          
          <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs">
            <span className="text-slate-400">Elapsed: <span className="text-white font-mono">{formatDuration(taskState.elapsed_ms)}</span></span>
            {taskState.estimated_remaining_ms && taskState.estimated_remaining_ms > 0 && (
              <span className="text-slate-400">ETA: <span className="text-white font-mono">{formatDuration(taskState.estimated_remaining_ms)}</span></span>
            )}
            {(taskState as any).pages_jumped > 0 && (
              <span className="text-slate-400">Pages Jumped: <span className="text-cyan-400 font-mono">{(taskState as any).pages_jumped.toLocaleString()}</span></span>
            )}
            {taskState.estimated_total_rounds && (
              <span className="text-slate-400">Est. Total: <span className="text-slate-300 font-mono">{taskState.estimated_total_rounds.toLocaleString()}</span></span>
            )}
          </div>
          
            <button
            onClick={onCancel}
            className="w-full px-3 py-1.5 text-sm bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded transition-colors"
          >
            Cancel
          </button>
        </div>
      ) : (
        <div className="space-y-3">
          <div className="flex gap-2">
            <input
              type="number"
              value={stopAtRound}
              onChange={(e) => setStopAtRound(e.target.value)}
              placeholder="Stop at round"
              className="flex-1 px-2 py-1 text-sm bg-slate-900 border border-slate-600 rounded text-white"
            />
            <input
              type="number"
              value={maxPages}
              onChange={(e) => setMaxPages(e.target.value)}
              placeholder="Max pages"
              className="w-24 px-2 py-1 text-sm bg-slate-900 border border-slate-600 rounded text-white"
            />
          </div>
          <button
            onClick={() => onStart(parseInt(stopAtRound) || 0, parseInt(maxPages) || 10000)}
            disabled={loading}
            className="w-full px-3 py-1.5 text-sm bg-emerald-500 hover:bg-emerald-600 text-white rounded transition-colors disabled:opacity-50"
          >
            {loading ? "Starting..." : "Start Backfill"}
          </button>
        </div>
      )}
    </div>
  );
}

function ActionQueueSection({ 
  queueStatus, 
  onPause, 
  onResume, 
  onClear, 
  onRetryFailed 
}: { 
  queueStatus: QueueStatus | null;
  onPause: () => void;
  onResume: () => void;
  onClear: () => void;
  onRetryFailed: () => void;
}) {
  if (!queueStatus) return null;

  const eta = queueStatus.processing_rate > 0 && queueStatus.pending_count > 0
    ? Math.round((queueStatus.pending_count / queueStatus.processing_rate) * 60 * 1000)
    : null;

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
      <div className="flex items-center justify-between mb-3">
        <h3 className="text-sm font-semibold text-white flex items-center gap-2">
          {queueStatus.processing && <span className="w-2 h-2 bg-blue-500 rounded-full animate-pulse" />}
          Action Queue
        </h3>
        <StatusBadge status={queueStatus.paused ? "paused" : queueStatus.processing ? "running" : "idle"} />
      </div>
      
      <div className="space-y-3">
        {queueStatus.processing && (
          <div className="text-xs bg-blue-500/10 rounded p-2">
            <span className="text-slate-400">Processing: </span>
            <span className="text-blue-400">Round {queueStatus.processing.round_id}</span>
            <span className="text-slate-500"> • {queueStatus.processing.action}</span>
          </div>
        )}
        
        <div className="grid grid-cols-2 gap-2 text-xs">
          <div>
            <span className="text-slate-400">Pending: </span>
            <span className="text-yellow-400">{queueStatus.pending_count.toLocaleString()}</span>
          </div>
          <div>
            <span className="text-slate-400">Rate: </span>
            <span className="text-white">{queueStatus.processing_rate.toFixed(1)}/min</span>
          </div>
          <div>
            <span className="text-slate-400">Completed: </span>
            <span className="text-green-400">{queueStatus.total_processed.toLocaleString()}</span>
          </div>
          <div>
            <span className="text-slate-400">Failed: </span>
            <span className="text-red-400">{queueStatus.total_failed.toLocaleString()}</span>
          </div>
        </div>
        
        {eta && (
          <div className="text-xs text-slate-400">
            ETA: <span className="text-white font-mono">{formatDuration(eta)}</span>
          </div>
        )}
        
        <div className="flex gap-2">
          {queueStatus.paused ? (
            <button onClick={onResume} className="flex-1 px-2 py-1 text-xs bg-green-500/20 hover:bg-green-500/30 text-green-400 rounded">
              Resume
            </button>
          ) : (
            <button onClick={onPause} className="flex-1 px-2 py-1 text-xs bg-yellow-500/20 hover:bg-yellow-500/30 text-yellow-400 rounded">
              Pause
            </button>
          )}
          <button onClick={onClear} className="flex-1 px-2 py-1 text-xs bg-slate-600 hover:bg-slate-500 text-white rounded">
            Clear
          </button>
          {queueStatus.total_failed > 0 && (
            <button onClick={onRetryFailed} className="flex-1 px-2 py-1 text-xs bg-red-500/20 hover:bg-red-500/30 text-red-400 rounded">
              Retry ✗
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function PipelineStatusSection({ stats, onStageClick }: { stats: PipelineStats | null; onStageClick: (stage: string) => void }) {
  if (!stats) return null;

  const stages = [
    { key: "not_in_workflow", label: "Not in Workflow", count: stats.not_in_workflow, color: "text-slate-400" },
    { key: "pending_txns", label: "Fetch Txns", count: stats.pending_txns, color: "text-yellow-400" },
    { key: "pending_reconstruct", label: "Rebuild", count: stats.pending_reconstruct, color: "text-orange-400" },
    { key: "pending_verify", label: "Verify", count: stats.pending_verify, color: "text-blue-400" },
    { key: "pending_finalize", label: "Finalize", count: stats.pending_finalize, color: "text-purple-400" },
    { key: "complete", label: "Done", count: stats.complete, color: "text-green-400" },
  ];

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
      <h3 className="text-sm font-semibold text-white mb-3">Pipeline Status</h3>
      <div className="flex items-center gap-1 overflow-x-auto pb-2">
        {stages.map((stage, i) => (
          <div key={stage.key} className="flex items-center">
            <button
              onClick={() => onStageClick(stage.key)}
              className={`px-3 py-2 rounded text-center min-w-[80px] hover:bg-slate-700/50 transition-colors ${
                stage.count > 0 ? 'bg-slate-700/30' : 'bg-slate-800/30'
              }`}
            >
              <div className={`text-lg font-bold ${stage.color}`}>{stage.count.toLocaleString()}</div>
              <div className="text-[10px] text-slate-400">{stage.label}</div>
            </button>
            {i < stages.length - 1 && (
              <span className="text-slate-600 mx-1">→</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

function BulkActionsSection({ 
  onEnqueue, 
  onAddToWorkflow, 
  onBulkVerify,
  loading 
}: { 
  onEnqueue: (start: number, end: number, action: string, skipDone: boolean, onlyInWorkflow: boolean) => void;
  onAddToWorkflow: (start: number, end: number) => void;
  onBulkVerify: (start: number, end: number) => void;
  loading: boolean;
}) {
  const [startRound, setStartRound] = useState("");
  const [endRound, setEndRound] = useState("");
  const [action, setAction] = useState("fetch_txns");
  const [skipDone, setSkipDone] = useState(true);
  const [onlyInWorkflow, setOnlyInWorkflow] = useState(true);

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
      <h3 className="text-sm font-semibold text-white mb-3">Bulk Actions</h3>
      
      <div className="space-y-3">
        <div className="flex gap-2">
          <input
            type="number"
            value={startRound}
            onChange={(e) => setStartRound(e.target.value)}
            placeholder="Start round"
            className="flex-1 px-2 py-1 text-sm bg-slate-900 border border-slate-600 rounded text-white"
          />
          <span className="text-slate-400 self-center">to</span>
          <input
            type="number"
            value={endRound}
            onChange={(e) => setEndRound(e.target.value)}
            placeholder="End round"
            className="flex-1 px-2 py-1 text-sm bg-slate-900 border border-slate-600 rounded text-white"
          />
        </div>
        
        <select
          value={action}
          onChange={(e) => setAction(e.target.value)}
          className="w-full px-2 py-1 text-sm bg-slate-900 border border-slate-600 rounded text-white"
        >
          <option value="fetch_txns">Fetch Transactions</option>
          <option value="reconstruct">Reconstruct</option>
          <option value="finalize">Finalize</option>
        </select>
        
        <div className="flex gap-4 text-xs">
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={skipDone}
              onChange={(e) => setSkipDone(e.target.checked)}
              className="w-3 h-3 rounded border-slate-600 bg-slate-700"
            />
            <span className="text-slate-400">Skip completed</span>
          </label>
          <label className="flex items-center gap-1.5">
            <input
              type="checkbox"
              checked={onlyInWorkflow}
              onChange={(e) => setOnlyInWorkflow(e.target.checked)}
              className="w-3 h-3 rounded border-slate-600 bg-slate-700"
            />
            <span className="text-slate-400">Only in workflow</span>
          </label>
        </div>
        
        <div className="flex gap-2">
              <button
            onClick={() => onEnqueue(parseInt(startRound) || 0, parseInt(endRound) || 0, action, skipDone, onlyInWorkflow)}
            disabled={loading || !startRound || !endRound}
            className="flex-1 px-3 py-1.5 text-sm bg-blue-500 hover:bg-blue-600 text-white rounded disabled:opacity-50 transition-colors"
          >
            Enqueue
              </button>
          <button
            onClick={() => onAddToWorkflow(parseInt(startRound) || 0, parseInt(endRound) || 0)}
            disabled={loading || !startRound || !endRound}
            className="flex-1 px-3 py-1.5 text-sm bg-emerald-500 hover:bg-emerald-600 text-white rounded disabled:opacity-50 transition-colors"
          >
            Add to Workflow
          </button>
          <button
            onClick={() => onBulkVerify(parseInt(startRound) || 0, parseInt(endRound) || 0)}
            disabled={loading || !startRound || !endRound}
            className="flex-1 px-3 py-1.5 text-sm bg-purple-500 hover:bg-purple-600 text-white rounded disabled:opacity-50 transition-colors"
          >
            Bulk Verify
          </button>
        </div>
      </div>
    </div>
  );
}

function AttentionSection({ 
  queueStatus, 
  pipelineStats, 
  onViewFailed,
  onViewPendingVerify 
}: { 
  queueStatus: QueueStatus | null;
  pipelineStats: PipelineStats | null;
  onViewFailed: () => void;
  onViewPendingVerify: () => void;
}) {
  const alerts = [];
  
  if (pipelineStats && pipelineStats.pending_verify > 0) {
    alerts.push({
      type: "warning",
      message: `${pipelineStats.pending_verify} rounds awaiting manual verification`,
      action: onViewPendingVerify,
      actionLabel: "View",
    });
  }
  
  if (queueStatus && queueStatus.total_failed > 0) {
    alerts.push({
      type: "error",
      message: `${queueStatus.total_failed} queue items failed`,
      action: onViewFailed,
      actionLabel: "View / Retry",
    });
  }
  
  if (pipelineStats && pipelineStats.not_in_workflow > 0) {
    alerts.push({
      type: "info",
      message: `${pipelineStats.not_in_workflow} rounds with invalid deployments not in workflow`,
      action: () => {},
      actionLabel: "Add Range",
    });
  }

  if (alerts.length === 0) return null;

  return (
    <div className="bg-slate-800/50 rounded-lg border border-orange-500/30 p-4">
      <h3 className="text-sm font-semibold text-orange-400 mb-3">⚠ Attention</h3>
      <div className="space-y-2">
        {alerts.map((alert, i) => (
          <div key={i} className={`flex items-center justify-between text-sm p-2 rounded ${
            alert.type === "error" ? "bg-red-500/10" :
            alert.type === "warning" ? "bg-yellow-500/10" : "bg-blue-500/10"
          }`}>
            <span className={`${
              alert.type === "error" ? "text-red-400" :
              alert.type === "warning" ? "text-yellow-400" : "text-blue-400"
            }`}>{alert.message}</span>
            <button
              onClick={alert.action}
              className="px-2 py-0.5 text-xs bg-slate-700 hover:bg-slate-600 text-white rounded"
            >
              {alert.actionLabel}
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}

function RoundsDataSection({
  rounds,
  loading,
  filterMode,
  onFilterChange,
  onAddToWorkflow,
  onBulkAddToWorkflow,
  addingRoundId,
  bulkAdding,
  hasMore,
  onLoadMore,
}: {
  rounds: RoundWithData[];
  loading: boolean;
  filterMode: FilterMode;
  onFilterChange: (mode: FilterMode) => void;
  onAddToWorkflow: (roundId: number) => void;
  onBulkAddToWorkflow: (roundIds: number[]) => void;
  addingRoundId: number | null;
  bulkAdding: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
}) {
  const [selectedRounds, setSelectedRounds] = useState<Set<number>>(new Set());
  
  const toggleSelection = (roundId: number) => {
    setSelectedRounds(prev => {
      const next = new Set(prev);
      if (next.has(roundId)) {
        next.delete(roundId);
      } else {
        next.add(roundId);
      }
      return next;
    });
  };
  
  const selectAll = () => {
    // Select all rounds that are missing deployments
    const missingRounds = rounds.filter(r => r.deployment_count === 0).map(r => r.round_id);
    setSelectedRounds(new Set(missingRounds));
  };
  
  const clearSelection = () => {
    setSelectedRounds(new Set());
  };
  
  const handleBulkAdd = () => {
    const roundIds = Array.from(selectedRounds);
    onBulkAddToWorkflow(roundIds);
    setSelectedRounds(new Set());
  };
  
  const missingCount = rounds.filter(r => r.deployment_count === 0).length;
  const selectedCount = selectedRounds.size;

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-700 flex flex-col gap-2">
        <div className="flex items-center justify-between">
          <h3 className="text-sm font-semibold text-white">Rounds Data (ClickHouse)</h3>
          <select
            value={filterMode}
            onChange={(e) => onFilterChange(e.target.value as FilterMode)}
            className="px-2 py-1 text-xs bg-slate-700 border border-slate-600 rounded text-white"
          >
            <option value="all">All</option>
            <option value="missing_deployments">Missing Deployments (0)</option>
            <option value="invalid_deployments">Invalid Deployments (mismatch)</option>
          </select>
        </div>
        
        {/* Bulk Actions Bar */}
        {rounds.length > 0 && (
          <div className="flex items-center gap-2 flex-wrap">
            <div className="flex gap-1">
              <button
                onClick={selectAll}
                className="px-2 py-1 text-xs bg-slate-700 hover:bg-slate-600 text-white rounded"
              >
                Select All Missing ({missingCount})
              </button>
              <button
                onClick={clearSelection}
                disabled={selectedCount === 0}
                className="px-2 py-1 text-xs bg-slate-700 hover:bg-slate-600 text-white rounded disabled:opacity-50"
              >
                Clear
              </button>
            </div>
            
            {selectedCount > 0 && (
              <div className="flex items-center gap-2 ml-2">
                <span className="text-xs text-slate-400">
                  {selectedCount} selected
                </span>
                <button
                  onClick={handleBulkAdd}
                  disabled={bulkAdding}
                  className="px-3 py-1 text-xs bg-emerald-500 hover:bg-emerald-600 text-white rounded disabled:opacity-50 font-medium"
                >
                  {bulkAdding ? 'Adding...' : `Add ${selectedCount} to Workflow`}
                </button>
              </div>
            )}
          </div>
        )}
      </div>
      
      {loading ? (
        <div className="flex items-center justify-center h-32">
          <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
        </div>
      ) : rounds.length === 0 ? (
        <div className="p-6 text-center text-slate-400 text-sm">
          No rounds found
        </div>
      ) : (
        <div className="overflow-x-auto max-h-96">
          <table className="w-full text-sm">
            <thead className="bg-slate-800/50 sticky top-0">
              <tr className="border-b border-slate-700">
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400 w-8">
        <input
          type="checkbox"
                    checked={selectedCount === missingCount && missingCount > 0}
                    onChange={() => selectedCount === missingCount ? clearSelection() : selectAll()}
                    className="rounded border-slate-600 bg-slate-700 text-emerald-500"
                  />
                </th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Round</th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Total Deployed</th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Deployments</th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Sum</th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Diff</th>
                <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Actions</th>
              </tr>
            </thead>
            <tbody>
              {rounds.map((round) => {
                const isMissing = round.deployment_count === 0;
                const isSelected = selectedRounds.has(round.round_id);
                const deploymentSum = round.deployments_sum ?? 0;
                const diff = round.total_deployed - deploymentSum;
                const hasMismatch = round.deployment_count > 0 && diff !== 0;
                
                return (
                  <tr 
                    key={round.round_id} 
                    className={`border-b border-slate-700/50 hover:bg-slate-700/30 ${isSelected ? 'bg-emerald-500/10' : ''} ${hasMismatch ? 'bg-red-500/5' : ''}`}
                  >
                    <td className="px-3 py-2">
                      {isMissing && (
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={() => toggleSelection(round.round_id)}
                          className="rounded border-slate-600 bg-slate-700 text-emerald-500"
                        />
                      )}
      </td>
                    <td className="px-3 py-2 font-mono text-white">{round.round_id.toLocaleString()}</td>
                    <td className="px-3 py-2 text-slate-300">{formatSol(round.total_deployed)} SOL</td>
                    <td className={`px-3 py-2 ${isMissing ? 'text-red-400' : 'text-green-400'}`}>
          {round.deployment_count}
      </td>
                    <td className={`px-3 py-2 ${hasMismatch ? 'text-yellow-400' : 'text-slate-400'}`}>
                      {round.deployment_count > 0 ? formatSol(deploymentSum) : '-'}
                    </td>
                    <td className={`px-3 py-2 font-mono ${hasMismatch ? 'text-red-400' : 'text-slate-500'}`}>
                      {hasMismatch ? formatSol(diff) : '-'}
                    </td>
                    <td className="px-3 py-2">
                      <div className="flex gap-1">
                        {isMissing && (
                          <button
                            onClick={() => onAddToWorkflow(round.round_id)}
                            disabled={addingRoundId === round.round_id || isSelected}
                            className="px-2 py-0.5 text-xs bg-emerald-500 hover:bg-emerald-600 text-white rounded disabled:opacity-50"
                          >
                            {addingRoundId === round.round_id ? '...' : '+WF'}
                          </button>
                        )}
                        <a
                          href={`/admin/transactions?round_id=${round.round_id}`}
                          className="px-2 py-0.5 text-xs bg-purple-500/20 hover:bg-purple-500/30 text-purple-400 rounded"
                        >
                          Txns
                        </a>
                      </div>
      </td>
    </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      
      {/* Load More + Info Footer */}
      <div className="border-t border-slate-700">
        {hasMore && (
          <button
            onClick={onLoadMore}
            disabled={loading}
            className="w-full py-2 text-xs bg-slate-700 hover:bg-slate-600 text-white transition-colors disabled:opacity-50"
          >
            {loading ? 'Loading...' : 'Load More'}
          </button>
        )}
        <div className="px-4 py-2 text-xs text-slate-500">
          Showing {rounds.length} rounds | 
          <span className="text-red-400 ml-1">{missingCount} missing</span> | 
          <span className="text-yellow-400 ml-1">{rounds.filter(r => r.deployment_count > 0 && (r.deployments_sum ?? 0) !== r.total_deployed).length} mismatched</span>
        </div>
      </div>
    </div>
  );
}

function RoundBrowserSection({ 
  rounds, 
  loading,
  hasMore, 
  onLoadMore, 
  onAction,
  actionLoading,
  filterMode,
  onFilterChange 
}: { 
  rounds: RoundStatus[];
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  onAction: (roundId: number, action: string) => void;
  actionLoading: number | null;
  filterMode: string;
  onFilterChange: (mode: string) => void;
}) {
  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
      <div className="px-4 py-3 border-b border-slate-700 flex items-center justify-between">
        <h3 className="text-sm font-semibold text-white">Round Browser</h3>
        <select
          value={filterMode}
          onChange={(e) => onFilterChange(e.target.value)}
          className="px-2 py-1 text-xs bg-slate-700 border border-slate-600 rounded text-white"
        >
          <option value="all">All Pending</option>
          <option value="pending_txns">Pending Txns</option>
          <option value="pending_reconstruct">Pending Rebuild</option>
          <option value="pending_verify">Pending Verify</option>
        </select>
      </div>
      
      {loading ? (
        <div className="flex items-center justify-center h-32">
          <div className="w-6 h-6 border-2 border-blue-500 border-t-transparent rounded-full animate-spin" />
        </div>
      ) : rounds.length === 0 ? (
        <div className="p-6 text-center text-slate-400 text-sm">
          No rounds in workflow. Add some via Bulk Actions.
        </div>
      ) : (
        <>
          <div className="overflow-x-auto max-h-80">
            <table className="w-full text-sm">
              <thead className="bg-slate-800/50 sticky top-0">
                <tr className="border-b border-slate-700">
                  <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Round</th>
                  <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Status</th>
                  <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Txns</th>
                  <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Deploys</th>
                  <th className="text-left px-3 py-2 text-xs font-medium text-slate-400">Actions</th>
                </tr>
              </thead>
              <tbody>
                {rounds.map((round) => {
                  const nextStep = !round.meta_fetched ? "meta" :
                    !round.transactions_fetched ? "txns" :
                    !round.reconstructed ? "reconstruct" :
                    !round.verified ? "verify" :
                    !round.finalized ? "finalize" : null;
                  const isLoading = actionLoading === round.round_id;

  return (
                    <tr key={round.round_id} className="border-b border-slate-700/50 hover:bg-slate-700/30">
                      <td className="px-3 py-2 font-mono text-white">{round.round_id}</td>
                      <td className="px-3 py-2">
                        <div className="flex gap-1">
                          <StepBadge done={round.meta_fetched} label="M" />
                          <StepBadge done={round.transactions_fetched} label="T" />
                          <StepBadge done={round.reconstructed} label="R" />
                          <StepBadge done={round.verified} label="V" />
                          <StepBadge done={round.finalized} label="F" />
                        </div>
      </td>
                      <td className="px-3 py-2 text-slate-400">{round.transaction_count}</td>
                      <td className="px-3 py-2 text-slate-400">{round.deployment_count}</td>
                      <td className="px-3 py-2">
                        <div className="flex gap-1">
                          {nextStep && (
                            <button
                              onClick={() => onAction(round.round_id, nextStep)}
                              disabled={isLoading}
                              className="px-2 py-0.5 text-xs bg-blue-500 hover:bg-blue-600 text-white rounded disabled:opacity-50"
                            >
                              {isLoading ? "..." : nextStep.charAt(0).toUpperCase() + nextStep.slice(1)}
                            </button>
                          )}
                          {round.transactions_fetched && round.transaction_count > 0 && (
                            <Link
                              href={`/admin/transactions?round_id=${round.round_id}`}
                              className="px-2 py-0.5 text-xs bg-purple-500/20 hover:bg-purple-500/30 text-purple-400 rounded"
                            >
                              Txns
                            </Link>
                          )}
                          {!nextStep && <span className="text-green-400 text-xs">✓</span>}
                        </div>
      </td>
    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
          {hasMore && (
            <button
              onClick={onLoadMore}
              className="w-full py-2 text-xs bg-slate-700 hover:bg-slate-600 text-white transition-colors"
            >
              Load More
            </button>
          )}
        </>
      )}
    </div>
  );
}

// ============================================================================
// Main Component
// ============================================================================

export default function BackfillCommandCenter() {
  const { isAuthenticated } = useAdmin();
  
  // State
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  // Data
  const [backfillTaskState, setBackfillTaskState] = useState<BackfillRoundsTaskState | null>(null);
  const [queueStatus, setQueueStatus] = useState<QueueStatus | null>(null);
  const [pipelineStats, setPipelineStats] = useState<PipelineStats | null>(null);
  const [memoryUsage, setMemoryUsage] = useState<MemoryUsage | null>(null);
  const [pendingRounds, setPendingRounds] = useState<RoundStatus[]>([]);
  const [roundsFilterMode, setRoundsFilterMode] = useState("all");
  
  // Rounds Data (ClickHouse)
  const [roundsData, setRoundsData] = useState<RoundWithData[]>([]);
  const [roundsDataFilter, setRoundsDataFilter] = useState<FilterMode>("missing_deployments");
  const [roundsDataLoading, setRoundsDataLoading] = useState(false);
  const [roundsDataPage, setRoundsDataPage] = useState(0);
  const [roundsDataHasMore, setRoundsDataHasMore] = useState(false);
  const [addingRoundId, setAddingRoundId] = useState<number | null>(null);
  const [bulkAdding, setBulkAdding] = useState(false);
  const [roundStats, setRoundStats] = useState<RoundStatsResponse | null>(null);
  
  // Active tab
  const [activeTab, setActiveTab] = useState<"command" | "data">("command");
  
  // Polling refs
  const pollIntervalRef = useRef<NodeJS.Timeout | null>(null);

  // ============================================================================
  // Data Fetching
  // ============================================================================
  
  const fetchQueueStatus = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await fetch("/api/admin/backfill/queue/status", {
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        setQueueStatus(await res.json());
      }
    } catch (err) {
      console.error("Failed to fetch queue status:", err);
    }
  }, [isAuthenticated]);

  const fetchPipelineStats = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await fetch("/api/admin/backfill/pipeline-stats", {
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        setPipelineStats(await res.json());
      }
    } catch (err) {
      console.error("Failed to fetch pipeline stats:", err);
    }
  }, [isAuthenticated]);

  const fetchMemoryUsage = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await fetch("/api/admin/backfill/memory", {
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        setMemoryUsage(await res.json());
      }
    } catch (err) {
      console.error("Failed to fetch memory usage:", err);
    }
  }, [isAuthenticated]);

  const fetchBackfillStatus = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await api.getBackfillRoundsStatus();
      setBackfillTaskState(res);
    } catch (err) {
      console.error("Failed to fetch backfill status:", err);
    }
  }, [isAuthenticated]);

  const fetchPendingRounds = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await api.getPendingRounds();
      setPendingRounds(res.pending);
    } catch (err) {
      console.error("Failed to fetch pending rounds:", err);
    }
  }, [isAuthenticated]);
  
  const fetchRoundsData = useCallback(async (append = false) => {
    if (!isAuthenticated) return;
    setRoundsDataLoading(true);
    try {
      const page = append ? roundsDataPage + 1 : 0;
      const res = await api.getRoundsWithData({ limit: 100, page, filterMode: roundsDataFilter });
      if (append) {
          setRoundsData(prev => [...prev, ...res.rounds]);
        } else {
        setRoundsData(res.rounds);
        }
      setRoundsDataPage(page);
      setRoundsDataHasMore(res.has_more);
    } catch (err) {
      console.error("Failed to fetch rounds data:", err);
    } finally {
      setRoundsDataLoading(false);
    }
  }, [isAuthenticated, roundsDataFilter, roundsDataPage]);

  const fetchRoundStats = useCallback(async () => {
    if (!isAuthenticated) return;
    try {
      const res = await api.getRoundStats();
      setRoundStats(res);
    } catch (err) {
      console.error("Failed to fetch round stats:", err);
    }
  }, [isAuthenticated]);

  const refreshAll = useCallback(async () => {
    await Promise.all([
      fetchQueueStatus(),
      fetchPipelineStats(),
      fetchMemoryUsage(),
      fetchBackfillStatus(),
      fetchPendingRounds(),
    ]);
    setLoading(false);
  }, [fetchQueueStatus, fetchPipelineStats, fetchMemoryUsage, fetchBackfillStatus, fetchPendingRounds]);

  // Initial load and polling
  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    
    refreshAll();
    
    // Poll every 2 seconds
    pollIntervalRef.current = setInterval(refreshAll, 2000);
    
    return () => {
      if (pollIntervalRef.current) {
        clearInterval(pollIntervalRef.current);
      }
    };
  }, [isAuthenticated, refreshAll]);

  // Fetch rounds data and stats when tab or filter changes
  useEffect(() => {
    if (activeTab === "data") {
      setRoundsDataPage(0);
      fetchRoundsData(false);
      fetchRoundStats();
    }
  // Only re-fetch on tab or filter change, not on fetchRoundsData change
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTab, roundsDataFilter]);

  // ============================================================================
  // Action Handlers
  // ============================================================================

  const handleStartBackfill = async (stopAt: number, maxPages: number) => {
    setError(null);
    try {
      await api.backfillRounds(stopAt || undefined, maxPages);
      setMessage("Backfill started");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to start backfill");
    }
  };

  const handleCancelBackfill = async () => {
    try {
      await api.cancelBackfillRounds();
      setMessage("Backfill cancelled");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to cancel backfill");
    }
  };

  const handlePauseQueue = async () => {
    try {
      const res = await fetch("/api/admin/backfill/queue/pause", {
        method: "POST",
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        setMessage("Queue paused");
        fetchQueueStatus();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to pause queue");
    }
  };

  const handleResumeQueue = async () => {
    try {
      const res = await fetch("/api/admin/backfill/queue/resume", {
        method: "POST",
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        setMessage("Queue resumed");
        fetchQueueStatus();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to resume queue");
    }
  };

  const handleClearQueue = async () => {
    try {
      const res = await fetch("/api/admin/backfill/queue/clear", {
        method: "POST",
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        const data = await res.json();
        setMessage(data.message);
        fetchQueueStatus();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to clear queue");
    }
  };

  const handleRetryFailed = async () => {
    try {
      const res = await fetch("/api/admin/backfill/queue/retry-failed", {
        method: "POST",
        headers: { Authorization: `Bearer ${localStorage.getItem("adminToken")}` }
      });
      if (res.ok) {
        const data = await res.json();
        setMessage(data.message);
        fetchQueueStatus();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to retry failed items");
    }
  };

  const handleEnqueue = async (start: number, end: number, action: string, skipDone: boolean, onlyInWorkflow: boolean) => {
    try {
      const res = await fetch("/api/admin/backfill/queue/enqueue", {
        method: "POST",
        headers: { 
          Authorization: `Bearer ${localStorage.getItem("adminToken")}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({
          start_round: start,
          end_round: end,
          action,
          skip_if_done: skipDone,
          only_in_workflow: onlyInWorkflow
        })
      });
      if (res.ok) {
        const data = await res.json();
        setMessage(data.message);
        fetchQueueStatus();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to enqueue");
    }
  };

  const handleAddToWorkflow = async (start: number, end: number) => {
    try {
      const res = await fetch("/api/admin/backfill/add-range", {
        method: "POST",
        headers: { 
          Authorization: `Bearer ${localStorage.getItem("adminToken")}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ start_round: start, end_round: end })
      });
      if (res.ok) {
        const data = await res.json();
        setMessage(data.message);
        fetchPipelineStats();
        fetchPendingRounds();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add to workflow");
    }
  };

  const handleBulkVerify = async (start: number, end: number) => {
    try {
      const res = await fetch("/api/admin/backfill/bulk-verify", {
        method: "POST",
        headers: { 
          Authorization: `Bearer ${localStorage.getItem("adminToken")}`,
          "Content-Type": "application/json"
        },
        body: JSON.stringify({ start_round: start, end_round: end })
      });
      if (res.ok) {
        const data = await res.json();
        setMessage(data.message);
        fetchPipelineStats();
      fetchPendingRounds();
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to bulk verify");
    }
  };

  const handleAddSingleToWorkflow = async (roundId: number) => {
    setAddingRoundId(roundId);
    try {
      await api.addToBackfillWorkflow([roundId]);
      setMessage(`Round ${roundId} added to workflow`);
      fetchRoundsData();
      fetchPipelineStats();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add round to workflow");
    } finally {
      setAddingRoundId(null);
    }
  };

  const handleBulkAddToWorkflow = async (roundIds: number[]) => {
    if (roundIds.length === 0) return;
    setBulkAdding(true);
    try {
      await api.addToBackfillWorkflow(roundIds);
      setMessage(`${roundIds.length} rounds added to workflow`);
      fetchRoundsData();
      fetchPipelineStats();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to add rounds to workflow");
    } finally {
      setBulkAdding(false);
    }
  };

  const handleRoundAction = async (roundId: number, action: string) => {
    setActionLoading(roundId);
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
      }
      fetchPendingRounds();
      fetchPipelineStats();
    } catch (err) {
      setError(err instanceof Error ? err.message : `Action failed for round ${roundId}`);
    } finally {
      setActionLoading(null);
    }
  };

  // ============================================================================
  // Render
  // ============================================================================

  return (
    <AdminShell title="Backfill Command Center" subtitle="Manage historical data backfill workflow">
      <div className="space-y-4">
        {/* Tabs + Header */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <button
              onClick={() => setActiveTab("command")}
              className={`px-4 py-2 text-sm rounded-t transition-colors ${
                activeTab === "command" 
                  ? "bg-slate-700 text-white" 
                  : "bg-slate-800/50 text-slate-400 hover:text-white"
              }`}
            >
              Command Center
            </button>
          <button
            onClick={() => setActiveTab("data")}
              className={`px-4 py-2 text-sm rounded-t transition-colors ${
              activeTab === "data"
                  ? "bg-slate-700 text-white" 
                  : "bg-slate-800/50 text-slate-400 hover:text-white"
              }`}
            >
              Rounds Data
          </button>
          <button
              onClick={refreshAll}
              className="ml-4 px-3 py-1.5 text-sm bg-slate-700 hover:bg-slate-600 text-white rounded transition-colors"
            >
              Refresh
          </button>
          </div>
          {memoryUsage && (
            <div className="text-sm text-slate-400">
              Memory: <span className="text-white font-mono">{memoryUsage.memory_human}</span>
            </div>
          )}
        </div>

        {/* Messages */}
        {message && (
          <div className="p-3 bg-green-500/10 border border-green-500/30 rounded-lg text-green-400 text-sm flex justify-between items-center">
            {message}
            <button onClick={() => setMessage(null)} className="text-green-400/60 hover:text-green-400">✕</button>
          </div>
        )}
        {error && (
          <div className="p-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm flex justify-between items-center">
            {error}
            <button onClick={() => setError(null)} className="text-red-400/60 hover:text-red-400">✕</button>
          </div>
        )}

        {activeTab === "command" ? (
          <>
            {/* Top Row: Round Backfilling + Action Queue */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <RoundBackfillingSection
                taskState={backfillTaskState}
                onStart={handleStartBackfill}
                onCancel={handleCancelBackfill}
                loading={loading}
              />
              <ActionQueueSection
                queueStatus={queueStatus}
                onPause={handlePauseQueue}
                onResume={handleResumeQueue}
                onClear={handleClearQueue}
                onRetryFailed={handleRetryFailed}
              />
              </div>
              
            {/* Pipeline Status */}
            <PipelineStatusSection 
              stats={pipelineStats} 
              onStageClick={(stage) => console.log("Stage clicked:", stage)}
            />

            {/* Bulk Actions */}
            <BulkActionsSection
              onEnqueue={handleEnqueue}
              onAddToWorkflow={handleAddToWorkflow}
              onBulkVerify={handleBulkVerify}
              loading={loading}
            />

            {/* Attention */}
            <AttentionSection
              queueStatus={queueStatus}
              pipelineStats={pipelineStats}
              onViewFailed={() => console.log("View failed")}
              onViewPendingVerify={() => console.log("View pending verify")}
            />

            {/* Round Browser */}
            <RoundBrowserSection
              rounds={pendingRounds}
              loading={loading}
              hasMore={false}
              onLoadMore={() => {}}
              onAction={handleRoundAction}
              actionLoading={actionLoading}
              filterMode={roundsFilterMode}
              onFilterChange={setRoundsFilterMode}
            />

            {/* Workflow Legend */}
            <div className="bg-slate-800/30 rounded-lg border border-slate-700/50 p-4">
              <h3 className="text-xs font-medium text-slate-400 mb-2">Workflow Steps</h3>
              <div className="grid grid-cols-5 gap-2 text-xs text-slate-500">
                <div><span className="text-emerald-400">M</span> = Meta fetched</div>
                <div><span className="text-emerald-400">T</span> = Txns fetched</div>
                <div><span className="text-emerald-400">R</span> = Reconstructed</div>
                <div><span className="text-emerald-400">V</span> = Verified</div>
                <div><span className="text-emerald-400">F</span> = Finalized</div>
              </div>
                </div>
          </>
        ) : (
          <>
            {/* Stats Cards */}
            {roundStats && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-4">
                  <p className="text-xs text-slate-400 mb-1">Total Rounds</p>
                  <p className="text-xl font-bold text-white">{roundStats.total_rounds.toLocaleString()}</p>
                  <p className="text-xs text-slate-500">
                    {roundStats.min_stored_round.toLocaleString()} - {roundStats.max_stored_round.toLocaleString()}
                  </p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-red-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Missing Deployments</p>
                  <p className={`text-xl font-bold ${roundStats.missing_deployments_count > 0 ? 'text-red-400' : 'text-green-400'}`}>
                    {roundStats.missing_deployments_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">rounds with no deployment data</p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-orange-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Invalid Deployments</p>
                  <p className={`text-xl font-bold ${roundStats.invalid_deployments_count > 0 ? 'text-orange-400' : 'text-green-400'}`}>
                    {roundStats.invalid_deployments_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">rounds with mismatched totals</p>
                </div>
                <div className="bg-slate-800/50 rounded-lg border border-yellow-500/30 p-4">
                  <p className="text-xs text-slate-400 mb-1">Missing Rounds</p>
                  <p className={`text-xl font-bold ${roundStats.missing_rounds_count > 0 ? 'text-yellow-400' : 'text-green-400'}`}>
                    {roundStats.missing_rounds_count.toLocaleString()}
                  </p>
                  <p className="text-xs text-slate-500">gaps in round sequence</p>
                </div>
              </div>
            )}

            {/* Rounds Data Table */}
            <RoundsDataSection
              rounds={roundsData}
              loading={roundsDataLoading}
              filterMode={roundsDataFilter}
              onFilterChange={setRoundsDataFilter}
              onAddToWorkflow={handleAddSingleToWorkflow}
              onBulkAddToWorkflow={handleBulkAddToWorkflow}
              addingRoundId={addingRoundId}
              bulkAdding={bulkAdding}
              hasMore={roundsDataHasMore}
              onLoadMore={() => fetchRoundsData(true)}
            />
          </>
        )}
      </div>
    </AdminShell>
  );
}
