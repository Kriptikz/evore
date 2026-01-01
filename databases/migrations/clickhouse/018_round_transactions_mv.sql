-- Materialized View to track transaction counts per round
-- Updates automatically when new transactions are inserted into raw_transactions_v2
-- Backfill is handled by the Rust round_addresses backfill task

-- Step 1: Create the target table (SummingMergeTree to aggregate counts from MV)
CREATE TABLE IF NOT EXISTS ore_stats.round_transaction_stats (
    round_id UInt64,
    address String,
    transaction_count UInt64,
    min_slot UInt64,
    max_slot UInt64,
    updated_at DateTime64(3) DEFAULT now64(3)
) ENGINE = SummingMergeTree((transaction_count))
ORDER BY (round_id, address);

-- Step 2: Create MV to capture NEW transactions going forward
-- Each new transaction insert triggers a row with count=1
-- SummingMergeTree will aggregate these on merge
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.round_transactions_mv
TO ore_stats.round_transaction_stats
AS
SELECT 
    ra.round_id AS round_id,
    ra.address AS address,
    1 AS transaction_count,
    v2.slot AS min_slot,
    v2.slot AS max_slot
FROM ore_stats.raw_transactions_v2 AS v2
INNER JOIN ore_stats.round_addresses AS ra ON has(v2.accounts, ra.address);

-- Note: Historical data backfill is handled by the Rust background task
-- in round_addresses.rs (spawn_round_addresses_backfill)
