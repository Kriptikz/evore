-- ============================================================================
-- Migration 019: Fix cost_per_ore_daily table engine
-- 
-- Problem: SummingMergeTree incorrectly sums the cost_per_ore_lamports ratio
-- on merge, inflating values by Nx where N = number of merged rows.
--
-- Solution: Use ReplacingMergeTree which keeps latest row instead of summing.
-- ============================================================================

-- Step 1: Drop the MV first (depends on table)
DROP VIEW IF EXISTS ore_stats.cost_per_ore_daily_mv;

-- Step 2: Drop the corrupted table
DROP TABLE IF EXISTS ore_stats.cost_per_ore_daily;

-- Step 3: Recreate table with correct engine
CREATE TABLE ore_stats.cost_per_ore_daily
(
    day Date,
    rounds_count UInt32,
    total_vaulted UInt64,
    ore_minted_base UInt64,
    ore_minted_motherlode UInt64,
    ore_minted_total UInt64,
    cost_per_ore_lamports UInt64
)
ENGINE = ReplacingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

-- Step 4: Recreate MV to auto-populate from rounds
CREATE MATERIALIZED VIEW ore_stats.cost_per_ore_daily_mv
TO ore_stats.cost_per_ore_daily
AS SELECT
    toDate(r.created_at) AS day,
    toUInt32(count()) AS rounds_count,
    sum(r.total_vaulted) AS total_vaulted,
    toUInt64(count()) * 100000000000 AS ore_minted_base,
    sum(r.motherlode * r.motherlode_hit) AS ore_minted_motherlode,
    toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit) AS ore_minted_total,
    if(toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit) > 0,
       toUInt64(sum(r.total_vaulted) * 100000000000 / 
                (toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit))),
       0) AS cost_per_ore_lamports
FROM ore_stats.rounds AS r
GROUP BY day;

