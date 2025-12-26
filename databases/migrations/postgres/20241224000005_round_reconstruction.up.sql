-- Round reconstruction status for admin workflow
-- Tracks the pipeline for historical round backfill

CREATE TABLE IF NOT EXISTS round_reconstruction_status (
    round_id BIGINT PRIMARY KEY,
    
    -- Pipeline status flags
    meta_fetched BOOLEAN DEFAULT FALSE,          -- round info from external API
    transactions_fetched BOOLEAN DEFAULT FALSE,  -- all txns stored in ClickHouse raw_transactions
    reconstructed BOOLEAN DEFAULT FALSE,         -- deployments calculated from txns
    verified BOOLEAN DEFAULT FALSE,              -- admin verified against external data
    finalized BOOLEAN DEFAULT FALSE,             -- deployments stored in ClickHouse deployments table
    
    -- Counts for progress tracking
    transaction_count INT DEFAULT 0,             -- number of txns fetched for this round
    deployment_count INT DEFAULT 0,              -- number of deployments reconstructed
    
    -- Verification notes (optional)
    verification_notes TEXT DEFAULT '',
    
    -- Timestamps for each stage
    meta_fetched_at TIMESTAMPTZ,
    transactions_fetched_at TIMESTAMPTZ,
    reconstructed_at TIMESTAMPTZ,
    verified_at TIMESTAMPTZ,
    finalized_at TIMESTAMPTZ,
    
    -- Last update
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Index for filtering by status
CREATE INDEX idx_reconstruction_status ON round_reconstruction_status (finalized, meta_fetched);

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_reconstruction_status_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_reconstruction_status
    BEFORE UPDATE ON round_reconstruction_status
    FOR EACH ROW
    EXECUTE FUNCTION update_reconstruction_status_updated_at();

