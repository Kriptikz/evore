-- ============================================================================
-- Migration 031: Backfill inflation_hourly and inflation_daily with correct aggregation
--
-- Uses rounds_count * 1.2 ORE for ore_minted calculation
-- (1.0 ORE base reward + 0.2 ORE motherlode contribution = 1.2 ORE per round)
-- ============================================================================

-- Clear existing aggregated data
TRUNCATE TABLE ore_stats.inflation_hourly;
TRUNCATE TABLE ore_stats.inflation_daily;

-- ============================================================================
-- PART 1: Backfill inflation_hourly with correct formulas
-- ============================================================================
INSERT INTO ore_stats.inflation_hourly
SELECT
    toStartOfHour(created_at) AS hour,
    argMax(supply, round_id) AS supply_end,
    toInt64(argMax(supply, round_id)) - toInt64(argMin(supply, round_id)) AS supply_change_total,
    argMax(unclaimed, round_id) AS unclaimed_end,
    toInt64(argMax(unclaimed, round_id)) - toInt64(argMin(unclaimed, round_id)) AS unclaimed_change_total,
    argMax(circulating, round_id) AS circulating_end,
    -- ore_minted = rounds_count * 1.2 ORE (120000000000 atomic units)
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
GROUP BY hour
ORDER BY hour;

-- ============================================================================
-- PART 2: Backfill inflation_daily with correct formulas
-- ============================================================================
INSERT INTO ore_stats.inflation_daily
SELECT
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
GROUP BY day
ORDER BY day;

-- ============================================================================
-- PART 3: Optimize
-- ============================================================================
OPTIMIZE TABLE ore_stats.inflation_hourly FINAL;
OPTIMIZE TABLE ore_stats.inflation_daily FINAL;
