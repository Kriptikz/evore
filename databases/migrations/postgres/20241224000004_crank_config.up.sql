-- Crank Configuration
-- Runtime configuration for the crank server

CREATE TABLE crank_config (
    key VARCHAR(100) PRIMARY KEY,
    value JSONB NOT NULL,
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_by VARCHAR(50) DEFAULT 'system'
);

-- Trigger to update updated_at
CREATE TRIGGER update_crank_config_updated_at
    BEFORE UPDATE ON crank_config
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert default configuration values
INSERT INTO crank_config (key, value, description) VALUES
    ('enabled', 'true', 'Master switch for crank operations'),
    ('max_concurrent_txs', '5', 'Maximum concurrent transactions'),
    ('priority_fee_strategy', '"dynamic"', 'Priority fee strategy: fixed, dynamic, aggressive'),
    ('priority_fee_base', '10000', 'Base priority fee in microlamports'),
    ('priority_fee_max', '100000', 'Maximum priority fee in microlamports'),
    ('retry_attempts', '3', 'Number of retry attempts for failed transactions'),
    ('retry_delay_ms', '500', 'Delay between retries in milliseconds'),
    ('deploy_batch_size', '10', 'Maximum miners to deploy per batch'),
    ('claim_batch_size', '20', 'Maximum claims to process per batch'),
    ('min_balance_sol', '0.1', 'Minimum SOL balance before alerting'),
    ('rpc_timeout_ms', '30000', 'RPC request timeout in milliseconds'),
    ('confirmation_commitment', '"confirmed"', 'Transaction confirmation commitment level');

-- Crank wallet management
CREATE TABLE crank_wallets (
    id SERIAL PRIMARY KEY,
    pubkey VARCHAR(44) NOT NULL UNIQUE,
    name VARCHAR(100),
    role VARCHAR(50) NOT NULL DEFAULT 'deployer',  -- 'deployer', 'claimer', 'fee_payer'
    enabled BOOLEAN NOT NULL DEFAULT true,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for active wallets
CREATE INDEX idx_crank_wallets_active ON crank_wallets(role, enabled) WHERE enabled = true;

