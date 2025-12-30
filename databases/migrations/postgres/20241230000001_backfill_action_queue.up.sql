-- Backfill action queue for processing workflow steps
-- Persists across server restarts

CREATE TABLE backfill_action_queue (
    id BIGSERIAL PRIMARY KEY,
    round_id BIGINT NOT NULL,
    action VARCHAR(50) NOT NULL,  -- 'fetch_txns', 'reconstruct', 'finalize'
    status VARCHAR(20) NOT NULL DEFAULT 'pending',  -- 'pending', 'processing', 'completed', 'failed'
    queued_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for finding items by status
CREATE INDEX idx_backfill_queue_status ON backfill_action_queue(status);

-- Index for efficiently fetching next pending item (FIFO)
CREATE INDEX idx_backfill_queue_pending ON backfill_action_queue(status, queued_at) 
    WHERE status = 'pending';

-- Prevent duplicate pending/processing items for same round+action
CREATE UNIQUE INDEX idx_backfill_queue_unique_pending 
    ON backfill_action_queue(round_id, action) 
    WHERE status IN ('pending', 'processing');

-- Index for finding items by round
CREATE INDEX idx_backfill_queue_round ON backfill_action_queue(round_id);

-- Queue control state (paused flag, etc.)
CREATE TABLE backfill_queue_control (
    id INTEGER PRIMARY KEY DEFAULT 1 CHECK (id = 1),  -- Single row table
    paused BOOLEAN NOT NULL DEFAULT false,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Initialize with default state
INSERT INTO backfill_queue_control (paused) VALUES (false);

