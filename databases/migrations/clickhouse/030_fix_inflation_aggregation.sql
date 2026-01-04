-- ============================================================================
-- Migration 030: Fix inflation aggregation to calculate ore_buried from totals
--
-- Problem: Per-round ore_burned = ore_won - supply_change is wrong when there
-- are gaps in mint_snapshots (supply_change captures multiple rounds).
--
-- Also: ore_won was using 1.0 ORE base, but actual minting is 1.2 ORE per round
-- (1.0 base reward + 0.2 motherlode contribution)
--
-- Solution: For hourly/daily, calculate values from rounds_count * 1.2 ORE
-- ============================================================================

-- Drop MVs
DROP VIEW IF EXISTS ore_stats.inflation_daily_mv;
DROP VIEW IF EXISTS ore_stats.inflation_hourly_mv;

-- ============================================================================
-- Recreate hourly MV with correct aggregation
-- ============================================================================
CREATE MATERIALIZED VIEW ore_stats.inflation_hourly_mv
TO ore_stats.inflation_hourly
AS SELECT
    toStartOfHour(created_at) AS hour,
    argMax(supply, round_id) AS supply_end,
    toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id)) AS supply_change_total,
    argMax(unclaimed, round_id) AS unclaimed_end,
    toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id)) AS unclaimed_change_total,
    argMax(circulating, round_id) AS circulating_end,
    -- ore_minted = rounds_count * 1.2 ORE (120000000000 atomic units)
    -- Each round mints 1.0 ORE base + 0.2 ORE motherlode contribution = 1.2 ORE
    toUInt64(count()) * toUInt64(120000000000) AS ore_won_total,
    -- ore_claimed = ore_minted - unclaimed_change
    toInt64(toUInt64(count()) * toUInt64(120000000000)) 
        - (toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id))) AS ore_claimed_total,
    -- ore_buried = ore_minted - supply_change (if supply increased less than minted, some was buried)
    toInt64(toUInt64(count()) * toUInt64(120000000000)) 
        - (toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id))) AS ore_burned_total,
    -- market_inflation = supply_change - unclaimed_change
    (toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id))) 
        - (toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id))) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id
FROM ore_stats.inflation_per_round
GROUP BY hour;

-- ============================================================================
-- Recreate daily MV with correct aggregation
-- ============================================================================
CREATE MATERIALIZED VIEW ore_stats.inflation_daily_mv
TO ore_stats.inflation_daily
AS SELECT
    toDate(created_at) AS day,
    argMin(supply, round_id) AS supply_start,
    argMax(supply, round_id) AS supply_end,
    toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id)) AS supply_change_total,
    argMin(unclaimed, round_id) AS unclaimed_start,
    argMax(unclaimed, round_id) AS unclaimed_end,
    toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id)) AS unclaimed_change_total,
    argMin(circulating, round_id) AS circulating_start,
    argMax(circulating, round_id) AS circulating_end,
    -- ore_minted = rounds_count * 1.2 ORE
    toUInt64(count()) * toUInt64(120000000000) AS ore_won_total,
    -- ore_claimed = ore_minted - unclaimed_change
    toInt64(toUInt64(count()) * toUInt64(120000000000)) 
        - (toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id))) AS ore_claimed_total,
    -- ore_buried = ore_minted - supply_change
    toInt64(toUInt64(count()) * toUInt64(120000000000)) 
        - (toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id))) AS ore_burned_total,
    -- market_inflation = supply_change - unclaimed_change
    (toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id))) 
        - (toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id))) AS market_inflation_total,
    toUInt32(count()) AS rounds_count,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id
FROM ore_stats.inflation_per_round
GROUP BY day;
