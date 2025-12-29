"use client";

import { useState, useEffect, useCallback, Suspense } from "react";
import Link from "next/link";
import { api, HistoricalRound, CursorResponse } from "@/lib/api";
import { Header } from "@/components/Header";
import { RoundRangeFilter } from "@/components/RoundRangeFilter";
import { useMultiUrlState } from "@/hooks/useUrlState";

const LAMPORTS_PER_SOL = 1_000_000_000;

function formatSol(lamports: number): string {
  const sol = lamports / LAMPORTS_PER_SOL;
  if (Math.abs(sol) >= 1000) {
    return sol.toLocaleString(undefined, { maximumFractionDigits: 1 });
  }
  return sol.toFixed(4);
}

function truncateAddress(addr: string): string {
  if (addr.length <= 12) return addr;
  return addr.slice(0, 6) + "..." + addr.slice(-4);
}

function RoundsContent() {
  const [rounds, setRounds] = useState<HistoricalRound[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [cursor, setCursor] = useState<string | null>(null);
  const [currentRoundId, setCurrentRoundId] = useState<number | undefined>(undefined);

  // URL state for filters
  const [urlState, setUrlState] = useMultiUrlState({
    round_min: undefined as number | undefined,
    round_max: undefined as number | undefined,
    motherlode: false as boolean,
  });

  const roundMin = urlState.round_min;
  const roundMax = urlState.round_max;
  const motherlodeOnly = urlState.motherlode;

  // Fetch current round ID
  useEffect(() => {
    api.getBoard().then(board => {
      setCurrentRoundId(board.round_id);
    }).catch(() => {
      api.getHistoricalRounds({ limit: 1 }).then(response => {
        if (response.data.length > 0) {
          setCurrentRoundId(response.data[0].round_id);
        }
      });
    });
  }, []);

  const fetchRounds = useCallback(async (loadMore = false) => {
    if (!loadMore) {
      setLoading(true);
      setCursor(null);
    }
    setError(null);

    try {
      const data = await api.getHistoricalRounds({
        cursor: loadMore ? cursor ?? undefined : undefined,
        limit: 50,
        roundIdGte: roundMin,
        roundIdLte: roundMax,
        motherlodeHit: motherlodeOnly ? true : undefined,
        order: "desc",
      });

      if (loadMore) {
        setRounds(prev => [...prev, ...data.data]);
      } else {
        setRounds(data.data);
      }
      setCursor(data.cursor);
      setHasMore(data.has_more);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load rounds");
    } finally {
      setLoading(false);
    }
  }, [cursor, roundMin, roundMax, motherlodeOnly]);

  useEffect(() => {
    fetchRounds(false);
  }, [roundMin, roundMax, motherlodeOnly]);

  const handleRoundRangeChange = (min?: number, max?: number) => {
    setUrlState({ round_min: min, round_max: max });
  };

  const handleMotherlodeToggle = () => {
    setUrlState({ motherlode: !motherlodeOnly });
  };

  return (
    <main className="max-w-7xl mx-auto px-4 py-8">
      <h1 className="text-2xl font-bold text-white mb-6">Rounds Explorer</h1>

      {/* Filters */}
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 mb-8 space-y-4">
        <RoundRangeFilter
          roundMin={roundMin}
          roundMax={roundMax}
          currentRoundId={currentRoundId}
          onChange={handleRoundRangeChange}
        />
        
        <div className="flex items-center gap-4">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="checkbox"
              checked={motherlodeOnly}
              onChange={handleMotherlodeToggle}
              className="w-4 h-4 rounded bg-slate-700 border-slate-600 text-amber-500 focus:ring-amber-500/50"
            />
            <span className="text-sm text-slate-300">Motherlode hits only</span>
          </label>
        </div>
      </div>

      {/* Stats Summary */}
      {rounds.length > 0 && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mb-8">
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Rounds Loaded</div>
            <div className="text-2xl font-bold text-white">{rounds.length.toLocaleString()}</div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Motherlode Hits</div>
            <div className="text-2xl font-bold text-amber-400">
              {rounds.filter(r => r.motherlode_hit).length.toLocaleString()}
            </div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Total Winnings</div>
            <div className="text-2xl font-bold text-green-400">
              {formatSol(rounds.reduce((acc, r) => acc + r.total_winnings, 0))} SOL
            </div>
          </div>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-4">
            <div className="text-slate-400 text-sm">Total Deployed</div>
            <div className="text-2xl font-bold text-blue-400">
              {formatSol(rounds.reduce((acc, r) => acc + r.total_deployed, 0))} SOL
            </div>
          </div>
        </div>
      )}

      {/* Loading State */}
      {loading && rounds.length === 0 && (
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      )}

      {/* Error State */}
      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-xl p-6 text-center">
          <div className="text-red-400 mb-2">Error loading rounds</div>
          <div className="text-slate-400">{error}</div>
          <button
            onClick={() => fetchRounds(false)}
            className="mt-4 px-4 py-2 bg-red-500 hover:bg-red-600 text-white rounded-lg transition-colors"
          >
            Retry
          </button>
        </div>
      )}

      {/* Rounds Table */}
      {!loading && !error && rounds.length > 0 && (
        <>
          <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden overflow-x-auto">
            <table className="w-full min-w-[800px]">
              <thead>
                <tr className="border-b border-slate-700/50 bg-slate-800/80">
                  <th className="text-left px-4 py-4 text-sm font-medium text-slate-400">Round</th>
                  <th className="text-center px-4 py-4 text-sm font-medium text-slate-400">Winner</th>
                  <th className="text-left px-4 py-4 text-sm font-medium text-slate-400">Top Miner</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">Deployed</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">Winnings</th>
                  <th className="text-right px-4 py-4 text-sm font-medium text-slate-400">Miners</th>
                  <th className="text-center px-4 py-4 text-sm font-medium text-slate-400">Motherlode</th>
                </tr>
              </thead>
              <tbody>
                {rounds.map((round) => (
                  <tr
                    key={round.round_id}
                    className="border-b border-slate-700/30 hover:bg-slate-700/30 transition-colors"
                  >
                    <td className="px-4 py-3">
                      <Link
                        href={`/?round=${round.round_id}`}
                        className="text-amber-400 hover:text-amber-300 font-medium"
                      >
                        #{round.round_id.toLocaleString()}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-center">
                      <span className="inline-flex items-center justify-center w-8 h-8 bg-slate-700 rounded-lg text-white font-medium">
                        {round.winning_square}
                      </span>
                    </td>
                    <td className="px-4 py-3">
                      <Link
                        href={`/miners/${round.top_miner}`}
                        className="font-mono text-slate-300 hover:text-amber-400 transition-colors text-sm"
                      >
                        {truncateAddress(round.top_miner)}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-blue-400 text-sm">
                      {formatSol(round.total_deployed)} SOL
                    </td>
                    <td className="px-4 py-3 text-right font-mono text-green-400 text-sm">
                      {formatSol(round.total_winnings)} SOL
                    </td>
                    <td className="px-4 py-3 text-right text-slate-400 text-sm">
                      {round.unique_miners}
                    </td>
                    <td className="px-4 py-3 text-center">
                      {round.motherlode_hit ? (
                        <span className="inline-flex items-center gap-1 px-2 py-1 bg-amber-500/20 text-amber-400 rounded-full text-xs font-medium">
                          💎 {formatSol(round.motherlode)} SOL
                        </span>
                      ) : (
                        <span className="text-slate-600">—</span>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {/* Load More */}
          {hasMore && (
            <div className="flex justify-center mt-6">
              <button
                onClick={() => fetchRounds(true)}
                disabled={loading}
                className="px-6 py-3 bg-slate-700 hover:bg-slate-600 text-white rounded-lg transition-colors disabled:opacity-50"
              >
                {loading ? "Loading..." : "Load More Rounds"}
              </button>
            </div>
          )}
        </>
      )}

      {/* Empty State */}
      {!loading && !error && rounds.length === 0 && (
        <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-12 text-center">
          <div className="text-slate-400 text-lg mb-2">No rounds found</div>
          <div className="text-slate-500">Try adjusting your filter criteria</div>
        </div>
      )}
    </main>
  );
}

export default function RoundsPage() {
  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />
      <Suspense fallback={
        <div className="flex items-center justify-center h-64">
          <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
        </div>
      }>
        <RoundsContent />
      </Suspense>
    </div>
  );
}
