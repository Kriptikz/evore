-- ============================================================================
-- Migration 020: Backfill cost_per_ore_daily from rounds
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

-- Optimize to merge any duplicate rows
OPTIMIZE TABLE ore_stats.cost_per_ore_daily FINAL;

