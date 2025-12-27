"use client";

import { useEffect, useState, useCallback } from "react";
import { AdminShell } from "@/components/admin/AdminShell";
import { useAdmin } from "@/context/AdminContext";
import { api, RoundStatus } from "@/lib/api";

type WorkflowStep = "meta" | "txns" | "reconstruct" | "verify" | "finalize";

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

export default function BackfillPage() {
  const { isAuthenticated } = useAdmin();
  const [pendingRounds, setPendingRounds] = useState<RoundStatus[]>([]);
  const [loading, setLoading] = useState(true);
  const [actionLoading, setActionLoading] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);

  // Backfill form state
  const [stopAtRound, setStopAtRound] = useState<string>("");
  const [maxPages, setMaxPages] = useState<string>("10");
  const [backfillLoading, setBackfillLoading] = useState(false);

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

  useEffect(() => {
    if (!isAuthenticated) {
      setLoading(false);
      return;
    }
    fetchPendingRounds();
  }, [fetchPendingRounds, isAuthenticated]);

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
        `Backfill complete: ${res.rounds_fetched} fetched, ${res.rounds_skipped} skipped` +
          (res.stopped_at_round ? `, stopped at round ${res.stopped_at_round}` : "")
      );
      fetchPendingRounds();
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
    } catch (err) {
      setError(err instanceof Error ? err.message : `Action failed for round ${roundId}`);
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <AdminShell title="Historical Backfill" subtitle="Reconstruct historical round data">
      <div className="space-y-6">
        {/* Backfill Form */}
        <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-6">
          <h2 className="text-lg font-semibold text-white mb-4">Fetch Round Metadata</h2>
          <p className="text-sm text-slate-400 mb-4">
            Fetch round metadata from the external API and store to ClickHouse.
            Newer rounds are fetched first (pagination from newest to oldest).
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
      </div>
    </AdminShell>
  );
}

