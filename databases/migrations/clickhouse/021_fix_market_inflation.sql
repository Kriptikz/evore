-- ============================================================================
-- Migration 021: Fix market inflation tables with proper formulas
--
-- Adds ore_won, ore_claimed, ore_burned columns and fixes market_inflation
-- calculation to use: supply_change - unclaimed_change
--
-- NOTE: This depends on mint_snapshots, treasury_snapshots, and rounds tables
-- Treasury data only goes back ~1 week, so inflation data will be limited.
-- ============================================================================

-- ============================================================================
-- PART 1: Drop all inflation MVs (must drop before tables they target)
-- ============================================================================
DROP VIEW IF EXISTS ore_stats.inflation_daily_mv;
DROP VIEW IF EXISTS ore_stats.inflation_hourly_mv;
DROP VIEW IF EXISTS ore_stats.inflation_per_round_mv;

-- ============================================================================
-- PART 2: Drop inflation aggregate tables (will be rebuilt)
-- ============================================================================
DROP TABLE IF EXISTS ore_stats.inflation_daily;
DROP TABLE IF EXISTS ore_stats.inflation_hourly;
DROP TABLE IF EXISTS ore_stats.inflation_per_round;

-- ============================================================================
-- PART 3: Recreate inflation_per_round with new columns
-- ============================================================================
CREATE TABLE ore_stats.inflation_per_round
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
    
    -- NEW: ORE won from rounds (base 1 ORE + motherlode)
    ore_won UInt64,
    
    -- NEW: ORE claimed from unclaimed pool = ore_won - unclaimed_change
    ore_claimed Int64,
    
    -- NEW: ORE burned = ore_won - supply_change
    ore_burned Int64,
    
    -- FIXED: Market Inflation = supply_change - unclaimed_change
    market_inflation Int64,
    
    created_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = ReplacingMergeTree(created_at)
PARTITION BY intDiv(round_id, 10000)
ORDER BY round_id;

-- ============================================================================
-- PART 4: Recreate inflation_per_round MV with correct formula
-- ============================================================================
CREATE MATERIALIZED VIEW ore_stats.inflation_per_round_mv
TO ore_stats.inflation_per_round
AS SELECT
    m.round_id AS round_id,
    m.supply AS supply,
    m.supply_change AS supply_change,
    t.total_unclaimed AS unclaimed,
    
    -- Calculate unclaimed_change by joining to previous round
    toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed)) AS unclaimed_change,
    
    -- Circulating = Supply - Unclaimed
    m.supply - t.total_unclaimed AS circulating,
    
    -- ore_won from rounds (base 1 ORE = 10^11 atomic + motherlode)
    toUInt64(100000000000) + (r.motherlode * r.motherlode_hit) AS ore_won,
    
    -- ore_claimed = ore_won - unclaimed_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) 
        - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS ore_claimed,
    
    -- ore_burned = ore_won - supply_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) - m.supply_change AS ore_burned,
    
    -- market_inflation = supply_change - unclaimed_change (FIXED!)
    m.supply_change - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS market_inflation,
    
    m.created_at AS created_at
FROM ore_stats.mint_snapshots AS m
INNER JOIN ore_stats.treasury_snapshots AS t ON m.round_id = t.round_id
INNER JOIN ore_stats.rounds AS r ON m.round_id = r.round_id
LEFT JOIN ore_stats.treasury_snapshots AS t_prev ON t_prev.round_id = m.round_id - 1
WHERE t.round_id > 0;

-- ============================================================================
-- PART 5: Recreate inflation_hourly with new columns
-- ============================================================================
CREATE TABLE ore_stats.inflation_hourly
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
    
    -- NEW: Aggregated ore metrics
    ore_won_total UInt64,
    ore_claimed_total Int64,
    ore_burned_total Int64,
    
    -- Total market inflation this hour (FIXED)
    market_inflation_total Int64,
    
    -- Round info
    rounds_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree(rounds_count)
PARTITION BY toYYYYMM(hour)
ORDER BY hour;

CREATE MATERIALIZED VIEW ore_stats.inflation_hourly_mv
TO ore_stats.inflation_hourly
AS SELECT
    toStartOfHour(i.created_at) AS hour,
    argMax(i.supply, i.round_id) AS supply_end,
    sum(i.supply_change) AS supply_change_total,
    argMax(i.unclaimed, i.round_id) AS unclaimed_end,
    sum(i.unclaimed_change) AS unclaimed_change_total,
    argMax(i.circulating, i.round_id) AS circulating_end,
    sum(i.ore_won) AS ore_won_total,
    sum(i.ore_claimed) AS ore_claimed_total,
    sum(i.ore_burned) AS ore_burned_total,
    sum(i.market_inflation) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(i.round_id) AS min_round_id,
    max(i.round_id) AS max_round_id
FROM ore_stats.inflation_per_round AS i
GROUP BY hour;

-- ============================================================================
-- PART 6: Recreate inflation_daily with new columns
-- ============================================================================
CREATE TABLE ore_stats.inflation_daily
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
    
    -- NEW: Aggregated ore metrics
    ore_won_total UInt64,
    ore_claimed_total Int64,
    ore_burned_total Int64,
    
    -- Total market inflation this day (FIXED)
    market_inflation_total Int64,
    
    -- Round info
    rounds_count UInt32,
    min_round_id UInt64,
    max_round_id UInt64
)
ENGINE = ReplacingMergeTree(rounds_count)
PARTITION BY toYYYYMM(day)
ORDER BY day;

CREATE MATERIALIZED VIEW ore_stats.inflation_daily_mv
TO ore_stats.inflation_daily
AS SELECT
    toDate(i.created_at) AS day,
    argMin(i.supply, i.round_id) AS supply_start,
    argMax(i.supply, i.round_id) AS supply_end,
    sum(i.supply_change) AS supply_change_total,
    argMin(i.unclaimed, i.round_id) AS unclaimed_start,
    argMax(i.unclaimed, i.round_id) AS unclaimed_end,
    sum(i.unclaimed_change) AS unclaimed_change_total,
    argMin(i.circulating, i.round_id) AS circulating_start,
    argMax(i.circulating, i.round_id) AS circulating_end,
    sum(i.ore_won) AS ore_won_total,
    sum(i.ore_claimed) AS ore_claimed_total,
    sum(i.ore_burned) AS ore_burned_total,
    sum(i.market_inflation) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(i.round_id) AS min_round_id,
    max(i.round_id) AS max_round_id
FROM ore_stats.inflation_per_round AS i
GROUP BY day;

