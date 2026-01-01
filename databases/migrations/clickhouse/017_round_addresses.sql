-- Round addresses mapping table
-- Maps round_id to its PDA address for efficient transaction lookups
-- This enables querying transactions by round since raw_transactions_v2 uses accounts array

CREATE TABLE IF NOT EXISTS ore_stats.round_addresses (
    round_id UInt64,
    address String,
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(created_at)
ORDER BY round_id;

-- Index for quick address lookups
ALTER TABLE ore_stats.round_addresses
ADD INDEX IF NOT EXISTS idx_round_address address TYPE bloom_filter GRANULARITY 4;

