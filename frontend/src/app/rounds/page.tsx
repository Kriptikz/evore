"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import Link from "next/link";
import { Header } from "@/components/Header";

// Types
interface RoundSummary {
  round_id: number;
  start_slot: number;
  end_slot: number;
  winning_square: number;
  top_miner: string;
  total_deployed: number;
  total_winnings: number;
  unique_miners: number;
  motherlode: number;
  motherlode_hit: boolean;
}

interface DeploymentSummary {
  miner_pubkey: string;
  square_id: number;
  amount: number;
  deployed_slot: number;
  sol_earned: number;
  ore_earned: number;
  is_winner: boolean;
  is_top_miner: boolean;
}

interface RoundDetail {
  round_id: number;
  start_slot: number;
  end_slot: number;
  winning_square: number;
  top_miner: string;
  top_miner_reward: number;
  total_deployed: number;
  total_vaulted: number;
  total_winnings: number;
  unique_miners: number;
  motherlode: number;
  motherlode_hit: boolean;
  source: string;
  deployments: DeploymentSummary[];
}

interface LiveRound {
  round_id: number;
  start_slot: number;
  end_slot: number;
  slots_remaining: number;
  deployed: number[];
  count: number[];
  total_deployed: number;
  unique_miners: number;
}

// API endpoint - use env var or relative URL for same-origin
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";

// Format functions
const formatSol = (lamports: number) => (lamports / 1e9).toFixed(4);
const truncate = (s: string) => s.length > 8 ? `${s.slice(0, 4)}...${s.slice(-4)}` : s;

// Grid colors
const SQUARE_COLORS = [
  "bg-rose-500", "bg-pink-500", "bg-fuchsia-500", "bg-purple-500", "bg-violet-500",
  "bg-indigo-500", "bg-blue-500", "bg-sky-500", "bg-cyan-500", "bg-teal-500",
  "bg-emerald-500", "bg-green-500", "bg-lime-500", "bg-yellow-500", "bg-amber-500",
  "bg-orange-500", "bg-red-500", "bg-rose-600", "bg-pink-600", "bg-fuchsia-600",
  "bg-purple-600", "bg-violet-600", "bg-indigo-600", "bg-blue-600", "bg-sky-600",
];

function SquareGrid({
  deployed,
  counts,
  winningSquare,
  highlightSlot,
  deployments,
}: {
  deployed: number[];
  counts: number[];
  winningSquare?: number;
  highlightSlot?: number;
  deployments?: DeploymentSummary[];
}) {
  // If highlightSlot is set, filter deployments to only show those <= highlightSlot
  const visibleDeployed = useMemo(() => {
    if (!highlightSlot || !deployments) return deployed;
    
    const amounts = new Array(25).fill(0);
    for (const d of deployments) {
      if (d.deployed_slot <= highlightSlot) {
        amounts[d.square_id] += d.amount;
      }
    }
    return amounts;
  }, [deployed, deployments, highlightSlot]);

  const maxDeployed = Math.max(...visibleDeployed, 1);

  return (
    <div className="grid grid-cols-5 gap-1">
      {visibleDeployed.map((amount, idx) => {
        const opacity = Math.min(0.3 + (amount / maxDeployed) * 0.7, 1);
        const isWinner = winningSquare === idx;
        
        return (
          <div
            key={idx}
            className={`relative aspect-square rounded-lg flex flex-col items-center justify-center text-xs font-mono transition-all duration-200 ${
              isWinner 
                ? "ring-2 ring-yellow-400 ring-offset-2 ring-offset-slate-950" 
                : ""
            }`}
            style={{
              backgroundColor: `hsl(${(idx * 15) % 360}, 70%, ${20 + opacity * 40}%)`,
            }}
          >
            <span className="text-white/90 font-bold text-lg">{idx}</span>
            <span className="text-white/70 text-[10px]">
              {formatSol(amount)} SOL
            </span>
            {counts && counts[idx] > 0 && (
              <span className="text-white/50 text-[9px]">
                {counts[idx]} miners
              </span>
            )}
            {isWinner && (
              <span className="absolute -top-1 -right-1 text-yellow-400">⭐</span>
            )}
          </div>
        );
      })}
    </div>
  );
}

function RoundsList({
  rounds,
  selectedRoundId,
  onSelectRound,
  liveRound,
}: {
  rounds: RoundSummary[];
  selectedRoundId: number | null;
  onSelectRound: (id: number) => void;
  liveRound: LiveRound | null;
}) {
  return (
    <div className="space-y-1 pr-2 overflow-y-auto max-h-[calc(100vh-200px)]">
      {/* Live round */}
      {liveRound && (
        <button
          onClick={() => onSelectRound(0)} // 0 = live
          className={`w-full text-left p-3 rounded-lg transition-all ${
            selectedRoundId === 0
              ? "bg-emerald-500/20 border border-emerald-500/50"
              : "bg-slate-800/50 hover:bg-slate-800 border border-transparent"
          }`}
        >
          <div className="flex items-center justify-between">
            <span className="text-emerald-400 font-bold">LIVE</span>
            <span className="text-xs text-slate-400">
              {liveRound.slots_remaining} slots
            </span>
          </div>
          <div className="text-xs text-slate-500 mt-1">
            Round #{liveRound.round_id}
          </div>
        </button>
      )}
      
      {/* Historical rounds */}
      {rounds.map((round) => (
        <button
          key={round.round_id}
          onClick={() => onSelectRound(round.round_id)}
          className={`w-full text-left p-3 rounded-lg transition-all ${
            selectedRoundId === round.round_id
              ? "bg-blue-500/20 border border-blue-500/50"
              : "bg-slate-800/50 hover:bg-slate-800 border border-transparent"
          }`}
        >
          <div className="flex items-center justify-between">
            <span className="font-mono font-bold text-white">
              #{round.round_id}
            </span>
            {round.motherlode_hit && (
              <span className="text-yellow-400 text-xs">💎</span>
            )}
          </div>
          <div className="flex items-center gap-2 text-xs text-slate-400 mt-1">
            <span className="bg-slate-700 px-1.5 py-0.5 rounded">
              ◼ {round.winning_square}
            </span>
            <span>{formatSol(round.total_winnings)} SOL</span>
          </div>
          <div className="text-xs text-slate-500 mt-1">
            {round.unique_miners} miners
          </div>
        </button>
      ))}
    </div>
  );
}

// Group deployments by slot for cleaner display
function DeploymentsGroupedBySlot({
  deployments,
  winningSquare,
  topMiner,
}: {
  deployments: DeploymentSummary[];
  winningSquare?: number;
  topMiner: string;
}) {
  // Group by slot
  const groupedBySlot = useMemo(() => {
    const groups: Map<number, DeploymentSummary[]> = new Map();
    for (const d of deployments) {
      const slot = d.deployed_slot;
      if (!groups.has(slot)) {
        groups.set(slot, []);
      }
      groups.get(slot)!.push(d);
    }
    // Sort by slot descending (most recent first)
    return Array.from(groups.entries()).sort((a, b) => b[0] - a[0]);
  }, [deployments]);

  if (deployments.length === 0) {
    return (
      <div className="bg-slate-800/50 rounded-lg border border-slate-700 p-6 text-center text-slate-400">
        No deployments in this time range
      </div>
    );
  }

  return (
    <div className="bg-slate-800/50 rounded-lg border border-slate-700 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white">
          Deployments ({deployments.length})
        </h3>
        <span className="text-sm text-slate-400">
          {groupedBySlot.length} slot{groupedBySlot.length !== 1 ? 's' : ''}
        </span>
      </div>
      <div className="max-h-[500px] overflow-y-auto">
        {groupedBySlot.slice(0, 50).map(([slot, slotDeployments]) => (
          <div key={slot} className="border-b border-slate-700/50 last:border-0">
            {/* Slot header */}
            <div className="px-4 py-2 bg-slate-900/50 flex justify-between items-center sticky top-0">
              <span className="text-xs font-mono text-slate-400">
                Slot {slot.toLocaleString()}
              </span>
              <span className="text-xs text-slate-500">
                {slotDeployments.length} miner{slotDeployments.length !== 1 ? 's' : ''}
              </span>
            </div>
            {/* Miners in this slot */}
            <div className="divide-y divide-slate-700/30">
              {slotDeployments.map((d, i) => {
                const isWinner = d.square_id === winningSquare;
                const isTopMiner = d.miner_pubkey === topMiner;
                
                return (
                  <div 
                    key={`${d.miner_pubkey}-${d.square_id}-${i}`}
                    className="px-4 py-2 flex items-center justify-between hover:bg-slate-700/20"
                  >
                    <div className="flex items-center gap-3">
                      {/* Miner address */}
                      <span className="font-mono text-sm text-white">
                        {truncate(d.miner_pubkey)}
                      </span>
                      {isTopMiner && <span className="text-yellow-400" title="Top Miner">👑</span>}
                      {isWinner && !isTopMiner && <span className="text-green-400" title="Winner">✓</span>}
                    </div>
                    <div className="flex items-center gap-4">
                      {/* Square */}
                      <span className={`px-2 py-0.5 rounded text-xs ${
                        isWinner ? "bg-yellow-500/20 text-yellow-400" : "bg-slate-700 text-slate-400"
                      }`}>
                        ◼ {d.square_id}
                      </span>
                      {/* Amount */}
                      <span className="font-mono text-sm text-white w-24 text-right">
                        {formatSol(d.amount)}
                      </span>
                      {/* Earned */}
                      {isWinner ? (
                        <span className="text-emerald-400 text-sm w-24 text-right">
                          +{formatSol(d.sol_earned)}
                        </span>
                      ) : (
                        <span className="text-slate-600 text-sm w-24 text-right">—</span>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}

function RoundDetail({
  round,
  liveRound,
  isLive,
}: {
  round: RoundDetail | null;
  liveRound: LiveRound | null;
  isLive: boolean;
}) {
  const [sliderValue, setSliderValue] = useState(100);
  
  // For live rounds, use live data
  const displayRound = isLive ? liveRound : round;

  const deployed = isLive && liveRound
    ? liveRound.deployed 
    : new Array(25).fill(0);
  
  const counts = isLive && liveRound
    ? liveRound.count 
    : new Array(25).fill(0);

  // For historical rounds, calculate deployed from deployments
  const historicalDeployed = useMemo((): { amounts: number[]; counts: number[] } => {
    if (isLive || !round) return { amounts: deployed, counts };
    const amounts = new Array(25).fill(0);
    const minerCounts = new Array(25).fill(0);
    for (const d of round.deployments) {
      amounts[d.square_id] += d.amount;
      minerCounts[d.square_id] += 1;
    }
    return { amounts, counts: minerCounts };
  }, [isLive, round, deployed, counts]);

  // Calculate highlight slot based on slider
  const highlightSlot = useMemo(() => {
    if (isLive || !round) return undefined;
    const range = round.end_slot - round.start_slot;
    return round.start_slot + Math.floor((sliderValue / 100) * range);
  }, [isLive, round, sliderValue]);

  // Get visible deployments (up to slider position)
  const visibleDeployments = useMemo(() => {
    if (isLive || !round) return [];
    return round.deployments.filter(d => 
      !highlightSlot || d.deployed_slot <= highlightSlot
    );
  }, [isLive, round, highlightSlot]);

  // Stats - compute after hooks
  const totalDeployed = useMemo(() => {
    if (!displayRound) return 0;
    return isLive 
      ? (displayRound as LiveRound).total_deployed 
      : visibleDeployments.reduce((sum, d) => sum + d.amount, 0);
  }, [displayRound, isLive, visibleDeployments]);
  
  const uniqueMiners = useMemo(() => {
    if (!displayRound) return 0;
    return isLive 
      ? (displayRound as LiveRound).unique_miners 
      : new Set(visibleDeployments.map(d => d.miner_pubkey)).size;
  }, [displayRound, isLive, visibleDeployments]);

  const totalWinnings = isLive ? 0 : (round?.total_winnings || 0);
  const winningSquare = isLive ? undefined : round?.winning_square;

  // Early return AFTER all hooks
  if (!displayRound) {
    return (
      <div className="flex items-center justify-center h-full text-slate-400">
        Select a round to view details
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-white flex items-center gap-2">
            {isLive ? (
              <>
                <span className="w-3 h-3 bg-emerald-500 rounded-full animate-pulse" />
                Live Round #{(displayRound as LiveRound).round_id}
              </>
            ) : (
              <>Round #{(round as RoundDetail).round_id}</>
            )}
          </h2>
          {!isLive && round && (
            <p className="text-sm text-slate-400 mt-1">
              Source: {round.source} | Slots: {round.start_slot.toLocaleString()} → {round.end_slot.toLocaleString()}
            </p>
          )}
        </div>
        {!isLive && round?.motherlode_hit && (
          <div className="bg-yellow-500/20 text-yellow-400 px-4 py-2 rounded-lg text-sm font-medium">
            💎 Motherlode Hit!
          </div>
        )}
      </div>

      {/* Slider (historical only) */}
      {!isLive && round && (
        <div className="bg-slate-800/50 rounded-lg p-4 border border-slate-700">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm text-slate-400">Round Replay</span>
            <span className="text-sm text-slate-400 font-mono">
              Slot {highlightSlot?.toLocaleString()}
            </span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            value={sliderValue}
            onChange={(e) => setSliderValue(parseInt(e.target.value))}
            className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-blue-500"
          />
          <div className="flex justify-between text-xs text-slate-500 mt-1">
            <span>Start</span>
            <span>End</span>
          </div>
        </div>
      )}

      {/* Grid and Stats */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Square Grid */}
        <div className="bg-slate-800/50 rounded-lg p-6 border border-slate-700">
          <h3 className="text-lg font-semibold text-white mb-4">Deployment Grid</h3>
          <SquareGrid
            deployed={isLive ? deployed : historicalDeployed.amounts}
            counts={isLive ? counts : historicalDeployed.counts}
            winningSquare={winningSquare}
            highlightSlot={highlightSlot}
            deployments={round?.deployments}
          />
        </div>

        {/* Stats */}
        <div className="space-y-4">
          <div className="bg-slate-800/50 rounded-lg p-4 border border-slate-700">
            <h3 className="text-sm text-slate-400 mb-3">Round Statistics</h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <p className="text-2xl font-bold text-white">
                  {formatSol(totalDeployed)} SOL
                </p>
                <p className="text-xs text-slate-500">Total Deployed</p>
              </div>
              <div>
                <p className="text-2xl font-bold text-emerald-400">
                  {uniqueMiners}
                </p>
                <p className="text-xs text-slate-500">Unique Miners</p>
              </div>
              {!isLive && (
                <>
                  <div>
                    <p className="text-2xl font-bold text-yellow-400">
                      {formatSol(totalWinnings)} SOL
                    </p>
                    <p className="text-xs text-slate-500">Total Winnings</p>
                  </div>
                  <div>
                    <p className="text-2xl font-bold text-blue-400">
                      ◼ {winningSquare}
                    </p>
                    <p className="text-xs text-slate-500">Winning Square</p>
                  </div>
                </>
              )}
              {isLive && liveRound && (
                <div>
                  <p className="text-2xl font-bold text-orange-400">
                    {liveRound.slots_remaining}
                  </p>
                  <p className="text-xs text-slate-500">Slots Remaining</p>
                </div>
              )}
            </div>
          </div>

          {/* Top Miner (historical only) */}
          {!isLive && round && (
            <div className="bg-slate-800/50 rounded-lg p-4 border border-slate-700">
              <h3 className="text-sm text-slate-400 mb-2">Top Miner</h3>
              <div className="flex items-center gap-3">
                <div className="w-10 h-10 rounded-full bg-gradient-to-br from-yellow-400 to-orange-500 flex items-center justify-center">
                  👑
                </div>
                <div>
                  <p className="text-white font-mono">{truncate(round.top_miner)}</p>
                  <p className="text-xs text-slate-500">
                    Reward: {formatSol(round.top_miner_reward)} SOL
                  </p>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Deployments Table - Grouped by Slot (historical only) */}
      {!isLive && round && (
        <DeploymentsGroupedBySlot 
          deployments={visibleDeployments} 
          winningSquare={winningSquare}
          topMiner={round.top_miner}
        />
      )}
    </div>
  );
}

export default function RoundsPage() {
  const [rounds, setRounds] = useState<RoundSummary[]>([]);
  const [selectedRoundId, setSelectedRoundId] = useState<number | null>(0); // 0 = live
  const [roundDetail, setRoundDetail] = useState<RoundDetail | null>(null);
  const [liveRound, setLiveRound] = useState<LiveRound | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Fetch rounds list
  const fetchRounds = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/rounds?per_page=50`);
      if (!res.ok) throw new Error("Failed to fetch rounds");
      const data = await res.json();
      setRounds(data.rounds);
    } catch (err) {
      console.error("Failed to fetch rounds:", err);
    }
  }, []);

  // Fetch live round
  const fetchLiveRound = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/round`);
      if (!res.ok) throw new Error("Failed to fetch live round");
      const data = await res.json();
      setLiveRound(data);
    } catch (err) {
      console.error("Failed to fetch live round:", err);
    }
  }, []);

  // Fetch round detail
  const fetchRoundDetail = useCallback(async (roundId: number) => {
    if (roundId === 0) {
      setRoundDetail(null);
      return;
    }
    try {
      setLoading(true);
      const res = await fetch(`${API_BASE}/rounds/${roundId}`);
      if (!res.ok) throw new Error("Failed to fetch round detail");
      const data = await res.json();
      setRoundDetail(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch round");
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load
  useEffect(() => {
    Promise.all([fetchRounds(), fetchLiveRound()]).finally(() => setLoading(false));
  }, [fetchRounds, fetchLiveRound]);

  // Poll live round
  useEffect(() => {
    if (selectedRoundId === 0) {
      const interval = setInterval(fetchLiveRound, 2000);
      return () => clearInterval(interval);
    }
  }, [selectedRoundId, fetchLiveRound]);

  // Fetch detail when selection changes
  useEffect(() => {
    if (selectedRoundId !== null && selectedRoundId !== 0) {
      fetchRoundDetail(selectedRoundId);
    }
  }, [selectedRoundId, fetchRoundDetail]);

  return (
    <div className="min-h-screen bg-slate-950">
      <Header />
      
      <main className="max-w-7xl mx-auto px-4 py-8">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-3xl font-bold text-white">Rounds Explorer</h1>
            <p className="text-slate-400 mt-1">
              View live and historical mining rounds with replay
            </p>
          </div>
          <Link 
            href="/"
            className="text-sm text-slate-400 hover:text-white transition-colors"
          >
            ← Back to AutoMiner
          </Link>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
          {/* Rounds List (Left Sidebar) */}
          <div className="lg:col-span-1 bg-slate-900 rounded-lg border border-slate-800 p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Rounds</h2>
            {loading && rounds.length === 0 ? (
              <div className="flex items-center justify-center h-48">
                <div className="w-8 h-8 border-4 border-blue-500 border-t-transparent rounded-full animate-spin" />
              </div>
            ) : (
              <RoundsList
                rounds={rounds}
                selectedRoundId={selectedRoundId}
                onSelectRound={setSelectedRoundId}
                liveRound={liveRound}
              />
            )}
          </div>

          {/* Round Detail (Main Area) */}
          <div className="lg:col-span-3 bg-slate-900 rounded-lg border border-slate-800 p-6">
            {error && (
              <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 mb-4">
                {error}
              </div>
            )}
            <RoundDetail
              round={roundDetail}
              liveRound={liveRound}
              isLive={selectedRoundId === 0}
            />
          </div>
        </div>
      </main>
    </div>
  );
}

