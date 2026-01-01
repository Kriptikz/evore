-- ClickHouse Migration 015: Create partial_rounds table
-- Stores round data when finalization times out (top_miner not populated)
-- Used by admin backfill to complete the round later

-- Partial rounds table (rounds that failed to finalize)
-- Contains everything EXCEPT top_miner which was never populated
CREATE TABLE IF NOT EXISTS ore_stats.partial_rounds (
    round_id UInt64,
    
    -- Round timing
    start_slot UInt64,
    end_slot UInt64,
    
    -- Hash data (available after reset)
    slot_hash FixedString(32),
    winning_square UInt8,
    
    -- Round totals (in lamports)
    total_deployed UInt64,
    total_vaulted UInt64,
    total_winnings UInt64,
    top_miner_reward UInt64,
    
    -- Motherlode
    motherlode UInt64,
    motherlode_hit UInt8,
    
    -- Stats from GPA snapshot
    unique_miners UInt32,
    total_deployments UInt32,
    
    -- Metadata
    created_at DateTime64(3) DEFAULT now64(3),
    failure_reason String DEFAULT ''
) ENGINE = ReplacingMergeTree(created_at)
ORDER BY round_id;

