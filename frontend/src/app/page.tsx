"use client";

import { useEffect, useState, useCallback, useMemo, Suspense, useRef } from "react";
import Link from "next/link";
import { useSearchParams, useRouter } from "next/navigation";
import { useOreStats, formatSol, formatOre, truncateAddress, type PendingRound, type RoundSummary as ContextRoundSummary } from "@/context/OreStatsContext";
import { Header } from "@/components/Header";

// Types
interface RoundSummary {
  round_id: number;
  start_slot: number;
  end_slot: number;
  winning_square: number;
  top_miner: string;
  total_deployed: number;
  total_vaulted: number;
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

// u64::MAX as a JavaScript number (will be a large number from JSON)
const U64_MAX = 18446744073709551615;
const INTERMISSION_SLOTS = 35;

type RoundStatus = "active" | "intermission" | "awaiting_reset" | "waiting";

// Derive round status from raw data
function getRoundStatus(round: LiveRound, currentSlot?: number): { status: RoundStatus; slotsSinceEnd?: number } {
  // If end_slot is u64::MAX (or very large), waiting for first deployment
  if (round.end_slot > 9999999999999999) {
    return { status: "waiting" };
  }
  
  // Use current_slot if available, otherwise calculate from slots_remaining
  const slot = currentSlot ?? (round.end_slot - round.slots_remaining);
  
  // If current slot is past end_slot, round has ended
  if (slot > round.end_slot) {
    const slotsSinceEnd = slot - round.end_slot;
    
    if (slotsSinceEnd <= INTERMISSION_SLOTS) {
      return { status: "intermission", slotsSinceEnd };
      } else {
      return { status: "awaiting_reset", slotsSinceEnd };
    }
  }
  
  return { status: "active" };
}

// SSE Live Deployment (batched by miner per slot)
interface LiveDeploymentEvent {
  round_id: number;
  miner_pubkey: string;
  amounts: number[];  // Array of 25 amounts, index = square_id
  slot: number;
}

// Aggregated live deployments for display
interface LiveDeploymentDisplay {
  miner_pubkey: string;
  square_id: number;
  amount: number;
  slot: number;
}

// API endpoint
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";

// Format functions
// Format helpers imported from context
const truncate = truncateAddress;

function SquareGrid({
  deployed,
  counts,
  winningSquare,
  highlightSlot,
  deployments,
  highlightedSquares,
  highlightedAmounts,
}: {
  deployed: number[];
  counts: number[];
  winningSquare?: number;
  highlightSlot?: number;
  deployments?: DeploymentSummary[];
  highlightedSquares?: number[];  // Squares to highlight (for selected miner)
  highlightedAmounts?: number[];  // Amounts for highlighted miner on each square
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
        const isHighlighted = highlightedSquares?.includes(idx);
        const highlightAmount = highlightedAmounts?.[idx] || 0;
        
        return (
          <div
            key={idx}
            className={`relative aspect-square rounded-lg flex flex-col items-center justify-center text-xs font-mono transition-all duration-200 ${
              isWinner 
                ? "ring-2 ring-amber-400 ring-offset-2 ring-offset-slate-900" 
                : ""
            } ${
              isHighlighted
                ? "ring-2 ring-cyan-400 ring-offset-1 ring-offset-slate-900"
                : ""
            }`}
            style={{
              backgroundColor: isHighlighted
                ? `rgba(34, 211, 238, 0.4)`
                : isWinner 
                  ? `rgba(245, 158, 11, ${0.3 + opacity * 0.5})` 
                  : `rgba(100, 116, 139, ${opacity * 0.4})`,
            }}
          >
            <span className={`font-bold text-lg ${isHighlighted ? "text-cyan-300" : isWinner ? "text-amber-300" : "text-white/90"}`}>
              {idx + 1}
            </span>
            <span className={`text-[10px] ${isHighlighted ? "text-cyan-200/80" : isWinner ? "text-amber-200/80" : "text-white/60"}`}>
              {isHighlighted ? formatSol(highlightAmount) : formatSol(amount)}
            </span>
            {counts && counts[idx] > 0 && !isHighlighted && (
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

// Aggregated miner data for winners/losers tabs
interface MinerSummary {
  pubkey: string;
  totalDeployed: number;
  squares: { squareId: number; amount: number }[];
  solEarned: number;
  oreEarned: number;
  isTopMiner: boolean;
}

function WinnersTab({
  miners,
  selectedMiner,
  onSelectMiner,
}: {
  miners: MinerSummary[];
  selectedMiner: string | null;
  onSelectMiner: (pubkey: string | null) => void;
}) {
  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white flex items-center gap-2">
          <span className="text-emerald-400">🏆</span>
          Winners ({miners.length})
        </h3>
        <span className="text-sm text-slate-400">
          Click to highlight on grid
        </span>
      </div>
      <div className="max-h-[400px] overflow-y-auto divide-y divide-slate-700/30">
        {miners.length === 0 ? (
          <div className="p-6 text-center text-slate-400">No winners this round</div>
        ) : (
          miners.map((miner) => (
            <div
              key={miner.pubkey}
              onClick={() => onSelectMiner(selectedMiner === miner.pubkey ? null : miner.pubkey)}
              className={`px-4 py-3 cursor-pointer transition-all ${
                selectedMiner === miner.pubkey
                  ? "bg-cyan-500/20 border-l-2 border-cyan-400"
                  : "hover:bg-slate-700/30"
              }`}
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Link
                    href={`/miners/${miner.pubkey}`}
                    onClick={(e) => e.stopPropagation()}
                    className="font-mono text-sm text-white hover:text-amber-400 transition-colors"
                  >
                    {truncate(miner.pubkey)}
                  </Link>
                  {miner.isTopMiner && <span className="text-amber-400" title="Top Miner">👑</span>}
                </div>
                <div className="flex items-center gap-4 text-sm">
                  <span className="text-emerald-400">+{formatSol(miner.solEarned)} SOL</span>
                  {miner.oreEarned > 0 && (
                    <span className="text-purple-400">+{formatOre(miner.oreEarned)} ORE</span>
                  )}
                </div>
              </div>
              <div className="flex items-center gap-2 mt-1 text-xs text-slate-400">
                <span>Deployed: {formatSol(miner.totalDeployed)}</span>
                <span>•</span>
                <span>Squares: {miner.squares.map(s => s.squareId + 1).join(', ')}</span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function LosersTab({
  miners,
  selectedMiner,
  onSelectMiner,
}: {
  miners: MinerSummary[];
  selectedMiner: string | null;
  onSelectMiner: (pubkey: string | null) => void;
}) {
  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white flex items-center gap-2">
          <span className="text-red-400">❌</span>
          Losers ({miners.length})
        </h3>
        <span className="text-sm text-slate-400">
          Click to highlight on grid
        </span>
      </div>
      <div className="max-h-[400px] overflow-y-auto divide-y divide-slate-700/30">
        {miners.length === 0 ? (
          <div className="p-6 text-center text-slate-400">No losers this round</div>
        ) : (
          miners.map((miner) => (
            <div
              key={miner.pubkey}
              onClick={() => onSelectMiner(selectedMiner === miner.pubkey ? null : miner.pubkey)}
              className={`px-4 py-3 cursor-pointer transition-all ${
                selectedMiner === miner.pubkey
                  ? "bg-cyan-500/20 border-l-2 border-cyan-400"
                  : "hover:bg-slate-700/30"
              }`}
            >
              <div className="flex items-center justify-between">
                <Link
                  href={`/miners/${miner.pubkey}`}
                  onClick={(e) => e.stopPropagation()}
                  className="font-mono text-sm text-white hover:text-amber-400 transition-colors"
                >
                  {truncate(miner.pubkey)}
                </Link>
                <span className="text-red-400 text-sm">-{formatSol(miner.totalDeployed)} SOL</span>
              </div>
              <div className="flex items-center gap-2 mt-1 text-xs text-slate-400">
                <span>Deployed on squares: {miner.squares.map(s => s.squareId + 1).join(', ')}</span>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

function RoundsList({
  rounds,
  pendingRounds,
  selectedRoundId,
  onSelectRound,
  liveRound,
  currentSlot,
  hasMore,
  loadingMore,
  onLoadMore,
}: {
  rounds: RoundSummary[] | ContextRoundSummary[];
  pendingRounds: PendingRound[];
  selectedRoundId: number | null;
  onSelectRound: (id: number) => void;
  liveRound: LiveRound | null;
  currentSlot: number;
  hasMore: boolean;
  loadingMore: boolean;
  onLoadMore: () => void;
}) {
  // Use context's phase detection
  const { phase, slotsRemaining, slotsSinceEnd } = useOreStats();
  
  return (
    <div className="space-y-1.5 overflow-y-auto max-h-[calc(100vh-280px)] pr-1">
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
              {phase === "active" && (
                <>
                  <span className="w-2 h-2 bg-emerald-500 rounded-full animate-pulse" />
                  <span className="text-emerald-400 font-bold">LIVE</span>
                </>
              )}
              {phase === "intermission" && (
                <>
                  <span className="w-2 h-2 bg-blue-500 rounded-full animate-pulse" />
                  <span className="text-blue-400 font-bold">INTERMISSION</span>
                </>
              )}
              {phase === "awaiting_reset" && (
                <>
                  <span className="w-2 h-2 bg-orange-500 rounded-full animate-pulse" />
                  <span className="text-orange-400 font-bold">RESETTING</span>
                </>
              )}
              {phase === "waiting" && (
                <>
                  <span className="w-2 h-2 bg-yellow-500 rounded-full animate-pulse" />
                  <span className="text-yellow-400 font-bold">STARTING</span>
                </>
              )}
            </span>
            <span className="text-xs text-slate-400">
              {phase === "active" && `${slotsRemaining} slots`}
              {phase === "intermission" && `~${35 - slotsSinceEnd}s`}
              {phase === "awaiting_reset" && "pending"}
              {phase === "waiting" && "ready"}
            </span>
          </div>
          <div className="text-xs text-slate-500 mt-1">
            Round #{liveRound.round_id}
          </div>
        </button>
      )}
      
      {/* Pending/Finalizing rounds */}
      {pendingRounds.map((round) => (
        <div
          key={`pending-${round.round_id}`}
          className="w-full text-left p-3 rounded-xl bg-purple-500/10 border border-purple-500/30"
        >
          <div className="flex items-center justify-between">
            <span className="flex items-center gap-2">
              <span className="w-2 h-2 bg-purple-500 rounded-full animate-pulse" />
              <span className="text-purple-400 font-bold">FINALIZING</span>
            </span>
            <span className="text-xs text-purple-400/70">
              {round.unique_miners} miners
            </span>
          </div>
          <div className="text-xs text-slate-500 mt-1">
            Round #{round.round_id} • {formatSol(round.total_deployed)} deployed
          </div>
        </div>
      ))}
      
      {/* Historical rounds */}
      {rounds.map((round) => (
        <button
          key={round.round_id}
          onClick={() => onSelectRound(round.round_id)}
          className={`w-full text-left p-3 rounded-xl transition-all ${
            selectedRoundId === round.round_id
              ? "bg-amber-500/20 border border-amber-500/50"
              : round.motherlode > 0
                ? "bg-purple-900/30 hover:bg-purple-800/40 border border-purple-500/30"
                : "bg-slate-800/50 hover:bg-slate-700/50 border border-slate-700/50"
          }`}
        >
          <div className="flex items-center justify-between">
            <span className="font-mono font-bold text-white">
              #{round.round_id}
            </span>
            <div className="flex items-center gap-1.5">
              {round.motherlode > 0 && (
                <span className="text-purple-400 text-xs">💎 {formatSol(round.motherlode)}</span>
              )}
              {round.motherlode_hit && !round.motherlode && (
                <span className="text-amber-400 text-xs">💎</span>
              )}
            </div>
          </div>
          <div className="flex items-center gap-2 text-xs text-slate-400 mt-1">
            <span className="bg-slate-700/80 px-1.5 py-0.5 rounded">
              ◼ {round.winning_square + 1}
            </span>
            <span className="text-white/70">{formatSol(round.total_deployed)} deployed</span>
            <span className="text-purple-400">{formatSol(round.total_vaulted || 0)} vaulted</span>
          </div>
          <div className="flex items-center gap-2 text-xs text-slate-500 mt-1">
            <span>{round.unique_miners} miners</span>
            <span>•</span>
            <span className="font-mono">👑 {truncateAddress(round.top_miner)}</span>
          </div>
        </button>
      ))}
      
      {/* Load More Button */}
      {hasMore && (
        <button
          onClick={onLoadMore}
          disabled={loadingMore}
          className="w-full p-3 rounded-xl bg-slate-800/30 hover:bg-slate-700/50 border border-slate-700/50 transition-all text-center text-sm text-slate-400 hover:text-white disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loadingMore ? (
            <span className="flex items-center justify-center gap-2">
              <span className="w-4 h-4 border-2 border-amber-500 border-t-transparent rounded-full animate-spin" />
              Loading...
            </span>
          ) : (
            "Load More Rounds"
          )}
        </button>
      )}
    </div>
  );
}

// Grouped deployment by miner at a slot
interface MinerDeploymentGroup {
  miner_pubkey: string;
  slot: number;
  amounts: number[];  // 25 squares
  total_amount: number;
  sol_earned: number;
  ore_earned: number;
  deployed_on_winning: boolean;
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
  // Group by slot, then by miner within each slot
  const groupedBySlotAndMiner = useMemo(() => {
    // First group by slot
    const slotGroups: Map<number, Map<string, MinerDeploymentGroup>> = new Map();
    
    for (const d of deployments) {
      const slot = d.deployed_slot;
      if (!slotGroups.has(slot)) {
        slotGroups.set(slot, new Map());
      }
      
      const minerGroups = slotGroups.get(slot)!;
      if (!minerGroups.has(d.miner_pubkey)) {
        minerGroups.set(d.miner_pubkey, {
          miner_pubkey: d.miner_pubkey,
          slot,
          amounts: new Array(25).fill(0),
          total_amount: 0,
          sol_earned: 0,
          ore_earned: 0,
          deployed_on_winning: false,
        });
      }
      
      const group = minerGroups.get(d.miner_pubkey)!;
      group.amounts[d.square_id] += d.amount;
      group.total_amount += d.amount;
      group.sol_earned += d.sol_earned;
      group.ore_earned += d.ore_earned;
      if (d.square_id === winningSquare) {
        group.deployed_on_winning = true;
      }
    }
    
    // Convert to sorted array: [(slot, [minerGroups])]
    return Array.from(slotGroups.entries())
      .sort((a, b) => b[0] - a[0])  // Sort slots descending
      .map(([slot, minerMap]) => ({
        slot,
        miners: Array.from(minerMap.values()),
      }));
  }, [deployments, winningSquare]);

  const uniqueMiners = useMemo(() => {
    return new Set(deployments.map(d => d.miner_pubkey)).size;
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
          Deployments ({uniqueMiners} miners)
        </h3>
        <span className="text-sm text-slate-400">
          {groupedBySlotAndMiner.length} slot{groupedBySlotAndMiner.length !== 1 ? 's' : ''}
        </span>
            </div>
      <div className="max-h-[400px] overflow-y-auto">
        {groupedBySlotAndMiner.slice(0, 50).map(({ slot, miners }) => (
          <div key={slot} className="border-b border-slate-700/30 last:border-0">
            <div className="px-4 py-2 bg-slate-900/50 flex justify-between items-center sticky top-0">
              <span className="text-xs font-mono text-slate-400">
                Slot {slot.toLocaleString()}
              </span>
              <span className="text-xs text-slate-500">
                {miners.length} miner{miners.length !== 1 ? 's' : ''}
              </span>
          </div>
            <div className="divide-y divide-slate-700/20">
              {miners.map((group) => {
                const isTopMiner = group.miner_pubkey === topMiner;
                const deployedSquares = group.amounts
                  .map((amt, idx) => amt > 0 ? idx : -1)
                  .filter(idx => idx >= 0);
                
                return (
                  <div 
                    key={`${group.miner_pubkey}-${slot}`}
                    className="px-4 py-3 hover:bg-slate-700/20"
                  >
                    <div className="flex items-center justify-between">
                      <div className="flex items-center gap-3">
                        <Link 
                          href={`/miners/${group.miner_pubkey}`}
                          className="font-mono text-sm text-white hover:text-amber-400 transition-colors"
                        >
                          {truncate(group.miner_pubkey)}
                        </Link>
                        {isTopMiner && <span className="text-amber-400" title="Top Miner">👑</span>}
                        {group.deployed_on_winning && !isTopMiner && (
                          <span className="text-emerald-400" title="Winner">✓</span>
                        )}
                      </div>
                      <div className="flex items-center gap-4">
                        <span className="font-mono text-sm text-white w-24 text-right">
                          {formatSol(group.total_amount)}
                        </span>
                        {group.sol_earned > 0 ? (
                          <span className="text-emerald-400 text-sm w-24 text-right">
                            +{formatSol(group.sol_earned)}
                          </span>
                        ) : (
                          <span className="text-slate-600 text-sm w-24 text-right">—</span>
                        )}
              </div>
              </div>
                    {/* Square breakdown */}
                    <div className="flex flex-wrap gap-1.5 mt-2">
                      {deployedSquares.map((squareId) => {
                        const isWinning = squareId === winningSquare;
                        return (
                          <span 
                            key={squareId}
                            className={`px-2 py-0.5 rounded text-xs ${
                              isWinning 
                                ? "bg-amber-500/20 text-amber-400 ring-1 ring-amber-500/50" 
                                : "bg-slate-700 text-slate-400"
                            }`}
                            title={`${formatSol(group.amounts[squareId])} SOL`}
                          >
                            ◼{squareId + 1}: {formatSol(group.amounts[squareId])}
                          </span>
                        );
                      })}
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

// Grouped live deployment by miner at a slot
interface LiveMinerGroup {
  miner_pubkey: string;
  slot: number;
  amounts: number[];  // 25 squares
  total_amount: number;
}

function LiveDeploymentsTable({ deployments }: { deployments: LiveDeploymentDisplay[] }) {
  // Group by slot, then by miner within each slot
  const groupedBySlotAndMiner = useMemo(() => {
    const slotGroups: Map<number, Map<string, LiveMinerGroup>> = new Map();
    
    for (const d of deployments) {
      if (!slotGroups.has(d.slot)) {
        slotGroups.set(d.slot, new Map());
      }
      
      const minerGroups = slotGroups.get(d.slot)!;
      if (!minerGroups.has(d.miner_pubkey)) {
        minerGroups.set(d.miner_pubkey, {
          miner_pubkey: d.miner_pubkey,
          slot: d.slot,
          amounts: new Array(25).fill(0),
          total_amount: 0,
        });
      }
      
      const group = minerGroups.get(d.miner_pubkey)!;
      group.amounts[d.square_id] += d.amount;
      group.total_amount += d.amount;
    }
    
    // Convert to sorted array
    return Array.from(slotGroups.entries())
      .sort((a, b) => b[0] - a[0])  // Sort slots descending
      .map(([slot, minerMap]) => ({
        slot,
        miners: Array.from(minerMap.values()),
      }));
  }, [deployments]);

  const uniqueMiners = useMemo(() => {
    return new Set(deployments.map(d => d.miner_pubkey)).size;
  }, [deployments]);

  return (
    <div className="bg-slate-800/50 rounded-xl border border-slate-700/50 overflow-hidden">
      <div className="px-6 py-4 border-b border-slate-700/50 flex justify-between items-center">
        <h3 className="text-lg font-semibold text-white flex items-center gap-2">
          <span className="w-2 h-2 bg-emerald-500 rounded-full animate-pulse" />
          Live Deployments ({uniqueMiners} miners)
        </h3>
        <span className="text-sm text-slate-400">
          {groupedBySlotAndMiner.length} slot{groupedBySlotAndMiner.length !== 1 ? 's' : ''}
        </span>
              </div>
      <div className="max-h-[400px] overflow-y-auto">
        {groupedBySlotAndMiner.slice(0, 50).map(({ slot, miners }) => (
          <div key={slot} className="border-b border-slate-700/30 last:border-0">
            <div className="px-4 py-2 bg-slate-900/50 flex justify-between items-center sticky top-0">
              <span className="text-xs font-mono text-slate-400">
                Slot {slot.toLocaleString()}
              </span>
              <span className="text-xs text-emerald-400">
                {miners.length} miner{miners.length !== 1 ? 's' : ''}
              </span>
            </div>
            <div className="divide-y divide-slate-700/20">
              {miners.map((group) => {
                const deployedSquares = group.amounts
                  .map((amt, idx) => amt > 0 ? idx : -1)
                  .filter(idx => idx >= 0);
                
                return (
                  <div 
                    key={`${group.miner_pubkey}-${slot}`}
                    className="px-4 py-3 hover:bg-slate-700/20"
                  >
              <div className="flex items-center justify-between">
                <Link 
                        href={`/miners/${group.miner_pubkey}`}
                        className="font-mono text-sm text-white hover:text-amber-400 transition-colors"
                >
                        {truncate(group.miner_pubkey)}
                </Link>
                      <span className="font-mono text-sm text-white">
                        {formatSol(group.total_amount)} SOL
                      </span>
              </div>
                    {/* Square breakdown */}
                    <div className="flex flex-wrap gap-1.5 mt-2">
                      {deployedSquares.map((squareId) => (
                        <span 
                          key={squareId}
                          className="px-2 py-0.5 rounded text-xs bg-slate-700 text-slate-400"
                          title={`${formatSol(group.amounts[squareId])} SOL`}
                        >
                          ◼{squareId + 1}: {formatSol(group.amounts[squareId])}
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
        {deployments.length === 0 && (
          <div className="p-6 text-center text-slate-400">
            Waiting for deployments...
          </div>
        )}
              </div>
              </div>
  );
}

type DetailTab = "deployments" | "winners" | "losers";

function RoundDetailView({
  round,
  liveRound,
  isLive,
  liveDeployments,
  currentSlot,
}: {
  round: RoundDetail | null;
  liveRound: LiveRound | null;
  isLive: boolean;
  liveDeployments: LiveDeploymentDisplay[];
  currentSlot: number;
}) {
  const [sliderValue, setSliderValue] = useState(100);
  const [activeTab, setActiveTab] = useState<DetailTab>("deployments");
  const [selectedMiner, setSelectedMiner] = useState<string | null>(null);
  
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
  const totalVaulted = isLive ? 0 : (round?.total_vaulted || 0);
  const winningSquare = isLive ? undefined : round?.winning_square;

  // Aggregate miners into winners and losers
  const { winners, losers } = useMemo((): { winners: MinerSummary[]; losers: MinerSummary[] } => {
    if (isLive || !round) return { winners: [], losers: [] };
    
    const minerMap = new Map<string, MinerSummary>();
    
    for (const d of visibleDeployments) {
      const existing = minerMap.get(d.miner_pubkey);
      if (existing) {
        existing.totalDeployed += d.amount;
        existing.solEarned += d.sol_earned;
        existing.oreEarned += d.ore_earned;
        if (!existing.squares.find(s => s.squareId === d.square_id)) {
          existing.squares.push({ squareId: d.square_id, amount: d.amount });
        } else {
          const sq = existing.squares.find(s => s.squareId === d.square_id);
          if (sq) sq.amount += d.amount;
        }
        if (d.is_top_miner) existing.isTopMiner = true;
      } else {
        minerMap.set(d.miner_pubkey, {
          pubkey: d.miner_pubkey,
          totalDeployed: d.amount,
          squares: [{ squareId: d.square_id, amount: d.amount }],
          solEarned: d.sol_earned,
          oreEarned: d.ore_earned,
          isTopMiner: d.is_top_miner,
        });
      }
    }
    
    const allMiners = Array.from(minerMap.values());
    const winnersList = allMiners.filter(m => m.squares.some(s => s.squareId === winningSquare));
    const losersList = allMiners.filter(m => !m.squares.some(s => s.squareId === winningSquare));
    
    // Sort by total earned (winners) or total lost (losers)
    winnersList.sort((a, b) => (b.solEarned + b.oreEarned) - (a.solEarned + a.oreEarned));
    losersList.sort((a, b) => b.totalDeployed - a.totalDeployed);
    
    return { winners: winnersList, losers: losersList };
  }, [isLive, round, visibleDeployments, winningSquare]);

  // Get highlighted squares and amounts for selected miner
  const { highlightedSquares, highlightedAmounts } = useMemo(() => {
    if (!selectedMiner) return { highlightedSquares: undefined, highlightedAmounts: undefined };
    
    const allMiners = [...winners, ...losers];
    const miner = allMiners.find(m => m.pubkey === selectedMiner);
    if (!miner) return { highlightedSquares: undefined, highlightedAmounts: undefined };
    
    const squares = miner.squares.map(s => s.squareId);
    const amounts = new Array(25).fill(0);
    miner.squares.forEach(s => {
      amounts[s.squareId] = s.amount;
    });
    
    return { highlightedSquares: squares, highlightedAmounts: amounts };
  }, [selectedMiner, winners, losers]);

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
            {isLive && liveRound ? (
              (() => {
                const { status } = getRoundStatus(liveRound, currentSlot);
                return (
                  <>
                    {status === "active" && (
                      <span className="w-3 h-3 bg-emerald-500 rounded-full animate-pulse" />
                    )}
                    {status === "intermission" && (
                      <span className="w-3 h-3 bg-blue-500 rounded-full animate-pulse" />
                    )}
                    {status === "awaiting_reset" && (
                      <span className="w-3 h-3 bg-orange-500 rounded-full animate-pulse" />
                    )}
                    {status === "waiting" && (
                      <span className="w-3 h-3 bg-yellow-500 rounded-full animate-pulse" />
                    )}
                    <span>Live Round #{liveRound.round_id}</span>
                    {status === "intermission" && (
                      <span className="text-sm font-normal text-blue-400 ml-2">
                        (Intermission)
                      </span>
                    )}
                    {status === "awaiting_reset" && (
                      <span className="text-sm font-normal text-orange-400 ml-2">
                        (Awaiting Reset)
                      </span>
                    )}
                    {status === "waiting" && (
                      <span className="text-sm font-normal text-yellow-400 ml-2">
                        (Waiting for first deployment)
                      </span>
                    )}
                  </>
                );
              })()
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
          <div className="flex items-center justify-between mb-4">
            <h3 className="text-lg font-semibold text-white">Deployment Grid</h3>
            {selectedMiner && (
                  <button
                onClick={() => setSelectedMiner(null)}
                className="text-xs text-cyan-400 hover:text-cyan-300 transition-colors"
              >
                Clear selection
                  </button>
            )}
          </div>
          <SquareGrid
            deployed={isLive ? deployed : historicalDeployed.amounts}
            counts={isLive ? counts : historicalDeployed.counts}
            winningSquare={winningSquare}
            highlightSlot={highlightSlot}
            deployments={round?.deployments}
            highlightedSquares={highlightedSquares}
            highlightedAmounts={highlightedAmounts}
          />
          {selectedMiner && (
            <div className="mt-3 p-2 bg-cyan-500/10 rounded-lg border border-cyan-500/30 text-xs text-cyan-300">
              Showing: {truncate(selectedMiner)}
            </div>
          )}
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
                    <p className="text-2xl font-bold text-purple-400">
                      {formatSol(totalVaulted)} SOL
                    </p>
                    <p className="text-xs text-slate-500">Total Vaulted</p>
                  </div>
                  <div>
                    <p className="text-2xl font-bold text-blue-400">
                      ◼ {winningSquare !== undefined ? winningSquare + 1 : '-'}
                    </p>
                    <p className="text-xs text-slate-500">Winning Square</p>
                  </div>
                  <div>
                    <p className="text-lg font-bold text-emerald-400">
                      {winners.length}
                    </p>
                    <p className="text-xs text-slate-500">Winners</p>
                  </div>
                </>
              )}
              {isLive && liveRound && (
                (() => {
                  const { status, slotsSinceEnd } = getRoundStatus(liveRound, currentSlot);
                  return (
                    <div>
                      {status === "active" && (
                        <>
                          <p className="text-2xl font-bold text-orange-400">
                            {liveRound.slots_remaining}
                          </p>
                          <p className="text-xs text-slate-500">Slots Remaining</p>
                        </>
                      )}
                      {status === "intermission" && (
                        <>
                          <p className="text-xl font-bold text-blue-400">
                            ~{INTERMISSION_SLOTS - (slotsSinceEnd || 0)}
                          </p>
                          <p className="text-xs text-slate-500">Intermission Slots</p>
                        </>
                      )}
                      {status === "awaiting_reset" && (
                        <>
                          <p className="text-xl font-bold text-orange-400 animate-pulse">
                            Pending...
                          </p>
                          <p className="text-xs text-slate-500">Awaiting Reset</p>
                        </>
                      )}
                      {status === "waiting" && (
                        <>
                          <p className="text-xl font-bold text-yellow-400 animate-pulse">
                            Waiting...
                          </p>
                          <p className="text-xs text-slate-500">For First Deploy</p>
                        </>
                  )}
                </div>
                  );
                })()
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
                    Reward: <span className="text-emerald-400">{formatOre(round.top_miner_reward)} ORE</span>
                  </p>
                </div>
            </div>
          </div>
        )}
        </div>
      </div>

      {/* Tabs for Deployments/Winners/Losers */}
      {!isLive && round && (
        <>
          <div className="flex gap-2 border-b border-slate-700/50">
            <button
              onClick={() => { setActiveTab("deployments"); setSelectedMiner(null); }}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                activeTab === "deployments"
                  ? "text-amber-400 border-b-2 border-amber-400"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              Deployments ({visibleDeployments.length})
            </button>
            <button
              onClick={() => { setActiveTab("winners"); setSelectedMiner(null); }}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                activeTab === "winners"
                  ? "text-emerald-400 border-b-2 border-emerald-400"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              Winners ({winners.length})
            </button>
            <button
              onClick={() => { setActiveTab("losers"); setSelectedMiner(null); }}
              className={`px-4 py-2 text-sm font-medium transition-colors ${
                activeTab === "losers"
                  ? "text-red-400 border-b-2 border-red-400"
                  : "text-slate-400 hover:text-white"
              }`}
            >
              Losers ({losers.length})
            </button>
          </div>
          
          {activeTab === "deployments" && (
            <DeploymentsGroupedBySlot 
              deployments={visibleDeployments} 
              winningSquare={winningSquare}
              topMiner={round.top_miner}
            />
          )}
          
          {activeTab === "winners" && (
            <WinnersTab
              miners={winners}
              selectedMiner={selectedMiner}
              onSelectMiner={setSelectedMiner}
            />
          )}
          
          {activeTab === "losers" && (
            <LosersTab
              miners={losers}
              selectedMiner={selectedMiner}
              onSelectMiner={setSelectedMiner}
            />
          )}
        </>
      )}
      
      {/* Live Deployments Table */}
      {isLive && (
        <LiveDeploymentsTable deployments={liveDeployments} />
      )}
    </div>
  );
}

function HomePageContent() {
  const router = useRouter();
  const searchParams = useSearchParams();
  const initialRoundId = searchParams.get("round");
  
  // Use shared context for global data
  const { 
    round: liveRound, 
    currentSlot, 
    historicalRounds: rounds, 
    pendingRounds,
    hasMoreRounds,
    loadingMoreRounds,
    loadMoreRounds,
    loading: contextLoading,
    refreshRounds 
  } = useOreStats();
  
  // Filter state
  const [roundSearch, setRoundSearch] = useState<string>("");
  const [motherlodeOnly, setMotherlodeOnly] = useState(false);
  
  // Local state for page-specific data
  const [selectedRoundId, setSelectedRoundId] = useState<number | null>(
    initialRoundId ? parseInt(initialRoundId) : 0
  );
  const [roundDetail, setRoundDetail] = useState<RoundDetail | null>(null);
  const [liveDeployments, setLiveDeployments] = useState<LiveDeploymentDisplay[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  // Track previous round for clearing deployments on transition
  const [prevRoundId, setPrevRoundId] = useState<number | null>(null);

  // Handle direct round search/navigation
  const handleRoundSearch = () => {
    const roundId = parseInt(roundSearch, 10);
    if (!isNaN(roundId) && roundId > 0) {
      setSelectedRoundId(roundId);
      router.push(`/?round=${roundId}`);
    }
  };

  const handleSearchKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleRoundSearch();
    }
  };

  // Filter rounds by motherlode
  const filteredRounds = useMemo(() => {
    if (!motherlodeOnly) return rounds;
    return rounds.filter(r => r.motherlode_hit || r.motherlode > 0);
  }, [rounds, motherlodeOnly]);
  
  // Clear live deployments when round changes
  useEffect(() => {
    if (liveRound && prevRoundId !== null && liveRound.round_id !== prevRoundId) {
      setLiveDeployments([]);
    }
    if (liveRound) {
      setPrevRoundId(liveRound.round_id);
    }
  }, [liveRound?.round_id, prevRoundId]);
  
  // Load existing deployments when viewing live round
  const fetchLiveDeployments = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/live/deployments`);
      if (!res.ok) return;
      const data = await res.json();
      
      // Convert API response to display format
      const deployments: LiveDeploymentDisplay[] = [];
      for (const entry of data.deployments) {
        entry.amounts.forEach((amount: number, squareId: number) => {
          if (amount > 0) {
            deployments.push({
              miner_pubkey: entry.miner_pubkey,
              square_id: squareId,
              amount,
              slot: entry.slot,
            });
          }
        });
      }
      
      // Merge with existing SSE deployments (avoid duplicates)
      setLiveDeployments((prev) => {
        // Create a key for each deployment: miner + square
        const existing = new Set(prev.map(d => `${d.miner_pubkey}-${d.square_id}`));
        const newDeps = deployments.filter(d => !existing.has(`${d.miner_pubkey}-${d.square_id}`));
        return [...newDeps, ...prev]; // Put loaded deployments first, SSE after
      });
    } catch (err) {
      console.error("Failed to fetch live deployments:", err);
    }
  }, []);
  
  // Fetch existing deployments when switching to live round view
  useEffect(() => {
    if (selectedRoundId === 0 && liveRound) {
      fetchLiveDeployments();
    }
  }, [selectedRoundId, liveRound?.round_id, fetchLiveDeployments]);

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

  // Use a ref to track current round_id without causing effect re-runs
  const liveRoundIdRef = useRef<number | null>(null);
  useEffect(() => {
    liveRoundIdRef.current = liveRound?.round_id ?? null;
  }, [liveRound?.round_id]);
  
  // SSE subscription for live deployments - only depends on selectedRoundId
  useEffect(() => {
    if (selectedRoundId !== 0) return;
    
    console.log("[SSE] Opening deployments connection");
    const eventSource = new EventSource(`${API_BASE}/sse/deployments`);
    
    eventSource.addEventListener("deployment", (event) => {
      try {
        const wrapper = JSON.parse(event.data);
        // The SSE uses serde tag format: { "type": "Deployment", "data": { round_id, miner_pubkey, amounts, slot } }
        if (wrapper.type !== "Deployment" || !wrapper.data) return;
        const deployment: LiveDeploymentEvent = wrapper.data;
        
        if (!deployment) return;
        
        // Only process deployments for the current round (use ref to avoid stale closure)
        const currentRoundId = liveRoundIdRef.current;
        if (currentRoundId !== null && deployment.round_id !== currentRoundId) return;
        
        // Convert batched amounts array to individual deployment entries
        const newDeployments: LiveDeploymentDisplay[] = [];
        deployment.amounts.forEach((amount, squareId) => {
          if (amount > 0) {
            newDeployments.push({
              miner_pubkey: deployment.miner_pubkey,
              square_id: squareId,
              amount,
              slot: deployment.slot,
            });
          }
        });
        
        if (newDeployments.length > 0) {
          console.log("[SSE] Received deployment:", deployment.miner_pubkey, newDeployments.length, "squares");
          setLiveDeployments((prev) => [...prev, ...newDeployments]);
        }
      } catch (err) {
        console.error("Failed to parse deployment event:", err);
      }
    });
    
    eventSource.onerror = (e) => {
      console.error("SSE connection error:", e);
    };
    
    eventSource.onopen = () => {
      console.log("[SSE] Deployments connection opened");
    };
    
    return () => {
      console.log("[SSE] Closing deployments connection");
      eventSource.close();
    };
  }, [selectedRoundId]); // Only re-run when selectedRoundId changes

  // Fetch round detail when selecting a historical round
  useEffect(() => {
    if (selectedRoundId !== null && selectedRoundId !== 0) {
      fetchRoundDetail(selectedRoundId);
    }
  }, [selectedRoundId, fetchRoundDetail]);
  
  // Handle URL param for initial round selection
  useEffect(() => {
    if (initialRoundId && parseInt(initialRoundId) !== selectedRoundId) {
      setSelectedRoundId(parseInt(initialRoundId));
    }
  }, [initialRoundId]);

  return (
    <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950">
      <Header />

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
            
            {/* Filters */}
            <div className="space-y-3 mb-4 pb-4 border-b border-slate-700/50">
              {/* Go to Round */}
              <div className="flex items-center gap-2">
                <input
                  type="number"
                  placeholder="Go to round #"
                  value={roundSearch}
                  onChange={(e) => setRoundSearch(e.target.value)}
                  onKeyDown={handleSearchKeyDown}
                  className="flex-1 px-3 py-2 text-sm font-mono bg-slate-800/50 border border-slate-700/50 rounded-lg text-white placeholder-slate-500 focus:border-amber-500/50 focus:ring-1 focus:ring-amber-500/20 focus:outline-none"
                />
                <button
                  onClick={handleRoundSearch}
                  disabled={!roundSearch}
                  className="px-3 py-2 text-sm font-medium bg-amber-500 text-black rounded-lg hover:bg-amber-400 transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  Go
                </button>
              </div>
              
              {/* Motherlode Filter */}
              <button
                onClick={() => setMotherlodeOnly(!motherlodeOnly)}
                className={`w-full flex items-center justify-center gap-2 px-3 py-2 rounded-lg font-medium text-sm transition-all ${
                  motherlodeOnly
                    ? "bg-gradient-to-r from-amber-500 via-yellow-400 to-amber-500 text-black shadow-lg shadow-amber-500/20"
                    : "bg-slate-800 text-slate-400 hover:bg-slate-700 hover:text-white border border-slate-700"
                }`}
              >
                <svg 
                  className={`w-4 h-4 ${motherlodeOnly ? "text-black" : "text-amber-400"}`} 
                  viewBox="0 0 24 24" 
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <polygon points="12,2 22,9 12,22 2,9" fill={motherlodeOnly ? "currentColor" : "none"} />
                </svg>
                <span>Motherlodes</span>
                {motherlodeOnly && <span className="text-lg">💎</span>}
              </button>
              
              {motherlodeOnly && (
                <div className="text-xs text-amber-400/70 text-center flex items-center justify-center gap-1">
                  <span className="inline-block w-1.5 h-1.5 bg-amber-400 rounded-full animate-pulse" />
                  {filteredRounds.length} motherlode rounds
                </div>
              )}
            </div>
            
            {contextLoading && rounds.length === 0 ? (
              <div className="flex items-center justify-center h-48">
                <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
              </div>
            ) : (
              <RoundsList
                rounds={filteredRounds}
                pendingRounds={pendingRounds}
                selectedRoundId={selectedRoundId}
                onSelectRound={setSelectedRoundId}
                liveRound={liveRound}
                currentSlot={currentSlot}
                hasMore={hasMoreRounds && !motherlodeOnly}
                loadingMore={loadingMoreRounds}
                onLoadMore={loadMoreRounds}
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
              liveDeployments={liveDeployments}
              currentSlot={currentSlot}
            />
          </div>
        </div>
      </main>
    </div>
  );
}

// Wrap with Suspense for useSearchParams
export default function HomePage() {
  return (
    <Suspense fallback={
      <div className="min-h-screen bg-gradient-to-br from-slate-950 via-slate-900 to-slate-950 flex items-center justify-center">
        <div className="w-8 h-8 border-4 border-amber-500 border-t-transparent rounded-full animate-spin" />
      </div>
    }>
      <HomePageContent />
    </Suspense>
  );
}
