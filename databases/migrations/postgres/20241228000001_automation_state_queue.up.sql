-- =============================================================================
-- Automation State Processing Queue
-- =============================================================================
-- Tracks deployments that need their automation state fetched.
-- Processed by a background task that searches transaction history.

CREATE TYPE automation_queue_status AS ENUM (
    'pending',
    'processing', 
    'completed',
    'failed'
);

CREATE TABLE automation_state_queue (
    id SERIAL PRIMARY KEY,
    
    -- Deployment identification
    round_id BIGINT NOT NULL,
    miner_pubkey VARCHAR(44) NOT NULL,
    authority_pubkey VARCHAR(44) NOT NULL,
    automation_pda VARCHAR(44) NOT NULL,
    deploy_signature VARCHAR(88) NOT NULL,
    deploy_ix_index SMALLINT NOT NULL DEFAULT 0,
    deploy_slot BIGINT NOT NULL,
    
    -- Processing status
    status automation_queue_status NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    
    -- Statistics (filled after completion)
    txns_searched INTEGER,
    pages_fetched INTEGER,
    fetch_duration_ms BIGINT,
    automation_found BOOLEAN,
    
    -- Priority (lower = higher priority)
    priority INTEGER NOT NULL DEFAULT 1000,
    
    -- Timestamps
    created_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
    started_at TIMESTAMP WITH TIME ZONE,
    completed_at TIMESTAMP WITH TIME ZONE,
    
    -- Uniqueness constraint
    UNIQUE (round_id, deploy_signature, deploy_ix_index)
);

-- Indexes for efficient queue processing
CREATE INDEX idx_automation_queue_status ON automation_state_queue(status, priority, created_at);
CREATE INDEX idx_automation_queue_round ON automation_state_queue(round_id);
CREATE INDEX idx_automation_queue_authority ON automation_state_queue(authority_pubkey);

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_automation_queue_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_automation_queue_updated_at
    BEFORE UPDATE ON automation_state_queue
    FOR EACH ROW
    EXECUTE FUNCTION update_automation_queue_updated_at();

