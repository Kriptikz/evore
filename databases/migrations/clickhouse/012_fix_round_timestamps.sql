-- ============================================================================
-- Migration 012: Fix round timestamps for backfilled data
-- ============================================================================
--
-- Problem: The rounds_hourly and rounds_daily materialized views were 
-- aggregating data by `created_at` which was set to the database insertion
-- time instead of the actual round timestamp.
--
-- Solution:
-- 1. For backfilled rounds where we have the original API timestamp, we should
--    have the correct created_at after the Rust code fix
-- 2. Drop and rebuild the time series tables to reflect correct timestamps
--
-- NOTE: After running this migration, you should re-run the backfill to 
-- populate rounds with correct created_at timestamps from the external API.
-- ============================================================================

-- ============================================================================
-- STEP 1: Drop existing time series MVs (they have incorrect data)
-- ============================================================================

DROP VIEW IF EXISTS ore_stats.rounds_hourly_mv;
DROP VIEW IF EXISTS ore_stats.rounds_daily_mv;

-- ============================================================================
-- STEP 2: Truncate time series tables (delete all rows but keep schema)
-- ============================================================================

TRUNCATE TABLE IF EXISTS ore_stats.rounds_hourly;
TRUNCATE TABLE IF EXISTS ore_stats.rounds_daily;

-- ============================================================================
-- STEP 3: Recreate the MVs (same as 008, they will capture new data correctly)
-- ============================================================================

-- Recreate MVs (same as original 008 migration)
-- The created_at column is DateTime64(3), so we use it directly
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rounds_hourly_mv
TO ore_stats.rounds_hourly
AS SELECT
    toStartOfHour(r.created_at) AS hour,
    toUInt32(count()) AS rounds_count,
    sum(r.total_deployments) AS total_deployments,
    sum(r.unique_miners) AS unique_miners,
    sum(r.total_deployed) AS total_deployed,
    sum(r.total_vaulted) AS total_vaulted,
    sum(r.total_winnings) AS total_winnings,
    toUInt32(sum(r.motherlode_hit)) AS motherlode_hits,
    sum(r.motherlode) AS total_motherlode,
    avg(r.total_deployed) AS avg_deployed_per_round,
    avg(r.unique_miners) AS avg_miners_per_round
FROM ore_stats.rounds AS r
GROUP BY hour;

CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rounds_daily_mv
TO ore_stats.rounds_daily
AS SELECT
    toDate(r.created_at) AS day,
    toUInt32(count()) AS rounds_count,
    sum(r.total_deployments) AS total_deployments,
    sum(r.unique_miners) AS unique_miners,
    sum(r.total_deployed) AS total_deployed,
    sum(r.total_vaulted) AS total_vaulted,
    sum(r.total_winnings) AS total_winnings,
    toUInt32(sum(r.motherlode_hit)) AS motherlode_hits,
    sum(r.motherlode) AS total_motherlode,
    min(r.round_id) AS min_round_id,
    max(r.round_id) AS max_round_id,
    min(r.start_slot) AS min_slot,
    max(r.end_slot) AS max_slot
FROM ore_stats.rounds AS r
GROUP BY day;

-- ============================================================================
-- HOW TO USE THIS MIGRATION
-- ============================================================================
--
-- Run these steps IN ORDER:
--
-- STEP A: Delete backfilled rounds (they have wrong timestamps)
--   ALTER TABLE ore_stats.rounds DELETE WHERE source = 'backfill';
--   
--   Wait for mutation to complete:
--   SELECT * FROM system.mutations WHERE table = 'rounds' AND is_done = 0;
--
-- STEP B: Run this migration (012)
--   This clears the time series tables and recreates MVs
--   Live data will remain and be correctly aggregated
--
-- STEP C: Re-backfill rounds from external API
--   Use the admin UI or trigger the backfill task
--   The fixed code will now use the API's ts field for created_at
--   The MVs will automatically populate as new rounds are inserted
--
-- ============================================================================

