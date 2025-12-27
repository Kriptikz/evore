-- ClickHouse Migration 006: Rename ip_hash to client_ip
-- Store real client IPs instead of hashed IPs for admin monitoring

-- Rename column in request_logs
ALTER TABLE ore_stats.request_logs RENAME COLUMN ip_hash TO client_ip;

-- Rename column in rate_limit_events
ALTER TABLE ore_stats.rate_limit_events RENAME COLUMN ip_hash TO client_ip;

-- Drop and recreate the materialized view (columns can't be renamed in MVs)
DROP VIEW IF EXISTS ore_stats.ip_activity_hourly;

CREATE MATERIALIZED VIEW ore_stats.ip_activity_hourly
ENGINE = SummingMergeTree()
PARTITION BY toYYYYMM(hour)
ORDER BY (hour, client_ip, endpoint)
AS SELECT
    toStartOfHour(timestamp) AS hour,
    client_ip,
    endpoint,
    count() AS request_count,
    countIf(status_code >= 400) AS error_count,
    countIf(status_code = 429) AS rate_limit_count,
    avg(duration_ms) AS avg_duration_ms
FROM ore_stats.request_logs
GROUP BY hour, client_ip, endpoint;

