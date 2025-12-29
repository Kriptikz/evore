-- Queue for transaction parsing jobs
-- Admin triggers this, background worker processes

CREATE TABLE transaction_parse_queue (
    id SERIAL PRIMARY KEY,
    round_id BIGINT NOT NULL UNIQUE,
    
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, completed, failed
    
    -- Results
    txns_found INT,
    deploys_queued INT,
    errors_count INT,
    
    -- Timing
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    
    -- Error info
    last_error TEXT
);

CREATE INDEX idx_txn_parse_queue_status ON transaction_parse_queue (status, created_at);

