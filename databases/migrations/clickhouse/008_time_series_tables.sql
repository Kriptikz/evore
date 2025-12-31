-- ============================================================================
-- Migration 008: Time Series Tables and Query Optimizations
-- 
-- This migration:
-- 1. Adds secondary indexes to existing tables
-- 2. FIXES the broken miner_totals_mv (SummingMergeTree -> AggregatingMergeTree)
-- 3. Creates new time series tables for charts
--
-- Run migration 009 separately to backfill historical data.
-- ============================================================================

-- ============================================================================
-- PART 1: Add Secondary Indexes to Existing Tables
-- ============================================================================

-- Add bloom filter index on miner_pubkey for faster leaderboard queries
ALTER TABLE ore_stats.deployments 
ADD INDEX IF NOT EXISTS idx_miner_pubkey miner_pubkey TYPE bloom_filter(0.01) GRANULARITY 4;

-- Add index for round-based queries on treasury snapshots
ALTER TABLE ore_stats.treasury_snapshots 
ADD INDEX IF NOT EXISTS idx_round_id round_id TYPE minmax GRANULARITY 1;

-- Add index for querying rounds by slot range
ALTER TABLE ore_stats.rounds 
ADD INDEX IF NOT EXISTS idx_start_slot start_slot TYPE minmax GRANULARITY 1;

ALTER TABLE ore_stats.rounds 
ADD INDEX IF NOT EXISTS idx_end_slot end_slot TYPE minmax GRANULARITY 1;

-- ============================================================================
-- PART 2: Fix miner_totals_mv (Replace SummingMergeTree with AggregatingMergeTree)
-- 
-- The existing miner_totals_mv uses SummingMergeTree incorrectly with 
-- uniqExact and max aggregates. These don't work with SummingMergeTree.
--
-- Since miner_totals_mv has no TO clause, dropping the MV also drops its
-- implicit inner table. We simply recreate with the correct schema.
-- 
-- Backfill is done in migration 009.
-- ============================================================================

-- Step 1: Drop the old broken MV (also drops its implicit inner table)
DROP VIEW IF EXISTS ore_stats.miner_totals_mv;

-- Step 2: Create storage table with correct AggregatingMergeTree schema
CREATE TABLE IF NOT EXISTS ore_stats.miner_totals
(
    miner_pubkey LowCardinality(String),
    total_deployments AggregateFunction(count, UInt64),
    rounds_played AggregateFunction(uniqExact, UInt64),
    total_ore_earned AggregateFunction(sum, UInt64),
    total_sol_earned AggregateFunction(sum, UInt64),
    total_sol_deployed AggregateFunction(sum, UInt64),
    rounds_won AggregateFunction(sum, UInt8),
    times_top_miner AggregateFunction(sum, UInt8),
    net_sol_change AggregateFunction(sum, Int64),
    last_round_id AggregateFunction(max, UInt64),
    last_active AggregateFunction(max, DateTime64(3))
)
ENGINE = AggregatingMergeTree()
ORDER BY miner_pubkey;

-- Step 3: Create new MV with TO clause (populates miner_totals on new deployments)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.miner_totals_mv
TO ore_stats.miner_totals
AS SELECT
    miner_pubkey,
    countState() AS total_deployments,
    uniqExactState(round_id) AS rounds_played,
    sumState(ore_earned) AS total_ore_earned,
    sumState(sol_earned) AS total_sol_earned,
    sumState(amount) AS total_sol_deployed,
    sumState(is_winner) AS rounds_won,
    sumState(is_top_miner) AS times_top_miner,
    sumState(toInt64(sol_earned) - toInt64(amount)) AS net_sol_change,
    maxState(round_id) AS last_round_id,
    maxState(recorded_at) AS last_active
FROM ore_stats.deployments
GROUP BY miner_pubkey;

-- ============================================================================
-- PART 3: Create Time Series Tables for Charts
-- ============================================================================

-- ----------------------------------------------------------------------------
-- 3.1: Hourly Round Statistics (for main dashboard charts)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ore_stats.rounds_hourly
(
    hour DateTime,
    
    -- Counts
    rounds_count UInt32,
    total_deployments UInt64,
    unique_miners UInt64,
    
    -- Totals (lamports)
    total_deployed UInt64,
    total_vaulted UInt64,
    total_winnings UInt64,
    
    -- Motherlode
    motherlode_hits UInt32,
    total_motherlode UInt64,
    
    -- Averages (computed on insert, helps with charting)
    avg_deployed_per_round Float64,
    avg_miners_per_round Float64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY hour;

-- Materialized view for new rounds
-- Note: We use table.column syntax to avoid alias conflicts in ClickHouse
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rounds_hourly_mv
TO ore_stats.rounds_hourly
AS SELECT
    toStartOfHour(r.created_at) AS hour,
    toUInt32(count()) AS rounds_count,
    sum(r.total_deployments) AS total_deployments,
    sum(r.unique_miners) AS unique_miners,
    sum(r.total_deployed) AS total_deployed,
    sum(r.total_vaulted) AS total_vaulted,
    sum(r.total_winnings) AS total_winnings,
    toUInt32(sum(r.motherlode_hit)) AS motherlode_hits,
    sum(r.motherlode) AS total_motherlode,
    avg(r.total_deployed) AS avg_deployed_per_round,
    avg(r.unique_miners) AS avg_miners_per_round
FROM ore_stats.rounds AS r
GROUP BY hour;

-- ----------------------------------------------------------------------------
-- 3.2: Daily Round Statistics (for longer time ranges)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ore_stats.rounds_daily
(
    day Date,
    
    -- Counts
    rounds_count UInt32,
    total_deployments UInt64,
    unique_miners UInt64,
    
    -- Totals (lamports)
    total_deployed UInt64,
    total_vaulted UInt64,
    total_winnings UInt64,
    
    -- Motherlode
    motherlode_hits UInt32,
    total_motherlode UInt64,
    
    -- Min/Max round IDs for the day (useful for range queries)
    min_round_id UInt64,
    max_round_id UInt64,
    
    -- Slot info
    min_slot UInt64,
    max_slot UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rounds_daily_mv
TO ore_stats.rounds_daily
AS SELECT
    toDate(r.created_at) AS day,
    toUInt32(count()) AS rounds_count,
    sum(r.total_deployments) AS total_deployments,
    sum(r.unique_miners) AS unique_miners,
    sum(r.total_deployed) AS total_deployed,
    sum(r.total_vaulted) AS total_vaulted,
    sum(r.total_winnings) AS total_winnings,
    toUInt32(sum(r.motherlode_hit)) AS motherlode_hits,
    sum(r.motherlode) AS total_motherlode,
    min(r.round_id) AS min_round_id,
    max(r.round_id) AS max_round_id,
    min(r.start_slot) AS min_slot,
    max(r.end_slot) AS max_slot
FROM ore_stats.rounds AS r
GROUP BY day;

-- ----------------------------------------------------------------------------
-- 3.3: Treasury Hourly Snapshots (for treasury charts)
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ore_stats.treasury_hourly
(
    hour DateTime,
    
    -- Latest values at end of hour (using last observed value)
    balance UInt64,
    motherlode UInt64,
    total_staked UInt64,
    total_unclaimed UInt64,
    total_refined UInt64,
    
    -- Associated round (if available)
    round_id UInt64,
    
    -- Number of snapshots that hour
    snapshot_count UInt32
)
ENGINE = ReplacingMergeTree(snapshot_count)
PARTITION BY toYYYYMM(hour)
ORDER BY hour;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.treasury_hourly_mv
TO ore_stats.treasury_hourly
AS SELECT
    toStartOfHour(t.created_at) AS hour,
    argMax(t.balance, t.created_at) AS balance,
    argMax(t.motherlode, t.created_at) AS motherlode,
    argMax(t.total_staked, t.created_at) AS total_staked,
    argMax(t.total_unclaimed, t.created_at) AS total_unclaimed,
    argMax(t.total_refined, t.created_at) AS total_refined,
    argMax(t.round_id, t.created_at) AS round_id,
    toUInt32(count()) AS snapshot_count
FROM ore_stats.treasury_snapshots AS t
GROUP BY hour;

-- ----------------------------------------------------------------------------
-- 3.4: Daily Cost Per ORE Statistics
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ore_stats.cost_per_ore_daily
(
    day Date,
    
    -- Daily metrics
    rounds_count UInt32,
    total_vaulted UInt64,           -- Lamports vaulted that day
    ore_minted_base UInt64,         -- Base ORE (= rounds_count * 10^11 atomic units)
    ore_minted_motherlode UInt64,   -- Extra ORE from motherlode hits
    ore_minted_total UInt64,        -- Total ORE minted (atomic units)
    
    -- Cost per ORE (lamports per 1 ORE with 11 decimals)
    cost_per_ore_lamports UInt64,
    
    -- Cumulative totals (for cumulative charts)
    cumulative_rounds UInt64,
    cumulative_vaulted UInt64,
    cumulative_ore UInt64,
    cumulative_cost_per_ore UInt64
)
ENGINE = ReplacingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

-- Note: This table is populated by a scheduled job or backfill script
-- Not a real-time MV because we need cumulative calculations

-- ----------------------------------------------------------------------------
-- 3.5: Daily Active Miners (for miner activity charts)
-- 
-- Uses AggregatingMergeTree for correct uniqExact during backfills.
-- When you backfill old rounds, the uniqExactState properly deduplicates miners.
-- ----------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS ore_stats.miner_activity_daily
(
    day Date,
    
    -- Unique miners who deployed that day (aggregate state for correct merging)
    active_miners AggregateFunction(uniqExact, String),
    
    -- Deployment stats (simple sums work fine)
    total_deployments UInt64,
    total_deployed UInt64,
    total_won UInt64
)
ENGINE = AggregatingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.miner_activity_daily_mv
TO ore_stats.miner_activity_daily
AS SELECT
    toDate(d.recorded_at) AS day,
    uniqExactState(d.miner_pubkey) AS active_miners,
    count() AS total_deployments,
    sum(d.amount) AS total_deployed,
    sum(d.sol_earned) AS total_won
FROM ore_stats.deployments AS d
GROUP BY day;

