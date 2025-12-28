-- Migration 006: Optimize miner_snapshots table
-- Changed ORDER BY from (round_id, miner_pubkey) to just round_id for better compression

DROP TABLE IF EXISTS ore_stats.miner_snapshots;

CREATE TABLE IF NOT EXISTS ore_stats.miner_snapshots (
    round_id UInt64,
    miner_pubkey LowCardinality(String),
    unclaimed_ore UInt64,
    refined_ore UInt64,
    lifetime_sol UInt64,
    lifetime_ore UInt64,
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY intDiv(round_id, 10000)
ORDER BY round_id;
