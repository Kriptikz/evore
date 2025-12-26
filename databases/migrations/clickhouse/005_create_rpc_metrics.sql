-- ClickHouse Migration 005: RPC Metrics
-- Usage tracking for ore-stats and crank to identify inefficiencies and issues

-- Individual RPC request logs
CREATE TABLE IF NOT EXISTS ore_stats.rpc_requests (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    -- Source identification
    program LowCardinality(String),         -- 'ore-stats', 'crank'
    provider LowCardinality(String),        -- 'helius', 'triton', 'quicknode', etc.
    api_key_id LowCardinality(String),      -- Short identifier for the key (e.g., 'helius-primary', 'helius-backup')
    
    -- Request details
    method LowCardinality(String),          -- 'getBalance', 'getMultipleAccounts', 'sendTransaction', etc.
    is_batch UInt8 DEFAULT 0,               -- 1 if batch request
    batch_size UInt16 DEFAULT 1,            -- Number of requests in batch
    
    -- Response details
    status LowCardinality(String),          -- 'success', 'error', 'timeout', 'rate_limited'
    error_code String DEFAULT '',           -- RPC error code if failed
    error_message String DEFAULT '',        -- Error message (truncated)
    
    -- Timing (milliseconds)
    duration_ms UInt32,
    
    -- Data sizes (bytes)
    request_size UInt32 DEFAULT 0,
    response_size UInt32 DEFAULT 0,
    
    -- Rate limit info (if available from headers)
    rate_limit_remaining Int32 DEFAULT -1,  -- -1 if unknown
    rate_limit_reset Int32 DEFAULT -1       -- Seconds until reset, -1 if unknown
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, method, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- Aggregated RPC metrics per minute (for dashboards)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rpc_metrics_minute
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(minute)
ORDER BY (minute, program, provider, method)
AS SELECT
    toStartOfMinute(timestamp) AS minute,
    program,
    provider,
    method,
    
    -- Counts
    count() AS total_requests,
    countIf(status = 'success') AS success_count,
    countIf(status = 'error') AS error_count,
    countIf(status = 'timeout') AS timeout_count,
    countIf(status = 'rate_limited') AS rate_limited_count,
    
    -- Batch stats
    sum(batch_size) AS total_operations,  -- Actual operations (batch unrolled)
    
    -- Timing
    avg(duration_ms) AS avg_duration_ms,
    max(duration_ms) AS max_duration_ms,
    quantile(0.95)(duration_ms) AS p95_duration_ms,
    
    -- Data transfer
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes
    
FROM ore_stats.rpc_requests
GROUP BY minute, program, provider, method;

-- Hourly rollup for longer retention
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.rpc_metrics_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (hour, program, provider, method)
TTL hour + INTERVAL 365 DAY
AS SELECT
    toStartOfHour(timestamp) AS hour,
    program,
    provider,
    method,
    
    count() AS total_requests,
    countIf(status = 'success') AS success_count,
    countIf(status = 'error') AS error_count,
    countIf(status = 'timeout') AS timeout_count,
    countIf(status = 'rate_limited') AS rate_limited_count,
    
    sum(batch_size) AS total_operations,
    
    avg(duration_ms) AS avg_duration_ms,
    max(duration_ms) AS max_duration_ms,
    
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes
    
FROM ore_stats.rpc_requests
GROUP BY hour, program, provider, method;

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
    avg(duration_ms) AS avg_duration_ms,
    
    sum(request_size) AS total_request_bytes,
    sum(response_size) AS total_response_bytes,
    
    -- Unique methods used
    uniqExact(method) AS unique_methods
    
FROM ore_stats.rpc_requests
GROUP BY day, program, provider;

-- WebSocket connection metrics (for tracking WS stability)
CREATE TABLE IF NOT EXISTS ore_stats.ws_events (
    timestamp DateTime64(3) DEFAULT now64(3),
    
    program LowCardinality(String),
    provider LowCardinality(String),
    subscription_type LowCardinality(String),  -- 'account', 'slot', 'program'
    subscription_key String DEFAULT '',         -- Pubkey or identifier being watched
    
    event LowCardinality(String),              -- 'connected', 'disconnected', 'error', 'reconnecting'
    
    -- For disconnects/errors
    error_message String DEFAULT '',
    disconnect_reason String DEFAULT '',        -- 'timeout', 'server_closed', 'error', 'manual'
    
    -- Connection stats at time of event
    uptime_seconds UInt32 DEFAULT 0,           -- How long was this connection up
    messages_received UInt64 DEFAULT 0,        -- Total messages on this connection
    reconnect_count UInt16 DEFAULT 0           -- How many times has this reconnected
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, subscription_type, timestamp)
TTL timestamp + INTERVAL 14 DAY;

-- WebSocket message throughput (sampled, not every message)
CREATE TABLE IF NOT EXISTS ore_stats.ws_throughput (
    timestamp DateTime DEFAULT now(),
    
    program LowCardinality(String),
    provider LowCardinality(String),
    subscription_type LowCardinality(String),
    
    -- Message counts in this sample period
    messages_received UInt32,
    bytes_received UInt64,
    
    -- Latency (if measurable)
    avg_process_time_us UInt32 DEFAULT 0       -- Microseconds to process message
    
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (program, provider, timestamp)
TTL timestamp + INTERVAL 7 DAY;
