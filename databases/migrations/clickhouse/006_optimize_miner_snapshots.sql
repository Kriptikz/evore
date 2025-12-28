-- Migration 006: Optimize miner_snapshots table for better compression
-- 
-- PROBLEM:
-- The original table had ORDER BY (round_id, miner_pubkey)
-- This caused poor compression because:
--   1. miner_pubkey is high-cardinality (44-char base58 strings)
--   2. Sorting by pubkey scatters similar values, defeating compression
--   3. Data stored: ~10k miners × ~1440 rounds/day = 14M+ rows/day
--
-- SOLUTION:
--   1. Remove miner_pubkey from ORDER BY - just ORDER BY round_id
--   2. Add CODEC(ZSTD(1)) for better compression on numeric columns
--   3. Keep LowCardinality for miner_pubkey (still helps, just not in ORDER BY)
--
-- EXPLANATION OF SETTINGS:
--
-- PARTITION BY intDiv(round_id, 10000):
--   - Creates a new partition every 10,000 rounds (~7 days at 1 round/min)
--   - Benefits: Can DROP entire partitions to delete old data quickly
--   - Queries automatically skip irrelevant partitions
--
-- ORDER BY round_id:
--   - Data is physically sorted by round_id on disk
--   - All miners for round 12345 are stored together
--   - Compression is MUCH better because adjacent rows have same round_id
--   - Queries filtering by round_id are very fast
--   - We DON'T add miner_pubkey because:
--     a) It's high cardinality → kills compression
--     b) We rarely query "get miner X across all rounds" for snapshots
--     c) The main query is "get all miners from round Y"
--
-- CODEC(ZSTD(1)):
--   - ZSTD compression level 1 (fast, good compression)
--   - Applied to numeric columns that compress well
--   - Delta encoding would be bad here since values aren't sequential
--
-- ============================================================================

-- Drop the old table (WARNING: This deletes all data!)
DROP TABLE IF EXISTS ore_stats.miner_snapshots;

-- Recreate with optimized structure
CREATE TABLE IF NOT EXISTS ore_stats.miner_snapshots (
    -- Round this snapshot was taken at
    round_id UInt64 CODEC(Delta, ZSTD(1)),
    
    -- Miner address (LowCardinality helps since same miners appear each round)
    miner_pubkey LowCardinality(String),
    
    -- Miner state at round end (with compression codecs)
    unclaimed_ore UInt64 CODEC(ZSTD(1)),    -- unclaimed ORE (atomic units, 11 decimals)
    refined_ore UInt64 CODEC(ZSTD(1)),       -- refined ORE (atomic units)
    lifetime_sol UInt64 CODEC(ZSTD(1)),      -- lifetime SOL earned (lamports)
    lifetime_ore UInt64 CODEC(ZSTD(1)),      -- lifetime ORE earned (atomic units)
    
    -- Timestamp
    created_at DateTime64(3) DEFAULT now64(3) CODEC(Delta, ZSTD(1))
) ENGINE = MergeTree()
PARTITION BY intDiv(round_id, 10000)  -- New partition every ~7 days
ORDER BY round_id;                     -- ONLY round_id - better compression!

-- Add a comment explaining the table purpose
-- This table stores a COMPLETE snapshot of ALL miner accounts at each round end.
-- This happens ~once per minute (when a round ends).
-- 
-- Typical query patterns:
--   1. SELECT * FROM miner_snapshots WHERE round_id = X (get all miners at round X)
--   2. SELECT * FROM miner_snapshots WHERE round_id = (SELECT max(round_id) FROM miner_snapshots)
--
-- For queries like "get miner X's history", use the deployments table instead,
-- as it tracks per-round participation with much less data.

