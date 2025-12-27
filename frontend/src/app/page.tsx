"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import Link from "next/link";

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

// API endpoint
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";

// Format functions
const formatSol = (lamports: number) => (lamports / 1e9).toFixed(4);
const truncate = (s: string) => s.length > 12 ? `${s.slice(0, 6)}...${s.slice(-4)}` : s;

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
    <div className="grid grid-cols-5 gap-1.5">
      {visibleDeployed.map((amount, idx) => {
        const opacity = Math.min(0.2 + (amount / maxDeployed) * 0.8, 1);
        const isWinner = winningSquare === idx;
        
        return (
          <div
            key={idx}
            className={`relative aspect-square rounded-lg flex flex-col items-center justify-center text-xs font-mono transition-all duration-200 ${
              isWinner 
                ? "ring-2 ring-amber-400 ring-offset-2 ring-offset-slate-900" 
                : ""
            }`}
            style={{
              backgroundColor: isWinner 
                ? `rgba(245, 158, 11, ${0.3 + opacity * 0.5})` 
                : `rgba(100, 116, 139, ${opacity * 0.4})`,
            }}
          >
            <span className={`font-bold text-lg ${isWinner ? "text-amber-300" : "text-white/90"}`}>
              {idx}
            </span>
            <span className={`text-[10px] ${isWinner ? "text-amber-200/80" : "text-white/60"}`}>
              {formatSol(amount)}
            </span>
            {counts && counts[idx] > 0 && (
              <span className="text-white/40 text-[9px]">
                {counts[idx]} miners
              </span>
            )}
            {isWinner && (
              <span className="absolute -top-1.5 -right-1.5 text-amber-400 text-lg">⭐</span>
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
    <div className="space-y-1.5 overflow-y-auto max-h-[calc(100vh-220px)] pr-1">
      {/* Live round */}
      {liveRound && (
        <button
          onClick={() => onSelectRound(0)}
          className={`w-full text-left p-3 rounded-xl transition-all ${
            selectedRoundId === 0
              ? "bg-emerald-500/20 border border-emerald-500/50"
              : "bg-slate-800/50 hover:bg-slate-700/50 border border-slate-700/50"
          }`}
        >
          <div className="flex items-center justify-between">
            <span className="flex items-center gap-2">
              <span className="w-2 h-2 bg-emerald-500 rounded-full animate-pulse" />
              <span className="text-emerald-400 font-bold">LIVE</span>
            </span>
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
          className={`w-full text-left p-3 rounded-xl transition-all ${
            selectedRoundId === round.round_id
              ? "bg-amber-500/20 border border-amber-500/50"
              : "bg-slate-800/50 hover:bg-slate-700/50 border border-slate-700/50"
          }`}
        >
          <div className="flex items-center justify-between">
            <span className="font-mono font-bold text-white">
              #{round.round_id}
            </span>
            {round.motherlode_hit && (
              <span className="text-amber-400 text-xs">💎</span>
            )}
          </div>
          <div className="flex items-center gap-2 text-xs text-slate-400 mt-1">
            <span className="bg-slate-700/80 px-1.5 py-0.5 rounded">
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

function DeploymentsGroupedBySlot({
  deployments,
  winningSquare,
  topMiner,
}: {
  deployments: DeploymentSummary[];
  winningSquare?: number;
  topMiner: string;
}) {
  const groupedBySlot = useMemo(() => {
    const groups: Map<number, DeploymentSummary[]> = new Map();
    for (const d of deployments) {
      const slot = d.deployed_slot;
      if (!groups.has(slot)) {
        groups.set(slot, []);
      }
      groups.get(slot)!.push(d);
    }
    return Array.from(groups.entries()).sort((a, b) => b[0] - a[0]);
  }, [deployments]);

  if (deployments.length === 0) {
    return (
      <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 p-6 text-center text-slate-400">
        No deployments in this time range
      </div>
    );
  }

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white">
          Deployments ({deployments.length})
        </h3>
        <span className="text-sm text-slate-400">
          {groupedBySlot.length} slot{groupedBySlot.length !== 1 ? 's' : ''}
        </span>
      </div>
      <div className="max-h-[400px] overflow-y-auto">
        {groupedBySlot.slice(0, 50).map(([slot, slotDeployments]) => (
          <div key={slot} className="border-b border-slate-700/30 last:border-0">
            <div className="px-4 py-2 bg-slate-900/50 flex justify-between items-center sticky top-0">
              <span className="text-xs font-mono text-slate-400">
                Slot {slot.toLocaleString()}
              </span>
              <span className="text-xs text-slate-500">
                {slotDeployments.length} miner{slotDeployments.length !== 1 ? 's' : ''}
              </span>
            </div>
            <div className="divide-y divide-slate-700/20">
              {slotDeployments.map((d, i) => {
                const isWinner = d.square_id === winningSquare;
                const isTopMiner = d.miner_pubkey === topMiner;
                
                return (
                  <div 
                    key={`${d.miner_pubkey}-${d.square_id}-${i}`}
                    className="px-4 py-2 flex items-center justify-between hover:bg-slate-700/20"
                  >
                    <div className="flex items-center gap-3">
                      <Link 
                        href={`/miners/${d.miner_pubkey}`}
                        className="font-mono text-sm text-white hover:text-amber-400 transition-colors"
                      >
                        {truncate(d.miner_pubkey)}
                      </Link>
                      {isTopMiner && <span className="text-amber-400" title="Top Miner">👑</span>}
                      {isWinner && !isTopMiner && <span className="text-emerald-400" title="Winner">✓</span>}
                    </div>
                    <div className="flex items-center gap-4">
                      <span className={`px-2 py-0.5 rounded text-xs ${
                        isWinner ? "bg-amber-500/20 text-amber-400" : "bg-slate-700 text-slate-400"
                      }`}>
                        ◼ {d.square_id}
                      </span>
                      <span className="font-mono text-sm text-white w-24 text-right">
                        {formatSol(d.amount)}
                      </span>
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

function RoundDetailView({
  round,
  liveRound,
  isLive,
}: {
  round: RoundDetail | null;
  liveRound: LiveRound | null;
  isLive: boolean;
}) {
  const [sliderValue, setSliderValue] = useState(100);
  
  const displayRound = isLive ? liveRound : round;

  const deployed = isLive && liveRound
    ? liveRound.deployed 
    : new Array(25).fill(0);
  
  const counts = isLive && liveRound
    ? liveRound.count 
    : new Array(25).fill(0);

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

  const highlightSlot = useMemo(() => {
    if (isLive || !round) return undefined;
    const range = round.end_slot - round.start_slot;
    return round.start_slot + Math.floor((sliderValue / 100) * range);
  }, [isLive, round, sliderValue]);

  const visibleDeployments = useMemo(() => {
    if (isLive || !round) return [];
    return round.deployments.filter(d => 
      !highlightSlot || d.deployed_slot <= highlightSlot
    );
  }, [isLive, round, highlightSlot]);

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
          <h2 className="text-2xl font-bold text-white flex items-center gap-3">
            {isLive ? (
              <>
                <span className="w-3 h-3 bg-emerald-500 rounded-full animate-pulse" />
                <span>Live Round #{(displayRound as LiveRound).round_id}</span>
              </>
            ) : (
              <span>Round #{(round as RoundDetail).round_id}</span>
            )}
          </h2>
          {!isLive && round && (
            <p className="text-sm text-slate-400 mt-1">
              Slots: {round.start_slot.toLocaleString()} → {round.end_slot.toLocaleString()}
            </p>
          )}
              </div>
        {!isLive && round?.motherlode_hit && (
          <div className="bg-amber-500/20 text-amber-400 px-4 py-2 rounded-xl text-sm font-medium border border-amber-500/30">
            💎 Motherlode Hit!
              </div>
        )}
            </div>

      {/* Slider (historical only) */}
      {!isLive && round && (
        <div className="bg-slate-800/50 rounded-xl p-4 border border-slate-700/50">
          <div className="flex items-center justify-between mb-2">
            <span className="text-sm text-slate-400">Round Replay</span>
            <span className="text-sm text-amber-400 font-mono">
              Slot {highlightSlot?.toLocaleString()}
            </span>
          </div>
          <input
            type="range"
            min="0"
            max="100"
            value={sliderValue}
            onChange={(e) => setSliderValue(parseInt(e.target.value))}
            className="w-full h-2 bg-slate-700 rounded-lg appearance-none cursor-pointer accent-amber-500"
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
        <div className="bg-slate-800/50 rounded-xl p-6 border border-slate-700/50">
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
          <div className="bg-slate-800/50 rounded-xl p-5 border border-slate-700/50">
            <h3 className="text-sm text-slate-400 mb-4">Round Statistics</h3>
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
                    <p className="text-2xl font-bold text-amber-400">
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
          {!isLive && round && round.top_miner && (
            <div className="bg-slate-800/50 rounded-xl p-5 border border-slate-700/50">
              <h3 className="text-sm text-slate-400 mb-3">Top Miner</h3>
              <div className="flex items-center gap-3">
                <div className="w-12 h-12 rounded-full bg-gradient-to-br from-amber-400 to-orange-500 flex items-center justify-center text-xl">
                  👑
                  </div>
                <div>
                  <Link 
                    href={`/miners/${round.top_miner}`}
                    className="text-white font-mono hover:text-amber-400 transition-colors"
                  >
                    {truncate(round.top_miner)}
                  </Link>
                  <p className="text-xs text-slate-500 mt-0.5">
                    Reward: <span className="text-emerald-400">{formatSol(round.top_miner_reward)} SOL</span>
                  </p>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Deployments Table (historical only) */}
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

export default function HomePage() {
  const [rounds, setRounds] = useState<RoundSummary[]>([]);
  const [selectedRoundId, setSelectedRoundId] = useState<number | null>(0);
  const [roundDetail, setRoundDetail] = useState<RoundDetail | null>(null);
  const [liveRound, setLiveRound] = useState<LiveRound | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

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

  useEffect(() => {
    Promise.all([fetchRounds(), fetchLiveRound()]).finally(() => setLoading(false));
  }, [fetchRounds, fetchLiveRound]);

  useEffect(() => {
    if (selectedRoundId === 0) {
      const interval = setInterval(fetchLiveRound, 2000);
      return () => clearInterval(interval);
    }
  }, [selectedRoundId, fetchLiveRound]);

  useEffect(() => {
    if (selectedRoundId !== null && selectedRoundId !== 0) {
      fetchRoundDetail(selectedRoundId);
    }
  }, [selectedRoundId, fetchRoundDetail]);

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      {/* Header */}
      <header className="border-b border-slate-800/50 bg-slate-900/50 backdrop-blur-sm sticky top-0 z-10">
        <div className="max-w-7xl mx-auto px-4 py-4 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link href="/" className="text-2xl font-bold bg-gradient-to-r from-amber-400 to-orange-500 bg-clip-text text-transparent">
              ORE Stats
            </Link>
          </div>
          <nav className="flex gap-4">
            <Link href="/" className="text-amber-400 font-medium">
              Rounds
            </Link>
            <Link href="/miners" className="text-slate-400 hover:text-white transition-colors">
              Miners
            </Link>
            <Link href="/autominers" className="text-slate-400 hover:text-white transition-colors">
              AutoMiners
            </Link>
          </nav>
        </div>
      </header>

      <main className="max-w-7xl mx-auto px-4 py-8">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-3xl font-bold text-white">Rounds Explorer</h1>
            <p className="text-slate-400 mt-1">
              View live and historical mining rounds with replay
            </p>
              </div>
            </div>

        <div className="grid grid-cols-1 lg:grid-cols-4 gap-6">
          {/* Rounds List (Left Sidebar) */}
          <div className="lg:col-span-1 bg-slate-900/80 rounded-xl border border-slate-800/50 p-4">
            <h2 className="text-lg font-semibold text-white mb-4">Rounds</h2>
            {loading && rounds.length === 0 ? (
              <div className="flex items-center justify-center h-48">
                <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
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
          <div className="lg:col-span-3 bg-slate-900/80 rounded-xl border border-slate-800/50 p-6">
            {error && (
              <div className="p-4 bg-red-500/10 border border-red-500/30 rounded-xl text-red-400 mb-4">
                {error}
          </div>
        )}
            <RoundDetailView
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
