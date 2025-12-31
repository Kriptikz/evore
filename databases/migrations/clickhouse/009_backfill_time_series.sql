-- ============================================================================
-- Migration 009: Backfill Historical Data into Time Series Tables
-- 
-- ONE-TIME MIGRATION - Only run after migration 008 creates the tables
-- 
-- This populates the new time series tables from existing historical data.
-- The migration tracking table ensures this only runs once.
-- ============================================================================

-- ============================================================================
-- 1. Backfill miner_totals from existing deployments
-- (Migration 008 replaced the broken SummingMergeTree MV with AggregatingMergeTree)
-- ============================================================================
INSERT INTO ore_stats.miner_totals
SELECT
    miner_pubkey,
    countState() AS total_deployments,
    uniqExactState(round_id) AS rounds_played,
    sumState(ore_earned) AS total_ore_earned,
    sumState(sol_earned) AS total_sol_earned,
    sumState(amount) AS total_sol_deployed,
    sumState(is_winner) AS rounds_won,
    sumState(is_top_miner) AS times_top_miner,
    sumState(toInt64(sol_earned) - toInt64(amount)) AS net_sol_change,
    maxState(round_id) AS last_round_id,
    maxState(recorded_at) AS last_active
FROM ore_stats.deployments
GROUP BY miner_pubkey;

-- ============================================================================
-- 2. Backfill rounds_hourly from existing rounds
-- ============================================================================
INSERT INTO ore_stats.rounds_hourly
SELECT
    toStartOfHour(created_at) AS hour,
    toUInt32(count()) AS rounds_count,
    sum(total_deployments) AS total_deployments,
    sum(unique_miners) AS unique_miners,
    sum(total_deployed) AS total_deployed,
    sum(total_vaulted) AS total_vaulted,
    sum(total_winnings) AS total_winnings,
    toUInt32(sum(motherlode_hit)) AS motherlode_hits,
    sum(motherlode) AS total_motherlode,
    avg(total_deployed) AS avg_deployed_per_round,
    avg(unique_miners) AS avg_miners_per_round
FROM ore_stats.rounds
GROUP BY hour
ORDER BY hour;

-- ============================================================================
-- 3. Backfill rounds_daily from existing rounds
-- ============================================================================
INSERT INTO ore_stats.rounds_daily
SELECT
    toDate(created_at) AS day,
    toUInt32(count()) AS rounds_count,
    sum(total_deployments) AS total_deployments,
    sum(unique_miners) AS unique_miners,
    sum(total_deployed) AS total_deployed,
    sum(total_vaulted) AS total_vaulted,
    sum(total_winnings) AS total_winnings,
    toUInt32(sum(motherlode_hit)) AS motherlode_hits,
    sum(motherlode) AS total_motherlode,
    min(round_id) AS min_round_id,
    max(round_id) AS max_round_id,
    min(start_slot) AS min_slot,
    max(end_slot) AS max_slot
FROM ore_stats.rounds
GROUP BY day
ORDER BY day;

-- ============================================================================
-- 4. Backfill treasury_hourly from existing treasury_snapshots
-- ============================================================================
INSERT INTO ore_stats.treasury_hourly
SELECT
    toStartOfHour(created_at) AS hour,
    argMax(balance, created_at) AS balance,
    argMax(motherlode, created_at) AS motherlode,
    argMax(total_staked, created_at) AS total_staked,
    argMax(total_unclaimed, created_at) AS total_unclaimed,
    argMax(total_refined, created_at) AS total_refined,
    argMax(round_id, created_at) AS round_id,
    toUInt32(count()) AS snapshot_count
FROM ore_stats.treasury_snapshots
GROUP BY hour
ORDER BY hour;

-- ============================================================================
-- 5. Backfill miner_activity_daily from existing deployments
-- (Uses AggregatingMergeTree with uniqExactState for correct backfill merging)
-- ============================================================================
INSERT INTO ore_stats.miner_activity_daily
SELECT
    toDate(recorded_at) AS day,
    uniqExactState(miner_pubkey) AS active_miners,
    count() AS total_deployments,
    sum(amount) AS total_deployed,
    sum(sol_earned) AS total_won
FROM ore_stats.deployments
GROUP BY day
ORDER BY day;

-- ============================================================================
-- 6. Backfill cost_per_ore_daily with cumulative calculations
-- 
-- This uses a window function to calculate cumulative totals
-- ============================================================================
INSERT INTO ore_stats.cost_per_ore_daily
WITH daily_stats AS (
    SELECT
        toDate(created_at) AS day,
        toUInt32(count()) AS rounds_count,
        sum(total_vaulted) AS total_vaulted,
        -- Base ORE = rounds * 10^11 (1 ORE per round)
        toUInt64(count()) * 100000000000 AS ore_minted_base,
        -- Motherlode ORE (already in atomic units)
        sum(motherlode * motherlode_hit) AS ore_minted_motherlode
    FROM ore_stats.rounds
    GROUP BY day
    ORDER BY day
)
SELECT
    day,
    rounds_count,
    total_vaulted,
    ore_minted_base,
    ore_minted_motherlode,
    ore_minted_base + ore_minted_motherlode AS ore_minted_total,
    -- Daily cost per ORE
    if(ore_minted_base + ore_minted_motherlode > 0,
       toUInt64(total_vaulted * 100000000000 / (ore_minted_base + ore_minted_motherlode)),
       0) AS cost_per_ore_lamports,
    -- Cumulative totals using window functions
    sum(rounds_count) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS cumulative_rounds,
    sum(total_vaulted) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS cumulative_vaulted,
    sum(ore_minted_base + ore_minted_motherlode) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) AS cumulative_ore,
    -- Cumulative cost per ORE
    if(sum(ore_minted_base + ore_minted_motherlode) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) > 0,
       toUInt64(sum(total_vaulted) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) * 100000000000 / 
                sum(ore_minted_base + ore_minted_motherlode) OVER (ORDER BY day ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW)),
       0) AS cumulative_cost_per_ore
FROM daily_stats
ORDER BY day;

-- ============================================================================
-- 7. Optimize new tables after backfill (merge parts for better query perf)
-- ============================================================================
OPTIMIZE TABLE ore_stats.miner_totals FINAL;
OPTIMIZE TABLE ore_stats.rounds_hourly FINAL;
OPTIMIZE TABLE ore_stats.rounds_daily FINAL;
OPTIMIZE TABLE ore_stats.treasury_hourly FINAL;
OPTIMIZE TABLE ore_stats.miner_activity_daily FINAL;
OPTIMIZE TABLE ore_stats.cost_per_ore_daily FINAL;

