-- ClickHouse Migration 003: Create Rounds and Deployments Tables
-- Core mining data - append-only, immutable after insert

-- Rounds table (one row per finalized round)
-- Populated from:
--   1. Live tracker (real-time as rounds finalize) - full data
--   2. External API backfill (historical rounds) - some fields defaulted
CREATE TABLE IF NOT EXISTS ore_stats.rounds (
    round_id UInt64,
    
    -- Round timing
    expires_at UInt64,          -- slot when round expires (backfill: ts + 24h in slots)
    start_slot UInt64,          -- first deployment slot (0 if unknown)
    end_slot UInt64,            -- last deployment slot (0 if unknown)
    
    -- Hash data
    slot_hash FixedString(32),  -- 32 bytes raw (backfill: all zeros)
    winning_square UInt8,       -- 0-24, from slot_hash or API
    
    -- Participants
    rent_payer LowCardinality(String),  -- pubkey who initialized round (backfill: empty)
    top_miner LowCardinality(String),   -- pubkey selected as top miner
    top_miner_reward UInt64,    -- lamports bonus (backfill: 100000000000 = 1 ORE)
    
    -- Round totals (all in lamports)
    total_deployed UInt64,      -- total SOL deployed by all miners
    total_vaulted UInt64,       -- total SOL vaulted this round  
    total_winnings UInt64,      -- total SOL rewards distributed
    
    -- Motherlode
    motherlode UInt64,          -- motherlode size at round end
    motherlode_hit UInt8,       -- 0 or 1 (backfill: 1 if motherlode > 0)
    
    -- Stats
    total_deployments UInt32,   -- count of deployment entries (0 until reconstructed)
    unique_miners UInt32,       -- count of unique miners (num_winners from API)
    
    -- Source tracking
    source LowCardinality(String) DEFAULT 'live',  -- 'live' or 'backfill'
    
    -- Timestamp (from external API ts field or now())
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(created_at)
ORDER BY round_id;

-- Deployments table (one row per miner per square per round)
-- LowCardinality for miner_pubkey creates internal dictionary automatically
CREATE TABLE IF NOT EXISTS ore_stats.deployments (
    round_id UInt64,
    miner_pubkey LowCardinality(String),  -- base58 pubkey (auto-dictionary)
    
    -- Deployment data
    square_id UInt8,            -- which square (0-24)
    amount UInt64,              -- SOL deployed (lamports)
    deployed_slot UInt64,       -- 0 if unknown from websocket mismatch
    
    -- Calculated rewards (at finalization)
    ore_earned UInt64,          -- ORE reward (atomic units)
    sol_earned UInt64,          -- SOL reward (lamports)
    
    -- Flags
    is_winner UInt8,            -- 0 or 1, deployed on winning square
    is_top_miner UInt8,         -- 0 or 1, selected as top miner
    
    -- Timestamp
    recorded_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY intDiv(round_id, 10000)  -- ~10k rounds per partition
ORDER BY (round_id, miner_pubkey, square_id);

-- Treasury snapshots (historical treasury state, live tracking)
CREATE TABLE IF NOT EXISTS ore_stats.treasury_snapshots (
    balance UInt64,             -- current treasury balance (lamports)
    motherlode UInt64,          -- current motherlode (lamports)
    total_staked UInt64,        -- total ORE staked
    total_unclaimed UInt64,     -- total unclaimed ORE
    total_refined UInt64,       -- total refined ORE
    
    -- Optional round association (0 if periodic snapshot, round_id if at round end)
    round_id UInt64 DEFAULT 0,
    
    -- Timestamp
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY created_at;

-- Miner snapshots (point-in-time miner state, live tracking only)
-- NOT used for historical backfill
CREATE TABLE IF NOT EXISTS ore_stats.miner_snapshots (
    round_id UInt64,
    miner_pubkey LowCardinality(String),
    
    -- Miner state at round end
    unclaimed_ore UInt64,       -- unclaimed ORE (atomic units)
    refined_ore UInt64,         -- refined ORE (atomic units)
    lifetime_sol UInt64,        -- lifetime SOL earned (lamports)
    lifetime_ore UInt64,        -- lifetime ORE earned (atomic units)
    
    -- Timestamp
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY intDiv(round_id, 10000)
ORDER BY (round_id, miner_pubkey);

-- ============================================================================
-- HISTORICAL BACKFILL TABLES (only used for reconstructing past rounds)
-- These are NOT populated during live operation
-- ============================================================================

-- Raw transactions for historical round reconstruction
-- Store once per round during backfill, never re-fetch from RPC
CREATE TABLE IF NOT EXISTS ore_stats.raw_transactions (
    signature String,           -- base58 transaction signature (unique)
    slot UInt64,
    block_time Int64,           -- unix timestamp (can be 0 if not available)
    
    -- Round association
    round_id UInt64,
    
    -- Transaction type for quick filtering
    tx_type LowCardinality(String),  -- 'deploy', 'automate', 'reset', 'claim', 'other'
    
    -- The raw transaction JSON (compressed)
    raw_json String CODEC(ZSTD(3)),
    
    -- Parsed key fields for indexing
    signer LowCardinality(String),
    authority LowCardinality(String) DEFAULT '',  -- miner authority if applicable
    
    -- Metadata
    inserted_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY intDiv(round_id, 1000)  -- ~1k rounds per partition
ORDER BY (round_id, signature);

-- Index for finding transactions by type
ALTER TABLE ore_stats.raw_transactions ADD INDEX IF NOT EXISTS idx_tx_type tx_type TYPE set(10) GRANULARITY 4;

-- Automation states for historical reconstruction
-- Used to avoid re-fetching automate txns when rebuilding deployment calculations
-- After first backfill, subsequent rounds use last known state + new automate txns
CREATE TABLE IF NOT EXISTS ore_stats.automation_states (
    authority LowCardinality(String),  -- miner authority pubkey
    round_id UInt64,                   -- automation state as of START of this round
    
    -- Automation config
    active UInt8,               -- 0 = closed/manual, 1 = open/automated
    executor LowCardinality(String),
    amount UInt64,              -- lamports per square
    fee UInt64,                 -- executor fee
    strategy UInt8,             -- 0 = Random, 1 = Preferred, 2 = Discretionary
    mask UInt64,                -- bitmask for squares (or num_squares for Random)
    
    -- Last known state update
    last_updated_slot UInt64,   -- slot of the last Automate tx that set this state
    
    -- Metadata
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(created_at)
ORDER BY (authority, round_id);

-- ============================================================================
-- MATERIALIZED VIEWS (auto-updated on insert)
-- ============================================================================

-- Miner totals (fast all-time leaderboards)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.miner_totals_mv
ENGINE = SummingMergeTree()
ORDER BY miner_pubkey
AS SELECT
    miner_pubkey,
    count() AS total_deployments,
    uniqExact(round_id) AS rounds_played,
    sum(ore_earned) AS total_ore_earned,
    sum(sol_earned) AS total_sol_earned,
    sum(amount) AS total_sol_deployed,
    sum(is_winner) AS rounds_won,
    sum(is_top_miner) AS times_top_miner,
    sum(toInt64(sol_earned) - toInt64(amount)) AS net_sol_change,
    max(round_id) AS last_round_id,
    max(recorded_at) AS last_active
FROM ore_stats.deployments
GROUP BY miner_pubkey;

-- Miner daily stats (for charts/trends)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.miner_daily_stats
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY (day, miner_pubkey)
AS SELECT
    toDate(recorded_at) AS day,
    miner_pubkey,
    count() AS deployments,
    sum(ore_earned) AS ore_earned,
    sum(sol_earned) AS sol_earned,
    sum(amount) AS sol_deployed,
    sum(is_winner) AS wins
FROM ore_stats.deployments
GROUP BY day, miner_pubkey;

-- Round daily summary (for overview charts)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.round_daily_stats
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY day
AS SELECT
    toDate(created_at) AS day,
    count() AS rounds,
    sum(total_deployed) AS total_deployed,
    sum(total_winnings) AS total_winnings,
    sum(unique_miners) AS total_miners
FROM ore_stats.rounds
GROUP BY day;

-- ============================================================================
-- DICTIONARY (for fast round lookups without JOINs)
-- ============================================================================

CREATE DICTIONARY IF NOT EXISTS ore_stats.rounds_dict (
    round_id UInt64,
    winning_square UInt8,
    top_miner String,
    top_miner_reward UInt64,
    total_deployed UInt64,
    total_winnings UInt64,
    motherlode UInt64,
    motherlode_hit UInt8,
    total_deployments UInt32,
    unique_miners UInt32
) PRIMARY KEY round_id
SOURCE(CLICKHOUSE(
    HOST 'localhost'
    PORT 9000
    USER 'default'
    TABLE 'rounds'
    DB 'ore_stats'
))
LAYOUT(FLAT())
LIFETIME(MIN 60 MAX 120);
