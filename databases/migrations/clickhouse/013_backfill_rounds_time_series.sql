-- ============================================================================
-- Migration 013: Backfill rounds time series from existing data
-- ============================================================================
--
-- Run this AFTER migration 012 to populate time series tables from existing
-- rounds data (live rounds that were already in the database).
--
-- The MVs created in 012 only capture NEW inserts, so we need to manually
-- populate from existing data.
-- ============================================================================

-- Backfill rounds_hourly from existing rounds
INSERT INTO ore_stats.rounds_hourly
SELECT
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
GROUP BY hour
ORDER BY hour;

-- Backfill rounds_daily from existing rounds
INSERT INTO ore_stats.rounds_daily
SELECT
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
GROUP BY day
ORDER BY day;

-- Optimize tables after bulk insert
OPTIMIZE TABLE ore_stats.rounds_hourly FINAL;
OPTIMIZE TABLE ore_stats.rounds_daily FINAL;

