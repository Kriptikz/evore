-- ============================================================================
-- Migration 011: Market Inflation Tables
-- 
-- Calculates circulating supply and market inflation by joining:
-- - mint_snapshots (total supply)
-- - treasury_snapshots (unclaimed ORE in treasury)
--
-- Formula:
--   Circulating Supply = Total Supply - Unclaimed in Treasury
--   Market Inflation = Change in Circulating Supply
--                    = Supply Change - Unclaimed Change
--
-- NOTE: No historical backfill possible - mint_snapshots only exist after
-- migration 010 is deployed. Data accumulates going forward.
-- ============================================================================

-- ============================================================================
-- PART 1: Per-Round Inflation (joins mint + treasury snapshots)
-- ============================================================================

CREATE TABLE IF NOT EXISTS ore_stats.inflation_per_round
(
    round_id UInt64,
    
    -- From mint_snapshots
    supply UInt64,
    supply_change Int64,
    
    -- From treasury_snapshots
    unclaimed UInt64,
    unclaimed_change Int64,
    
    -- Calculated: Circulating = Supply - Unclaimed
    circulating UInt64,
    
    -- Market Inflation = Supply Change - Unclaimed Change
    -- Positive = ORE claimed and entered circulation
    -- Negative = More ORE accumulated as unclaimed than was minted (rare)
    market_inflation Int64,
    
    created_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = ReplacingMergeTree(created_at)
PARTITION BY intDiv(round_id, 10000)
ORDER BY round_id;

-- MV that joins mint_snapshots and treasury_snapshots on round_id
-- Triggers when new rows are inserted into mint_snapshots
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.inflation_per_round_mv
TO ore_stats.inflation_per_round
AS
WITH 
    -- Get previous values for calculating changes
    prev AS (
        SELECT 
            round_id,
            supply,
            lagInFrame(supply) OVER (ORDER BY round_id) AS prev_supply
        FROM ore_stats.mint_snapshots
    )
SELECT
    m.round_id,
    m.supply,
    m.supply_change,
    t.total_unclaimed AS unclaimed,
    -- Calculate unclaimed change (need to get previous treasury snapshot)
    toInt64(t.total_unclaimed) - toInt64(
        lagInFrame(t.total_unclaimed) OVER (ORDER BY m.round_id)
    ) AS unclaimed_change,
    -- Circulating = Supply - Unclaimed
    m.supply - t.total_unclaimed AS circulating,
    -- Market Inflation = Supply Change - Unclaimed Change
    m.supply_change - (
        toInt64(t.total_unclaimed) - toInt64(
            lagInFrame(t.total_unclaimed) OVER (ORDER BY m.round_id)
        )
    ) AS market_inflation,
    m.created_at
FROM ore_stats.mint_snapshots m
INNER JOIN ore_stats.treasury_snapshots t ON m.round_id = t.round_id
WHERE t.round_id > 0;  -- Exclude periodic snapshots without round_id

-- ============================================================================
-- PART 2: Hourly Inflation Aggregate
-- ============================================================================

CREATE TABLE IF NOT EXISTS ore_stats.inflation_hourly
(
    hour DateTime,
    
    -- Supply at end of hour
    supply_end UInt64,
    supply_change_total Int64,
    
    -- Unclaimed at end of hour
    unclaimed_end UInt64,
    unclaimed_change_total Int64,
    
    -- Circulating at end of hour
    circulating_end UInt64,
    
    -- Total market inflation this hour
    market_inflation_total Int64,
    
    -- Round info
    rounds_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree(rounds_count)
PARTITION BY toYYYYMM(hour)
ORDER BY hour;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.inflation_hourly_mv
TO ore_stats.inflation_hourly
AS SELECT
    toStartOfHour(created_at) AS hour,
    argMax(supply, round_id) AS supply_end,
    sum(supply_change) AS supply_change_total,
    argMax(unclaimed, round_id) AS unclaimed_end,
    sum(unclaimed_change) AS unclaimed_change_total,
    argMax(circulating, round_id) AS circulating_end,
    sum(market_inflation) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id
FROM ore_stats.inflation_per_round
GROUP BY hour;

-- ============================================================================
-- PART 3: Daily Inflation Aggregate
-- ============================================================================

CREATE TABLE IF NOT EXISTS ore_stats.inflation_daily
(
    day Date,
    
    -- Supply at start/end of day
    supply_start UInt64,
    supply_end UInt64,
    supply_change_total Int64,
    
    -- Unclaimed at start/end of day
    unclaimed_start UInt64,
    unclaimed_end UInt64,
    unclaimed_change_total Int64,
    
    -- Circulating at start/end of day
    circulating_start UInt64,
    circulating_end UInt64,
    
    -- Total market inflation this day (= circulating_end - circulating_start)
    market_inflation_total Int64,
    
    -- Round info
    rounds_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree(rounds_count)
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.inflation_daily_mv
TO ore_stats.inflation_daily
AS SELECT
    toDate(created_at) AS day,
    argMin(supply, round_id) AS supply_start,
    argMax(supply, round_id) AS supply_end,
    sum(supply_change) AS supply_change_total,
    argMin(unclaimed, round_id) AS unclaimed_start,
    argMax(unclaimed, round_id) AS unclaimed_end,
    sum(unclaimed_change) AS unclaimed_change_total,
    argMin(circulating, round_id) AS circulating_start,
    argMax(circulating, round_id) AS circulating_end,
    sum(market_inflation) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id
FROM ore_stats.inflation_per_round
GROUP BY day;

