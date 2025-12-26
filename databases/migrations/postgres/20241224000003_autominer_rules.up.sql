-- Autominer Rules
-- Configuration for automated mining deployments

CREATE TABLE autominer_rules (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    
    -- Rule conditions
    enabled BOOLEAN NOT NULL DEFAULT true,
    priority INTEGER NOT NULL DEFAULT 0,  -- Higher = checked first
    
    -- Miner filters (NULL = apply to all)
    miner_pubkey VARCHAR(44),  -- Base58 pubkey or NULL for all miners
    min_balance_lamports BIGINT,
    
    -- Deployment settings
    target_square SMALLINT,  -- 0-255 or NULL for auto
    power_multiplier DECIMAL(5,2) DEFAULT 1.0,
    
    -- Timing
    min_slots_remaining INTEGER DEFAULT 0,
    max_slots_remaining INTEGER,
    
    -- Limits
    max_deploys_per_round INTEGER,
    cooldown_rounds INTEGER DEFAULT 0,
    
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(50) DEFAULT 'admin'
);

-- Index for enabled rules lookup
CREATE INDEX idx_autominer_rules_enabled ON autominer_rules(enabled, priority DESC);

-- Index for miner-specific rules
CREATE INDEX idx_autominer_rules_miner ON autominer_rules(miner_pubkey) WHERE miner_pubkey IS NOT NULL;

-- Trigger to update updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_autominer_rules_updated_at
    BEFORE UPDATE ON autominer_rules
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

