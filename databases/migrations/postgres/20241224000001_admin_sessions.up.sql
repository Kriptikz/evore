-- Admin Sessions
-- Shared authentication between ore-stats and crank servers

CREATE TABLE admin_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    token_hash VARCHAR(64) NOT NULL UNIQUE,  -- SHA256 of session token
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    ip_address INET NOT NULL,
    user_agent TEXT
);

-- Index for token lookup
CREATE INDEX idx_admin_sessions_token ON admin_sessions(token_hash);

-- Index for cleanup of expired sessions
CREATE INDEX idx_admin_sessions_expires ON admin_sessions(expires_at);

