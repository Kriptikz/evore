CREATE TABLE IF NOT EXISTS ore_stats.deployment_automation_states (
    round_id UInt64,
    miner_pubkey LowCardinality(String),
    authority_pubkey LowCardinality(String),
    deploy_signature String,
    deploy_ix_index UInt8,
    deploy_slot UInt64,
    
    automation_found Bool,
    automation_active Bool,
    automation_amount UInt64,
    automation_mask UInt64,
    automation_strategy UInt8,
    automation_fee UInt64,
    automation_executor LowCardinality(String),
    
    automate_signature String,
    automate_ix_index UInt8,
    automate_slot UInt64,
    
    txns_searched UInt32,
    pages_fetched UInt32,
    fetch_duration_ms UInt64,
    
    automation_balance UInt64 DEFAULT 0,
    is_partial_deploy Bool DEFAULT false,
    actual_squares_deployed UInt8 DEFAULT 0,
    actual_mask UInt64 DEFAULT 0,
    total_sol_spent UInt64 DEFAULT 0,
    
    created_at DateTime DEFAULT now()
)
ENGINE = ReplacingMergeTree()
PARTITION BY intDiv(round_id, 10000)
ORDER BY (round_id, miner_pubkey, deploy_signature, deploy_ix_index);

ALTER TABLE ore_stats.deployment_automation_states
ADD INDEX idx_authority (authority_pubkey) TYPE set(1000) GRANULARITY 1;

ALTER TABLE ore_stats.deployment_automation_states
ADD INDEX idx_found (automation_found) TYPE minmax GRANULARITY 1;
