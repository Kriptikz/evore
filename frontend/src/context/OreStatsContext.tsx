"use client";

import React, { createContext, useContext, useEffect, useState, useCallback, useMemo, ReactNode } from "react";

// ============================================================================
// Types
// ============================================================================

export interface Treasury {
  balance: number;       // HaWG balance
  motherlode: number;    // Current motherlode size
  total_staked: number;
  total_unclaimed: number;
  total_refined: number;
}

export interface Board {
  round_id: number;
  start_slot: number;
  end_slot: number;
}

export interface LiveRound {
  round_id: number;
  start_slot: number;
  end_slot: number;
  slots_remaining: number;
  deployed: number[];
  count: number[];
  total_deployed: number;
  unique_miners: number;
}

export interface RoundSummary {
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
}

// Phase states for the round lifecycle
export type RoundPhase = 
  | "active"           // Round in progress, slots_remaining > 0
  | "intermission"     // Round ended, within 35 slots after end_slot
  | "awaiting_reset"   // Past 35 slots, waiting for reset transaction
  | "waiting"          // Reset done, end_slot == u64::MAX, waiting for first deploy
  | "finalizing";      // Round just ended, being finalized by backend

// Pending round (from live data, before finalization)
export interface PendingRound {
  round_id: number;
  start_slot: number;
  end_slot: number;
  total_deployed: number;
  unique_miners: number;
  deployed: number[];
  status: "finalizing";
}

// ============================================================================
// Constants
// ============================================================================

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "";
const U64_MAX_THRESHOLD = 9999999999999999; // If end_slot is larger than this, it's u64::MAX
const INTERMISSION_SLOTS = 35;

// Polling intervals (ms)
const SLOT_POLL_INTERVAL = 1000;     // Every second for slot
const ROUND_POLL_INTERVAL = 2000;    // Every 2 seconds for round/board
const TREASURY_POLL_INTERVAL = 60000; // Every minute (only changes on finalization)

// ============================================================================
// Phase Detection
// ============================================================================

export function getRoundPhase(board: Board, currentSlot: number): RoundPhase {
  // If end_slot is u64::MAX, waiting for first deployment
  if (board.end_slot > U64_MAX_THRESHOLD) {
    return "waiting";
  }
  
  // If current slot is past end_slot
  if (currentSlot > board.end_slot) {
    const slotsSinceEnd = currentSlot - board.end_slot;
    
    if (slotsSinceEnd <= INTERMISSION_SLOTS) {
      return "intermission";
    } else {
      return "awaiting_reset";
    }
  }
  
  return "active";
}

// ============================================================================
// Context
// ============================================================================

interface OreStatsContextValue {
  // Core data
  treasury: Treasury | null;
  board: Board | null;
  round: LiveRound | null;
  currentSlot: number;
  
  // Derived state
  phase: RoundPhase | null;
  slotsRemaining: number;
  slotsSinceEnd: number;
  
  // Historical rounds with pending
  historicalRounds: RoundSummary[];
  pendingRounds: PendingRound[];
  
  // Pagination state
  hasMoreRounds: boolean;
  roundsNextCursor: number | null;
  loadingMoreRounds: boolean;
  
  // Loading/error states
  loading: boolean;
  error: string | null;
  
  // Actions
  refreshRounds: () => Promise<void>;
  loadMoreRounds: () => Promise<void>;
  addPendingRound: (round: PendingRound) => void;
  finalizePendingRound: (roundId: number, finalizedData: RoundSummary) => void;
}

const OreStatsContext = createContext<OreStatsContextValue | null>(null);

// ============================================================================
// Provider
// ============================================================================

export function OreStatsProvider({ children }: { children: ReactNode }) {
  // Core state
  const [treasury, setTreasury] = useState<Treasury | null>(null);
  const [board, setBoard] = useState<Board | null>(null);
  const [round, setRound] = useState<LiveRound | null>(null);
  const [currentSlot, setCurrentSlot] = useState<number>(0);
  
  // Historical rounds
  const [historicalRounds, setHistoricalRounds] = useState<RoundSummary[]>([]);
  const [pendingRounds, setPendingRounds] = useState<PendingRound[]>([]);
  
  // Pagination state
  const [hasMoreRounds, setHasMoreRounds] = useState(false);
  const [roundsNextCursor, setRoundsNextCursor] = useState<number | null>(null);
  const [loadingMoreRounds, setLoadingMoreRounds] = useState(false);
  
  // Track previous round for transition detection
  const [previousRoundId, setPreviousRoundId] = useState<number | null>(null);
  
  // Loading/error
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Fetch functions
  const fetchSlot = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/slot`);
      if (res.ok) {
        const data = await res.json();
        setCurrentSlot(data.slot);
      }
    } catch (err) {
      console.error("Failed to fetch slot:", err);
    }
  }, []);

  const fetchBoard = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/board`);
      if (res.ok) {
        const data = await res.json();
        setBoard(data);
      }
    } catch (err) {
      console.error("Failed to fetch board:", err);
    }
  }, []);

  const fetchRound = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/round`);
      if (res.ok) {
        const data = await res.json();
        
        // Detect round transition
        if (previousRoundId !== null && data.round_id !== previousRoundId) {
          // Round changed! Create pending round from previous data
          if (round) {
            const pending: PendingRound = {
              round_id: round.round_id,
              start_slot: round.start_slot,
              end_slot: round.end_slot,
              total_deployed: round.total_deployed,
              unique_miners: round.unique_miners,
              deployed: round.deployed,
              status: "finalizing",
            };
            setPendingRounds(prev => [pending, ...prev]);
          }
          
          // Refresh historical rounds to get the finalized data
          setTimeout(() => refreshRounds(), 5000); // Give backend time to finalize
        }
        
        setPreviousRoundId(data.round_id);
        setRound(data);
      }
    } catch (err) {
      console.error("Failed to fetch round:", err);
    }
  }, [previousRoundId, round]);

  const fetchTreasury = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/treasury`);
      if (res.ok) {
        const data = await res.json();
        setTreasury(data);
      }
    } catch (err) {
      console.error("Failed to fetch treasury:", err);
    }
  }, []);

  const refreshRounds = useCallback(async () => {
    try {
      const res = await fetch(`${API_BASE}/rounds?per_page=50`);
      if (res.ok) {
        const data = await res.json();
        setHistoricalRounds(data.rounds || []);
        setHasMoreRounds(data.has_more || false);
        setRoundsNextCursor(data.next_cursor || null);
        
        // Check if any pending rounds are now finalized
        if (data.rounds?.length > 0) {
          const finalizedIds = new Set(data.rounds.map((r: RoundSummary) => r.round_id));
          setPendingRounds(prev => prev.filter(p => !finalizedIds.has(p.round_id)));
        }
      }
    } catch (err) {
      console.error("Failed to fetch rounds:", err);
    }
  }, []);
  
  const loadMoreRounds = useCallback(async () => {
    if (!hasMoreRounds || !roundsNextCursor || loadingMoreRounds) return;
    
    setLoadingMoreRounds(true);
    try {
      const res = await fetch(`${API_BASE}/rounds?per_page=50&before=${roundsNextCursor}`);
      if (res.ok) {
        const data = await res.json();
        if (data.rounds?.length > 0) {
          setHistoricalRounds(prev => [...prev, ...data.rounds]);
          setHasMoreRounds(data.has_more || false);
          setRoundsNextCursor(data.next_cursor || null);
        } else {
          setHasMoreRounds(false);
          setRoundsNextCursor(null);
        }
      }
    } catch (err) {
      console.error("Failed to load more rounds:", err);
    } finally {
      setLoadingMoreRounds(false);
    }
  }, [hasMoreRounds, roundsNextCursor, loadingMoreRounds]);

  const addPendingRound = useCallback((round: PendingRound) => {
    setPendingRounds(prev => [round, ...prev.filter(p => p.round_id !== round.round_id)]);
  }, []);

  const finalizePendingRound = useCallback((roundId: number, finalizedData: RoundSummary) => {
    setPendingRounds(prev => prev.filter(p => p.round_id !== roundId));
    setHistoricalRounds(prev => {
      const filtered = prev.filter(r => r.round_id !== roundId);
      return [finalizedData, ...filtered].sort((a, b) => b.round_id - a.round_id);
    });
  }, []);

  // Computed values
  const phase = useMemo((): RoundPhase | null => {
    if (!board || currentSlot === 0) return null;
    return getRoundPhase(board, currentSlot);
  }, [board, currentSlot]);

  const slotsRemaining = useMemo(() => {
    if (!board || currentSlot === 0 || board.end_slot > U64_MAX_THRESHOLD) return 0;
    return Math.max(0, board.end_slot - currentSlot);
  }, [board, currentSlot]);

  const slotsSinceEnd = useMemo(() => {
    if (!board || currentSlot === 0 || board.end_slot > U64_MAX_THRESHOLD) return 0;
    if (currentSlot <= board.end_slot) return 0;
    return currentSlot - board.end_slot;
  }, [board, currentSlot]);

  // Initial load
  useEffect(() => {
    const init = async () => {
      setLoading(true);
      try {
        await Promise.all([
          fetchSlot(),
          fetchBoard(),
          fetchRound(),
          fetchTreasury(),
          refreshRounds(),
        ]);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Failed to initialize");
      } finally {
        setLoading(false);
      }
    };
    init();
  }, []);

  // Polling intervals
  useEffect(() => {
    const slotInterval = setInterval(fetchSlot, SLOT_POLL_INTERVAL);
    return () => clearInterval(slotInterval);
  }, [fetchSlot]);

  useEffect(() => {
    const roundInterval = setInterval(() => {
      fetchBoard();
      fetchRound();
    }, ROUND_POLL_INTERVAL);
    return () => clearInterval(roundInterval);
  }, [fetchBoard, fetchRound]);

  useEffect(() => {
    const treasuryInterval = setInterval(fetchTreasury, TREASURY_POLL_INTERVAL);
    return () => clearInterval(treasuryInterval);
  }, [fetchTreasury]);

  const value: OreStatsContextValue = {
    treasury,
    board,
    round,
    currentSlot,
    phase,
    slotsRemaining,
    slotsSinceEnd,
    historicalRounds,
    pendingRounds,
    hasMoreRounds,
    roundsNextCursor,
    loadingMoreRounds,
    loading,
    error,
    refreshRounds,
    loadMoreRounds,
    addPendingRound,
    finalizePendingRound,
  };

  return (
    <OreStatsContext.Provider value={value}>
      {children}
    </OreStatsContext.Provider>
  );
}

// ============================================================================
// Hook
// ============================================================================

export function useOreStats() {
  const context = useContext(OreStatsContext);
  if (!context) {
    throw new Error("useOreStats must be used within an OreStatsProvider");
  }
  return context;
}

// ============================================================================
// Format Helpers
// ============================================================================

export const formatSol = (lamports: number) => (lamports / 1e9).toFixed(4);
export const formatOre = (atomicOre: number) => (atomicOre / 1e11).toFixed(2);
export const truncateAddress = (s: string) => s.length > 12 ? `${s.slice(0, 6)}...${s.slice(-4)}` : s;

