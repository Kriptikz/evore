-- ============================================================================
-- Migration 024: Fix mint_hourly and mint_daily tables
--
-- Problem: supply_change_total uses inconsistent calculations:
-- - Hourly: sum(supply_change) - sums per-round values which may be inaccurate
-- - Daily: argMax - argMin - calculates actual net change (correct approach)
--
-- Solution: Use argMax - argMin for both, ensuring we see actual supply change.
-- Also adds supply_start to hourly for consistency with daily.
-- ============================================================================

-- ============================================================================
-- PART 1: Drop existing MVs (must drop before modifying tables)
-- ============================================================================
DROP VIEW IF EXISTS ore_stats.mint_hourly_mv;
DROP VIEW IF EXISTS ore_stats.mint_daily_mv;

-- ============================================================================
-- PART 2: Drop and recreate mint_hourly with supply_start
-- ============================================================================
DROP TABLE IF EXISTS ore_stats.mint_hourly;

CREATE TABLE ore_stats.mint_hourly (
    hour DateTime,
    
    -- Supply at start and end of hour
    supply_start UInt64,
    supply UInt64,  -- supply at end of hour
    
    -- Supply change during this hour (end - start, actual net change)
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

-- Recreate MV with consistent calculation (argMax - argMin)
CREATE MATERIALIZED VIEW ore_stats.mint_hourly_mv
TO ore_stats.mint_hourly
AS SELECT
    toStartOfHour(m.created_at) AS hour,
    argMin(m.supply, m.created_at) AS supply_start,
    argMax(m.supply, m.created_at) AS supply,
    toInt64(argMax(m.supply, m.created_at)) - toInt64(argMin(m.supply, m.created_at)) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(m.round_id) AS min_round_id,
    max(m.round_id) AS max_round_id,
    toUInt32(count()) AS snapshot_count
FROM ore_stats.mint_snapshots AS m
GROUP BY hour;

-- ============================================================================
-- PART 3: Drop and recreate mint_daily (already uses correct formula, 
-- but recreating for consistency and to clear any bad data)
-- ============================================================================
DROP TABLE IF EXISTS ore_stats.mint_daily;

CREATE TABLE ore_stats.mint_daily (
    day Date,
    
    -- Latest supply at end of day
    supply UInt64,
    
    -- Supply at start of day
    supply_start UInt64,
    
    -- Supply change during this day (end - start)
    supply_change_total Int64,
    
    -- Round info
    round_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW ore_stats.mint_daily_mv
TO ore_stats.mint_daily
AS SELECT
    toDate(m.created_at) AS day,
    argMax(m.supply, m.created_at) AS supply,
    argMin(m.supply, m.created_at) AS supply_start,
    toInt64(argMax(m.supply, m.created_at)) - toInt64(argMin(m.supply, m.created_at)) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(m.round_id) AS min_round_id,
    max(m.round_id) AS max_round_id
FROM ore_stats.mint_snapshots AS m
GROUP BY day;

