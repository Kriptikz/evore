-- ============================================================================
-- Migration 010: Mint Supply Snapshots
-- 
-- Tracks the ORE token mint supply after each round.
-- This provides historical data for:
-- - Total ORE supply over time (for charts)
-- - ORE inflation rate analysis
-- - Correlation with motherlode hits
-- ============================================================================

-- Mint supply snapshots (one per round, alongside treasury snapshots)
CREATE TABLE IF NOT EXISTS ore_stats.mint_snapshots (
    round_id UInt64,
    
    -- Mint supply (in atomic units - 11 decimals for ORE)
    supply UInt64,
    
    -- Optional: additional mint metadata (constants, but useful for verification)
    decimals UInt8 DEFAULT 11,
    
    -- Derived: supply change since last snapshot (calculated on insert or query)
    -- Positive = ORE minted (from rounds + motherlode)
    -- Stored to avoid window function on every query
    supply_change Int64 DEFAULT 0,
    
    -- Timestamp
    created_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = ReplacingMergeTree(created_at)
PARTITION BY intDiv(round_id, 10000)
ORDER BY round_id;

-- Index for time-based queries
ALTER TABLE ore_stats.mint_snapshots
ADD INDEX IF NOT EXISTS idx_created_at created_at TYPE minmax GRANULARITY 1;

-- ============================================================================
-- Hourly aggregate for charts (populated by MV)
-- ============================================================================

CREATE TABLE IF NOT EXISTS ore_stats.mint_hourly (
    hour DateTime,
    
    -- Latest supply at end of hour
    supply UInt64,
    
    -- Supply change during this hour (sum of all round changes)
    supply_change_total Int64,
    
    -- Round info
    round_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64,
    
    -- Snapshot count (for ReplacingMergeTree)
    snapshot_count UInt32
)
ENGINE = ReplacingMergeTree(snapshot_count)
PARTITION BY toYYYYMM(hour)
ORDER BY hour;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.mint_hourly_mv
TO ore_stats.mint_hourly
AS SELECT
    toStartOfHour(created_at) AS hour,
    argMax(supply, created_at) AS supply,
    sum(supply_change) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id,
    toUInt32(count()) AS snapshot_count
FROM ore_stats.mint_snapshots
GROUP BY hour;

-- ============================================================================
-- Daily aggregate for long-term charts
-- ============================================================================

CREATE TABLE IF NOT EXISTS ore_stats.mint_daily (
    day Date,
    
    -- Latest supply at end of day
    supply UInt64,
    
    -- Supply at start of day (for calculating daily change)
    supply_start UInt64,
    
    -- Supply change during this day
    supply_change_total Int64,
    
    -- Round info
    round_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.mint_daily_mv
TO ore_stats.mint_daily
AS SELECT
    toDate(created_at) AS day,
    argMax(supply, created_at) AS supply,
    argMin(supply, created_at) AS supply_start,
    toInt64(argMax(supply, created_at)) - toInt64(argMin(supply, created_at)) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id
FROM ore_stats.mint_snapshots
GROUP BY day;

