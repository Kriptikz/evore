-- ============================================================================
-- Migration 026: Fix inflation calculations to handle gaps in treasury_snapshots
--
-- Problem: The LEFT JOIN to round_id - 1 breaks when there are gaps in data.
-- When gaps exist, we either get NULL (handled by COALESCE to 0) or find a
-- row many rounds back, causing huge incorrect jumps.
--
-- Solution: Use a subquery to find the actual previous snapshot by round_id,
-- not assuming round_id - 1 exists.
-- ============================================================================

-- ============================================================================
-- PART 1: Drop all inflation MVs and tables
-- ============================================================================
DROP VIEW IF EXISTS ore_stats.inflation_daily_mv;
DROP VIEW IF EXISTS ore_stats.inflation_hourly_mv;
DROP VIEW IF EXISTS ore_stats.inflation_per_round_mv;

DROP TABLE IF EXISTS ore_stats.inflation_daily;
DROP TABLE IF EXISTS ore_stats.inflation_hourly;
DROP TABLE IF EXISTS ore_stats.inflation_per_round;

-- ============================================================================
-- PART 2: Recreate inflation_per_round table
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
    
    -- ORE won from rounds (base 1 ORE + motherlode)
    ore_won UInt64,
    
    -- ORE claimed from unclaimed pool = ore_won - unclaimed_change
    -- Note: Can be negative if unclaimed grew more than ore_won (data gap issue)
    -- Frontend should handle/filter these cases
    ore_claimed Int64,
    
    -- ORE burned = ore_won - supply_change
    ore_burned Int64,
    
    -- Market Inflation = supply_change - unclaimed_change
    market_inflation Int64,
    
    created_at DateTime64(3) DEFAULT now64(3)
)
ENGINE = ReplacingMergeTree(created_at)
PARTITION BY intDiv(round_id, 10000)
ORDER BY round_id;

-- ============================================================================
-- PART 3: Recreate inflation_per_round MV with better gap handling
-- 
-- Uses a subquery to find the ACTUAL previous treasury snapshot, not assuming
-- round_id - 1 exists. This handles gaps gracefully.
-- ============================================================================
CREATE MATERIALIZED VIEW ore_stats.inflation_per_round_mv
TO ore_stats.inflation_per_round
AS 
WITH prev_treasury AS (
    SELECT 
        round_id,
        total_unclaimed,
        lagInFrame(total_unclaimed, 1, total_unclaimed) OVER (ORDER BY round_id) AS prev_unclaimed
    FROM ore_stats.treasury_snapshots
)
SELECT
    m.round_id AS round_id,
    m.supply AS supply,
    m.supply_change AS supply_change,
    t.total_unclaimed AS unclaimed,
    
    -- Use the actual previous value from window function
    toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed) AS unclaimed_change,
    
    -- Circulating = Supply - Unclaimed
    m.supply - t.total_unclaimed AS circulating,
    
    -- ore_won from rounds (base 1 ORE = 10^11 atomic + motherlode)
    toUInt64(100000000000) + (r.motherlode * r.motherlode_hit) AS ore_won,
    
    -- ore_claimed = ore_won - unclaimed_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) 
        - (toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed)) AS ore_claimed,
    
    -- ore_burned = ore_won - supply_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) - m.supply_change AS ore_burned,
    
    -- market_inflation = supply_change - unclaimed_change
    m.supply_change - (toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed)) AS market_inflation,
    
    m.created_at AS created_at
FROM ore_stats.mint_snapshots AS m
INNER JOIN ore_stats.treasury_snapshots AS t ON m.round_id = t.round_id
INNER JOIN ore_stats.rounds AS r ON m.round_id = r.round_id
INNER JOIN prev_treasury AS pt ON m.round_id = pt.round_id
WHERE t.round_id > 0;

-- ============================================================================
-- PART 4: Recreate inflation_hourly
-- ============================================================================
CREATE TABLE ore_stats.inflation_hourly
(
    hour DateTime,
    supply_end UInt64,
    supply_change_total Int64,
    unclaimed_end UInt64,
    unclaimed_change_total Int64,
    circulating_end UInt64,
    ore_won_total UInt64,
    ore_claimed_total Int64,
    ore_burned_total Int64,
    market_inflation_total Int64,
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
-- PART 5: Recreate inflation_daily
-- ============================================================================
CREATE TABLE ore_stats.inflation_daily
(
    day Date,
    supply_start UInt64,
    supply_end UInt64,
    supply_change_total Int64,
    unclaimed_start UInt64,
    unclaimed_end UInt64,
    unclaimed_change_total Int64,
    circulating_start UInt64,
    circulating_end UInt64,
    ore_won_total UInt64,
    ore_claimed_total Int64,
    ore_burned_total Int64,
    market_inflation_total Int64,
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

