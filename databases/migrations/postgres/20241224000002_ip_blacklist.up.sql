-- IP Blacklist
-- Persistent storage for blocked IPs (e.g., failed admin login attempts)

CREATE TABLE ip_blacklist (
    id SERIAL PRIMARY KEY,
    ip_address INET NOT NULL UNIQUE,
    reason VARCHAR(255) NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0,
    blocked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,  -- NULL = permanent block
    created_by VARCHAR(50) DEFAULT 'system'  -- 'system' or 'admin'
);

-- Index for IP lookup
CREATE INDEX idx_ip_blacklist_ip ON ip_blacklist(ip_address);

-- Failed login attempts tracking (before blocking)
CREATE TABLE failed_login_attempts (
    id SERIAL PRIMARY KEY,
    ip_address INET NOT NULL,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    endpoint VARCHAR(100) NOT NULL DEFAULT '/admin/login'
);

-- Index for counting recent attempts
CREATE INDEX idx_failed_attempts_ip_time ON failed_login_attempts(ip_address, attempted_at);

-- Auto-cleanup old attempts (keep last 24 hours)
-- This would typically be done by a scheduled job

