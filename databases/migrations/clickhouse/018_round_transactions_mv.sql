-- Materialized View to track transaction counts per round
-- Updates automatically when new transactions are inserted into raw_transactions_v2

-- Target table for the materialized view
CREATE TABLE IF NOT EXISTS ore_stats.round_transaction_stats (
    round_id UInt64,
    address String,
    transaction_count UInt64,
    min_slot UInt64,
    max_slot UInt64,
    updated_at DateTime64(3) DEFAULT now64(3)
) ENGINE = SummingMergeTree()
ORDER BY (round_id, address);

-- Materialized View: watches raw_transactions_v2 and joins with round_addresses
-- For each new transaction, finds matching round(s) and updates the stats
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

-- Backfill existing data into the stats table
INSERT INTO ore_stats.round_transaction_stats (round_id, address, transaction_count, min_slot, max_slot)
SELECT 
    ra.round_id,
    ra.address,
    count() AS transaction_count,
    min(v2.slot) AS min_slot,
    max(v2.slot) AS max_slot
FROM ore_stats.raw_transactions_v2 AS v2 FINAL
INNER JOIN ore_stats.round_addresses AS ra FINAL ON has(v2.accounts, ra.address)
GROUP BY ra.round_id, ra.address;

