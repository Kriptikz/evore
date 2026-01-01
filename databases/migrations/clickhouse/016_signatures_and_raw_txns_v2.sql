-- ClickHouse Migration 016: Create signatures and raw_transactions_v2 tables
-- Replaces round_id-based querying with account-based querying
-- Enables finding transactions by ANY account (miner, program, round PDA, etc.)

-- Signatures table: Stores signature metadata for quick existence checks
-- and incremental fetching
CREATE TABLE IF NOT EXISTS ore_stats.signatures (
    signature String,
    slot UInt64,
    block_time Int64,
    accounts Array(String),  -- All accounts for indexing
    fetched_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(fetched_at)
ORDER BY signature;

-- Bloom filter index for efficient account lookups
ALTER TABLE ore_stats.signatures 
ADD INDEX IF NOT EXISTS idx_sig_accounts accounts TYPE bloom_filter GRANULARITY 4;

-- Raw transactions v2: Full transaction data with account-based indexing
-- Note: signer is always accounts[0] (first account in the array)
CREATE TABLE IF NOT EXISTS ore_stats.raw_transactions_v2 (
    signature String,
    slot UInt64,
    block_time Int64,
    accounts Array(String),  -- All accounts including LUT-resolved (signer is accounts[0])
    raw_json String CODEC(ZSTD(3)),  -- Compressed JSON
    inserted_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(inserted_at)
ORDER BY signature;

-- Bloom filter index for efficient account lookups
ALTER TABLE ore_stats.raw_transactions_v2 
ADD INDEX IF NOT EXISTS idx_txn_accounts accounts TYPE bloom_filter GRANULARITY 4;


