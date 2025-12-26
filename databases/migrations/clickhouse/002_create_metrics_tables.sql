-- ClickHouse Migration 002: Create Metrics Tables
-- Request logs, server metrics snapshots, and rate limit events

-- Request logs (append-only, high volume)
CREATE TABLE IF NOT EXISTS ore_stats.request_logs (
    timestamp DateTime64(3) DEFAULT now64(3),
    endpoint String,
    method LowCardinality(String),
    status_code UInt16,
    duration_ms UInt32,
    ip_hash String,  -- SHA256 of IP for privacy
    user_agent String DEFAULT ''
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (endpoint, timestamp)
TTL timestamp + INTERVAL 30 DAY;

-- Server metrics snapshots (periodic snapshots)
CREATE TABLE IF NOT EXISTS ore_stats.server_metrics (
    timestamp DateTime DEFAULT now(),
    
    -- Request counters (since last snapshot)
    requests_total UInt64,
    requests_success UInt64,
    requests_error UInt64,
    
    -- Latency percentiles (ms)
    latency_p50 Float32,
    latency_p95 Float32,
    latency_p99 Float32,
    latency_avg Float32,
    
    -- Active connections
    active_connections UInt32,
    
    -- Memory usage (bytes)
    memory_used UInt64,
    
    -- Cache stats
    cache_hits UInt64,
    cache_misses UInt64
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY timestamp
TTL timestamp + INTERVAL 90 DAY;

-- Rate limit events (for admin monitoring)
CREATE TABLE IF NOT EXISTS ore_stats.rate_limit_events (
    timestamp DateTime64(3) DEFAULT now64(3),
    ip_hash String,
    endpoint String,
    requests_in_window UInt32,
    window_seconds UInt16
) ENGINE = MergeTree()
PARTITION BY toYYYYMM(timestamp)
ORDER BY (ip_hash, timestamp)
TTL timestamp + INTERVAL 7 DAY;

-- IP activity hourly aggregate (materialized view)
CREATE MATERIALIZED VIEW IF NOT EXISTS ore_stats.ip_activity_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (hour, ip_hash, endpoint)
AS SELECT
    toStartOfHour(timestamp) AS hour,
    ip_hash,
    endpoint,
    count() AS request_count,
    countIf(status_code >= 400) AS error_count,
    countIf(status_code = 429) AS rate_limit_count,
    avg(duration_ms) AS avg_duration_ms
FROM ore_stats.request_logs
GROUP BY hour, ip_hash, endpoint;

