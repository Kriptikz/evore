-- ============================================================================
-- Migration 022: Fix inflation_per_round MV JOIN type mismatch
--
-- The MV created in 021 has a type mismatch in the LEFT JOIN condition.
-- This migration drops and recreates the MV with the correct cast.
-- ============================================================================

-- Drop the MV with the broken JOIN
DROP VIEW IF EXISTS ore_stats.inflation_per_round_mv;

-- Recreate with fixed JOIN (cast both sides to Int64)
CREATE MATERIALIZED VIEW ore_stats.inflation_per_round_mv
TO ore_stats.inflation_per_round
AS SELECT
    m.round_id AS round_id,
    m.supply AS supply,
    m.supply_change AS supply_change,
    t.total_unclaimed AS unclaimed,
    
    -- Calculate unclaimed_change by joining to previous round
    toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed)) AS unclaimed_change,
    
    -- Circulating = Supply - Unclaimed
    m.supply - t.total_unclaimed AS circulating,
    
    -- ore_won from rounds (base 1 ORE = 10^11 atomic + motherlode)
    toUInt64(100000000000) + (r.motherlode * r.motherlode_hit) AS ore_won,
    
    -- ore_claimed = ore_won - unclaimed_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) 
        - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS ore_claimed,
    
    -- ore_burned = ore_won - supply_change
    toInt64(100000000000 + (r.motherlode * r.motherlode_hit)) - m.supply_change AS ore_burned,
    
    -- market_inflation = supply_change - unclaimed_change (FIXED!)
    m.supply_change - (toInt64(t.total_unclaimed) - toInt64(COALESCE(t_prev.total_unclaimed, t.total_unclaimed))) AS market_inflation,
    
    m.created_at AS created_at
FROM ore_stats.mint_snapshots AS m
INNER JOIN ore_stats.treasury_snapshots AS t ON m.round_id = t.round_id
INNER JOIN ore_stats.rounds AS r ON m.round_id = r.round_id
LEFT JOIN ore_stats.treasury_snapshots AS t_prev ON toInt64(t_prev.round_id) = toInt64(m.round_id) - 1
WHERE t.round_id > 0;
