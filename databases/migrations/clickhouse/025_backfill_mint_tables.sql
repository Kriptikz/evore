-- ============================================================================
-- Migration 025: Backfill mint_hourly and mint_daily from mint_snapshots
-- ============================================================================

-- ============================================================================
-- PART 1: Backfill mint_hourly
-- ============================================================================
INSERT INTO ore_stats.mint_hourly
SELECT
    toStartOfHour(m.created_at) AS hour,
    argMin(m.supply, m.created_at) AS supply_start,
    argMax(m.supply, m.created_at) AS supply,
    toInt64(argMax(m.supply, m.created_at)) - toInt64(argMin(m.supply, m.created_at)) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(m.round_id) AS min_round_id,
    max(m.round_id) AS max_round_id,
    toUInt32(count()) AS snapshot_count
FROM ore_stats.mint_snapshots AS m
GROUP BY hour
ORDER BY hour;

-- ============================================================================
-- PART 2: Backfill mint_daily
-- ============================================================================
INSERT INTO ore_stats.mint_daily
SELECT
    toDate(m.created_at) AS day,
    argMax(m.supply, m.created_at) AS supply,
    argMin(m.supply, m.created_at) AS supply_start,
    toInt64(argMax(m.supply, m.created_at)) - toInt64(argMin(m.supply, m.created_at)) AS supply_change_total,
    toUInt32(count()) AS round_count,
    min(m.round_id) AS min_round_id,
    max(m.round_id) AS max_round_id
FROM ore_stats.mint_snapshots AS m
GROUP BY day
ORDER BY day;

-- ============================================================================
-- PART 3: Optimize tables
-- ============================================================================
OPTIMIZE TABLE ore_stats.mint_hourly FINAL;
OPTIMIZE TABLE ore_stats.mint_daily FINAL;

