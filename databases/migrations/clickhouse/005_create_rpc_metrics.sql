-- ClickHouse Migration 005: RPC Metrics
-- Detailed usage tracking for ore-stats and crank RPC calls

-- Individual RPC request logs with full details
CREATE TABLE IF NOT EXISTS ore_stats.rpc_requests (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    -- Source identification
    program LowCardinality(String),         -- 'ore-stats', 'crank'
    provider LowCardinality(String),        -- 'helius', 'triton', 'quicknode', etc.
    api_key_id LowCardinality(String),      -- Short identifier for the key
    
    -- Request details
    method LowCardinality(String),          -- 'getAccountInfo', 'getProgramAccountsV2', etc.
    target_type LowCardinality(String),     -- 'board', 'round', 'treasury', 'miner', 'token', 'slot', 'balance', 'program'
    target_address String DEFAULT '',        -- Pubkey being queried (if applicable)
    
    -- Batch info
    is_batch UInt8 DEFAULT 0,
    batch_size UInt16 DEFAULT 1,
    
    -- Pagination info (for paginated calls like getProgramAccountsV2)
    is_paginated UInt8 DEFAULT 0,
    page_number UInt16 DEFAULT 0,           -- Which page of results (0 = first/only)
    cursor String DEFAULT '',                -- Pagination cursor if used
    
    -- Response details
    status LowCardinality(String),          -- 'success', 'error', 'timeout', 'rate_limited', 'not_found'
    error_code String DEFAULT '',
    error_message String DEFAULT '',
    
    -- Result counts (for multi-result queries)
    result_count UInt32 DEFAULT 0,          -- Number of items returned
    
    -- Timing (milliseconds)
    duration_ms UInt32,
    
    -- Data sizes (bytes)
    request_size UInt32 DEFAULT 0,
    response_size UInt32 DEFAULT 0,
    
    -- Rate limit info
    rate_limit_remaining Int32 DEFAULT -1,
    rate_limit_reset Int32 DEFAULT -1
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, method, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- Aggregated RPC metrics per minute (for dashboards)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rpc_metrics_minute
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(minute)
ORDER BY (minute, program, provider, method, target_type)
AS SELECT
    toStartOfMinute(timestamp) AS minute,
    program,
    provider,
    method,
    target_type,
    
    -- Counts
    count() AS total_requests,
    countIf(status = 'success') AS success_count,
    countIf(status = 'error') AS error_count,
    countIf(status = 'timeout') AS timeout_count,
    countIf(status = 'rate_limited') AS rate_limited_count,
    countIf(status = 'not_found') AS not_found_count,
    
    -- Batch stats
    sum(batch_size) AS total_operations,
    sum(result_count) AS total_results,
    
    -- Timing
    sum(duration_ms) AS total_duration_ms,
    count() AS duration_count,  -- For computing avg later
    max(duration_ms) AS max_duration_ms,
    min(duration_ms) AS min_duration_ms,
    
    -- Data transfer
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes
    
FROM ore_stats.rpc_requests
GROUP BY minute, program, provider, method, target_type;

-- Hourly rollup for longer retention
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rpc_metrics_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (hour, program, provider, method, target_type)
TTL hour + INTERVAL 365 DAY
AS SELECT
    toStartOfHour(timestamp) AS hour,
    program,
    provider,
    method,
    target_type,
    
    count() AS total_requests,
    countIf(status = 'success') AS success_count,
    countIf(status = 'error') AS error_count,
    countIf(status = 'timeout') AS timeout_count,
    countIf(status = 'rate_limited') AS rate_limited_count,
    countIf(status = 'not_found') AS not_found_count,
    
    sum(batch_size) AS total_operations,
    sum(result_count) AS total_results,
    
    sum(duration_ms) AS total_duration_ms,
    count() AS duration_count,
    max(duration_ms) AS max_duration_ms,
    
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes
    
FROM ore_stats.rpc_requests
GROUP BY hour, program, provider, method, target_type;

-- Daily summary for long-term trends
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rpc_metrics_daily
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(day)
ORDER BY (day, program, provider)
TTL day + INTERVAL 3 YEAR
AS SELECT
    toDate(timestamp) AS day,
    program,
    provider,
    
    count() AS total_requests,
    countIf(status = 'success') AS success_count,
    countIf(status = 'error') AS error_count,
    countIf(status = 'rate_limited') AS rate_limited_count,
    
    sum(batch_size) AS total_operations,
    sum(result_count) AS total_results,
    
    sum(duration_ms) AS total_duration_ms,
    count() AS duration_count,
    
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes,
    
    uniqExact(method) AS unique_methods
    
FROM ore_stats.rpc_requests
GROUP BY day, program, provider;

-- WebSocket connection events
CREATE TABLE IF NOT EXISTS ore_stats.ws_events (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    program LowCardinality(String),
    provider LowCardinality(String),
    subscription_type LowCardinality(String),  -- 'slot', 'program', 'account'
    subscription_key String DEFAULT '',
    
    event LowCardinality(String),              -- 'connecting', 'connected', 'disconnected', 'error'
    
    error_message String DEFAULT '',
    disconnect_reason String DEFAULT '',
    
    uptime_seconds UInt32 DEFAULT 0,
    messages_received UInt64 DEFAULT 0,
    reconnect_count UInt16 DEFAULT 0
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, subscription_type, timestamp)
TTL timestamp + INTERVAL 14 DAY;

-- WebSocket throughput samples
CREATE TABLE IF NOT EXISTS ore_stats.ws_throughput (
    timestamp DateTime DEFAULT now(),
    
    program LowCardinality(String),
    provider LowCardinality(String),
    subscription_type LowCardinality(String),
    
    messages_received UInt32,
    bytes_received UInt64,
    avg_process_time_us UInt32 DEFAULT 0
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, timestamp)
TTL timestamp + INTERVAL 7 DAY;
