-- ============================================================================
-- Migration 014: Fix cost_per_ore_daily with proper MV
-- ============================================================================
--
-- Problem: cost_per_ore_daily was manually backfilled (no MV) because it had
-- cumulative columns that require window functions. But cumulative data isn't
-- needed - the frontend can compute it if required.
--
-- Solution: 
-- 1. Drop the old table and recreate with simpler schema (no cumulative columns)
-- 2. Create a proper MV that auto-populates from rounds
-- 3. Backfill from existing rounds data
-- ============================================================================

-- ============================================================================
-- STEP 1: Drop old table (no MV existed, so just drop the table)
-- ============================================================================
DROP TABLE IF EXISTS ore_stats.cost_per_ore_daily;

-- ============================================================================
-- STEP 2: Create new simplified table (no cumulative columns)
-- ============================================================================
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
    cost_per_ore_lamports UInt64
)
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day;

-- ============================================================================
-- STEP 3: Create MV to auto-populate from rounds
-- ============================================================================
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.cost_per_ore_daily_mv
TO ore_stats.cost_per_ore_daily
AS SELECT
    toDate(r.created_at) AS day,
    toUInt32(count()) AS rounds_count,
    sum(r.total_vaulted) AS total_vaulted,
    -- Base ORE = rounds * 10^11 (1 ORE per round)
    toUInt64(count()) * 100000000000 AS ore_minted_base,
    -- Motherlode ORE (already in atomic units)
    sum(r.motherlode * r.motherlode_hit) AS ore_minted_motherlode,
    -- Total ORE minted
    toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit) AS ore_minted_total,
    -- Daily cost per ORE (handle division by zero)
    if(toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit) > 0,
       toUInt64(sum(r.total_vaulted) * 100000000000 / 
                (toUInt64(count()) * 100000000000 + sum(r.motherlode * r.motherlode_hit))),
       0) AS cost_per_ore_lamports
FROM ore_stats.rounds AS r
GROUP BY day;

-- ============================================================================
-- STEP 4: Backfill from existing rounds
-- ============================================================================
INSERT INTO ore_stats.cost_per_ore_daily
SELECT
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
GROUP BY day
ORDER BY day;

-- ============================================================================
-- STEP 5: Optimize table
-- ============================================================================
OPTIMIZE TABLE ore_stats.cost_per_ore_daily FINAL;
