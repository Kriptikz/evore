-- ClickHouse Migration 004: Create Crank Tables
-- Crank server metrics and execution logs

-- Crank execution logs
CREATE TABLE IF NOT EXISTS crank.execution_logs (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    -- Execution context
    action LowCardinality(String),  -- 'deploy', 'claim', 'reset', etc.
    miner_pubkey FixedString(32),
    round_id UInt64,
    
    -- Result
    success UInt8,
    error_message String DEFAULT '',
    
    -- Timing
    duration_ms UInt32,
    
    -- Transaction details
    tx_signature String DEFAULT '',
    slot UInt64 DEFAULT 0
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (action, timestamp)
TTL timestamp + INTERVAL 90 DAY;

-- Crank transaction results
CREATE TABLE IF NOT EXISTS crank.tx_results (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    tx_signature String,
    slot UInt64,
    
    -- Status
    confirmed UInt8,
    error_code String DEFAULT '',
    
    -- Timing
    submit_to_confirm_ms UInt32,
    
    -- Cost
    compute_units UInt32,
    priority_fee UInt64
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY timestamp
TTL timestamp + INTERVAL 30 DAY;

-- Crank server metrics (periodic snapshots)
CREATE TABLE IF NOT EXISTS crank.server_metrics (
    timestamp DateTime DEFAULT now(),
    
    -- Execution stats (since last snapshot)
    executions_total UInt64,
    executions_success UInt64,
    executions_failed UInt64,
    
    -- Timing
    avg_execution_ms Float32,
    avg_confirm_ms Float32,
    
    -- Queue stats
    pending_deploys UInt32,
    pending_claims UInt32,
    
    -- Cost stats
    total_priority_fees UInt64,
    avg_compute_units Float32
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY timestamp
TTL timestamp + INTERVAL 90 DAY;

