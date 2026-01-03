-- ============================================================================
-- Migration 028: Fix inflation MV type mismatch (lagInFrame returns Float64)
--
-- Problem: lagInFrame() returns Float64, causing type mismatch with Int64 columns
-- Solution: Explicitly cast lagInFrame result to UInt64
-- ============================================================================

-- Drop MVs first
DROP VIEW IF EXISTS ore_stats.inflation_daily_mv;
DROP VIEW IF EXISTS ore_stats.inflation_hourly_mv;
DROP VIEW IF EXISTS ore_stats.inflation_per_round_mv;

-- Recreate inflation_per_round MV with proper type casting
CREATE MATERIALIZED VIEW ore_stats.inflation_per_round_mv
TO ore_stats.inflation_per_round
AS 
WITH prev_treasury AS (
    SELECT 
        round_id,
        total_unclaimed,
        toUInt64(lagInFrame(total_unclaimed, 1, total_unclaimed) OVER (ORDER BY round_id)) AS prev_unclaimed
    FROM ore_stats.treasury_snapshots
)
SELECT
    m.round_id AS round_id,
    m.supply AS supply,
    m.supply_change AS supply_change,
    t.total_unclaimed AS unclaimed,
    
    -- Use the actual previous value from window function (properly cast)
    toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed) AS unclaimed_change,
    
    -- Circulating = Supply - Unclaimed
    m.supply - t.total_unclaimed AS circulating,
    
    -- ore_won from rounds (base 1 ORE = 10^11 atomic + motherlode)
    toUInt64(100000000000) + (r.motherlode * r.motherlode_hit) AS ore_won,
    
    -- ore_claimed = ore_won - unclaimed_change
    toInt64(toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) 
        - (toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed))) AS ore_claimed,
    
    -- ore_burned = ore_won - supply_change
    toInt64(toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) - m.supply_change) AS ore_burned,
    
    -- market_inflation = supply_change - unclaimed_change
    toInt64(m.supply_change - (toInt64(t.total_unclaimed) - toInt64(pt.prev_unclaimed))) AS market_inflation,
    
    m.created_at AS created_at
FROM ore_stats.mint_snapshots AS m
INNER JOIN ore_stats.treasury_snapshots AS t ON m.round_id = t.round_id
INNER JOIN ore_stats.rounds AS r ON m.round_id = r.round_id
INNER JOIN prev_treasury AS pt ON m.round_id = pt.round_id
WHERE t.round_id > 0;

-- Recreate hourly MV
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

-- Recreate daily MV
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

