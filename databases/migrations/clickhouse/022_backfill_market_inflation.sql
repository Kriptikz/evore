-- ============================================================================
-- Migration 022: Backfill inflation tables from mint_snapshots + treasury_snapshots
--
-- NOTE: Data limited to rounds that have BOTH mint_snapshots AND treasury_snapshots
-- (treasury data only goes back ~1 week)
-- ============================================================================

-- ============================================================================
-- PART 1: Backfill inflation_per_round
-- ============================================================================
INSERT INTO ore_stats.inflation_per_round
SELECT
    m.round_id,
    m.supply,
    m.supply_change,
    t.total_unclaimed AS unclaimed,
    toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed)) AS unclaimed_change,
    m.supply - t.total_unclaimed AS circulating,
    toUInt64(100000000000) + (r.motherlode * r.motherlode_hit) AS ore_won,
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) 
        - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS ore_claimed,
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) - m.supply_change AS ore_burned,
    m.supply_change - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS market_inflation,
    m.created_at
FROM ore_stats.mint_snapshots AS m
INNER JOIN ore_stats.treasury_snapshots AS t ON m.round_id = t.round_id
INNER JOIN ore_stats.rounds AS r ON m.round_id = r.round_id
LEFT JOIN ore_stats.treasury_snapshots AS t_prev ON t_prev.round_id = m.round_id - 1
WHERE t.round_id > 0
ORDER BY m.round_id;

-- ============================================================================
-- PART 2: Backfill inflation_hourly from inflation_per_round
-- ============================================================================
INSERT INTO ore_stats.inflation_hourly
SELECT
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
GROUP BY hour
ORDER BY hour;

-- ============================================================================
-- PART 3: Backfill inflation_daily from inflation_per_round
-- ============================================================================
INSERT INTO ore_stats.inflation_daily
SELECT
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
GROUP BY day
ORDER BY day;

-- ============================================================================
-- PART 4: Optimize tables
-- ============================================================================
OPTIMIZE TABLE ore_stats.inflation_per_round FINAL;
OPTIMIZE TABLE ore_stats.inflation_hourly FINAL;
OPTIMIZE TABLE ore_stats.inflation_daily FINAL;

