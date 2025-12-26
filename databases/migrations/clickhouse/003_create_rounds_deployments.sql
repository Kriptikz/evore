-- ClickHouse Migration 003: Create Rounds and Deployments Tables
-- Core mining data - append-only, immutable after insert

-- Rounds table (one row per finalized round)
CREATE TABLE IF NOT EXISTS ore_stats.rounds (
    round_id UInt64,
    
    -- Round timing
    expires_at UInt64,          -- slot when round expires
    start_slot UInt64,          -- first deployment slot (0 if unknown)
    end_slot UInt64,            -- last deployment slot (0 if unknown)
    
    -- Hash data
    slot_hash FixedString(32),  -- 32 bytes raw
    winning_square UInt8,       -- 0-99, calculated from slot_hash
    
    -- Participants
    rent_payer String,          -- pubkey who initialized the round
    top_miner String,           -- pubkey selected as top miner (random weighted or split)
    top_miner_reward UInt64,    -- lamports bonus for top miner
    
    -- Round totals (all in lamports)
    total_deployed UInt64,      -- total SOL deployed by all miners
    total_vaulted UInt64,       -- total SOL vaulted this round  
    total_winnings UInt64,      -- total rewards distributed
    
    -- Motherlode (round-specific, may be distributed)
    motherlode_balance UInt64,  -- motherlode size at round end
    motherlode_hit UInt8,       -- 0 or 1
    
    -- Stats
    total_deployments UInt32,   -- count of deployment entries
    unique_miners UInt32,       -- count of unique miners
    
    -- Metadata
    finalized_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(finalized_at)
ORDER BY round_id;

-- Deployments table (one row per miner deployment per round)
CREATE TABLE IF NOT EXISTS ore_stats.deployments (
    round_id UInt64,
    miner_pubkey String,        -- base58 pubkey (easier for queries/display)
    
    -- Deployment data
    square_id UInt8,            -- which square (0-99)
    amount UInt64,              -- SOL deployed (lamports)
    deployed_slot UInt64,       -- 0 if unknown from websocket mismatch
    
    -- Miner state at deployment (for historical tracking)
    unclaimed_ore UInt64,       -- miner's unclaimed ore at time of deployment
    
    -- Calculated rewards (at finalization)
    ore_earned UInt64,          -- ORE reward (atomic units)
    sol_earned UInt64,          -- SOL reward (lamports, from top miner bonus or motherlode)
    
    -- Flags
    is_winner UInt8,            -- 0 or 1, deployed on winning square
    is_top_miner UInt8,         -- 0 or 1, selected as top miner (random weighted or split)
    
    -- Timestamp
    recorded_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY intDiv(round_id, 10000)  -- ~10k rounds per partition
ORDER BY (round_id, miner_pubkey, square_id);

-- Treasury snapshots (historical treasury state)
-- Can take multiple snapshots, keyed by auto-increment id
CREATE TABLE IF NOT EXISTS ore_stats.treasury_snapshots (
    -- Treasury state
    balance UInt64,             -- current treasury balance (lamports)
    motherlode UInt64,          -- current motherlode (lamports)
    total_staked UInt64,        -- total ORE staked
    total_unclaimed UInt64,     -- total unclaimed ORE
    total_refined UInt64,       -- total refined ORE
    
    -- Optional round association (0 if not tied to a specific round)
    round_id UInt64 DEFAULT 0,
    
    -- Timestamp
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(created_at)
ORDER BY created_at;

-- Miner snapshots (point-in-time miner state, stored at round end)
CREATE TABLE IF NOT EXISTS ore_stats.miner_snapshots (
    round_id UInt64,            -- which round this snapshot is for
    miner_pubkey String,        -- base58 pubkey
    
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

-- Raw transactions for historical round reconstruction
-- Store once, never re-fetch from RPC
CREATE TABLE IF NOT EXISTS ore_stats.raw_transactions (
    -- Transaction identification
    signature String,           -- base58 transaction signature (unique)
    slot UInt64,
    block_time Int64,           -- unix timestamp (can be 0 if not available)
    
    -- Round association (if known, 0 otherwise)
    round_id UInt64 DEFAULT 0,
    
    -- Transaction type for quick filtering
    tx_type LowCardinality(String),  -- 'deploy', 'automate', 'reset', 'claim', 'other'
    
    -- The raw transaction JSON (compressed)
    raw_json String CODEC(ZSTD(3)),
    
    -- Parsed key fields for indexing (extracted from raw_json)
    signer String,              -- first signer pubkey
    authority String DEFAULT '',-- miner authority if applicable
    
    -- Metadata
    inserted_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(inserted_at)
PARTITION BY intDiv(slot, 1000000)  -- ~1M slots per partition (~5 days)
ORDER BY (signature);

-- Index for finding transactions by round
ALTER TABLE ore_stats.raw_transactions ADD INDEX idx_round_id round_id TYPE minmax GRANULARITY 4;

-- Index for finding transactions by type within a slot range
ALTER TABLE ore_stats.raw_transactions ADD INDEX idx_tx_type tx_type TYPE set(10) GRANULARITY 4;

-- Automation states (snapshot of miner automation config at round boundaries)
-- Used to quickly rebuild deployment calculations without re-fetching automate transactions
CREATE TABLE IF NOT EXISTS ore_stats.automation_states (
    -- Which miner and at what point in time
    authority String,           -- miner authority pubkey
    round_id UInt64,            -- automation state as of the START of this round
    
    -- Automation config (matches AutomationCache/ReconstructedAutomation)
    active UInt8,               -- 0 = closed/manual, 1 = open/automated
    executor String,            -- who can execute the automation
    amount UInt64,              -- lamports per square
    fee UInt64,                 -- executor fee
    strategy UInt8,             -- 0 = Preferred, 1 = Random
    mask UInt64,                -- bitmask for squares (or num_squares for Random)
    
    -- Last known state update
    last_updated_slot UInt64,   -- slot of the last Automate tx that set this state
    
    -- Metadata
    created_at DateTime64(3) DEFAULT now64(3)
) ENGINE = ReplacingMergeTree(created_at)
ORDER BY (authority, round_id);

-- Miner totals (materialized view for fast all-time leaderboards)
-- Automatically updates when deployments are inserted
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

-- Dictionary for fast round lookups (avoids JOINs)
CREATE DICTIONARY IF NOT EXISTS ore_stats.rounds_dict (
    round_id UInt64,
    expires_at UInt64,
    winning_square UInt8,
    top_miner String,
    top_miner_reward UInt64,
    total_deployed UInt64,
    total_winnings UInt64,
    motherlode_balance UInt64,
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
