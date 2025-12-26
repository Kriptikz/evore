//! ClickHouse client for ore-stats metrics and historical data.
//!
//! This module provides batched inserts for:
//! - Request logs and server metrics
//! - Rounds and deployments (append-only, immutable)
//! - Treasury and miner snapshots
//! - RPC usage metrics

use std::time::Duration;

use clickhouse::{Client, Row, inserter::Inserter};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClickHouseError {
    #[error("ClickHouse error: {0}")]
    Client(#[from] clickhouse::error::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
}

/// ClickHouse client wrapper with connection pooling and batched inserts.
#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    /// Create a new ClickHouse client.
    /// 
    /// # Arguments
    /// * `url` - ClickHouse HTTP URL (e.g., "http://localhost:8123")
    /// * `database` - Database name (e.g., "ore_stats")
    /// * `user` - Username
    /// * `password` - Password
    pub fn new(url: &str, database: &str, user: &str, password: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database(database)
            .with_user(user)
            .with_password(password);
        
        Self { client }
    }
    
    /// Create from environment variables.
    /// Expects: CLICKHOUSE_URL, CLICKHOUSE_DB, CLICKHOUSE_USER, CLICKHOUSE_PASSWORD
    pub fn from_env() -> Result<Self, ClickHouseError> {
        let url = std::env::var("CLICKHOUSE_URL")
            .map_err(|_| ClickHouseError::Config("CLICKHOUSE_URL not set".into()))?;
        let database = std::env::var("CLICKHOUSE_DB")
            .unwrap_or_else(|_| "ore_stats".to_string());
        let user = std::env::var("CLICKHOUSE_USER")
            .unwrap_or_else(|_| "default".to_string());
        let password = std::env::var("CLICKHOUSE_PASSWORD")
            .unwrap_or_default();
        
        Ok(Self::new(&url, &database, &user, &password))
    }
    
    /// Get the underlying client for custom queries.
    pub fn client(&self) -> &Client {
        &self.client
    }
    
    // ========== Request Logs ==========
    
    /// Create an inserter for request logs.
    /// Flushes every 1000 rows or 1 second, whichever comes first.
    pub fn request_logs_inserter(&self) -> Result<Inserter<RequestLog>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RequestLog>("request_logs")?
            .with_max_rows(1000)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert a single request log (for immediate writes, prefer inserter for batching).
    pub async fn insert_request_log(&self, log: RequestLog) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("request_logs")?;
        insert.write(&log).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Server Metrics ==========
    
    /// Insert server metrics snapshot.
    pub async fn insert_server_metrics(&self, metrics: ServerMetrics) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("server_metrics")?;
        insert.write(&metrics).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Rounds ==========
    
    /// Insert a finalized round (from live tracker or backfill).
    pub async fn insert_round(&self, round: RoundInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rounds")?;
        insert.write(&round).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Insert multiple rounds (for batch backfill from external API).
    pub async fn insert_rounds(&self, rounds: Vec<RoundInsert>) -> Result<(), ClickHouseError> {
        if rounds.is_empty() {
            return Ok(());
        }
        let mut insert = self.client.insert("rounds")?;
        for r in rounds {
            insert.write(&r).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    /// Check if a round exists (for skipping duplicates during backfill).
    pub async fn round_exists(&self, round_id: u64) -> Result<bool, ClickHouseError> {
        let count: u64 = self.client
            .query("SELECT count() FROM rounds WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(count > 0)
    }
    
    /// Get the oldest round ID in the database.
    pub async fn get_oldest_round_id(&self) -> Result<Option<u64>, ClickHouseError> {
        let result: Option<u64> = self.client
            .query("SELECT min(round_id) FROM rounds")
            .fetch_optional()
            .await?;
        Ok(result)
    }
    
    // ========== Deployments ==========
    
    /// Create an inserter for deployments.
    /// Flushes every 500 rows or 500ms, whichever comes first.
    pub fn deployments_inserter(&self) -> Result<Inserter<DeploymentInsert>, ClickHouseError> {
        let inserter = self.client
            .inserter::<DeploymentInsert>("deployments")?
            .with_max_rows(500)
            .with_period(Some(Duration::from_millis(500)));
        Ok(inserter)
    }
    
    /// Insert multiple deployments at once.
    pub async fn insert_deployments(&self, deployments: Vec<DeploymentInsert>) -> Result<(), ClickHouseError> {
        if deployments.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("deployments")?;
        for d in deployments {
            insert.write(&d).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    // ========== Treasury Snapshots ==========
    
    /// Insert a treasury snapshot.
    pub async fn insert_treasury_snapshot(&self, snapshot: TreasurySnapshot) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("treasury_snapshots")?;
        insert.write(&snapshot).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Miner Snapshots ==========
    
    /// Create an inserter for miner snapshots.
    pub fn miner_snapshots_inserter(&self) -> Result<Inserter<MinerSnapshot>, ClickHouseError> {
        let inserter = self.client
            .inserter::<MinerSnapshot>("miner_snapshots")?
            .with_max_rows(1000)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert multiple miner snapshots at once.
    pub async fn insert_miner_snapshots(&self, snapshots: Vec<MinerSnapshot>) -> Result<(), ClickHouseError> {
        if snapshots.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("miner_snapshots")?;
        for s in snapshots {
            insert.write(&s).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    // ========== RPC Metrics ==========
    
    /// Create an inserter for RPC request metrics.
    pub fn rpc_metrics_inserter(&self) -> Result<Inserter<RpcRequestInsert>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RpcRequestInsert>("rpc_requests")?
            .with_max_rows(100)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert a single RPC metric (for immediate logging).
    pub async fn insert_rpc_metric(&self, metric: RpcRequestInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rpc_requests")?;
        insert.write(&metric).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== RPC Metrics Queries ==========
    
    /// Get RPC metrics summary for the last N hours, grouped by provider and method.
    pub async fn get_rpc_summary(&self, hours: u32) -> Result<Vec<RpcSummaryRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    program,
                    provider,
                    method,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    sum(timeout_count) AS timeout_count,
                    sum(rate_limited_count) AS rate_limited_count,
                    avg(avg_duration_ms) AS avg_duration_ms,
                    max(max_duration_ms) AS max_duration_ms,
                    sum(total_request_bytes) AS total_request_bytes,
                    sum(total_response_bytes) AS total_response_bytes
                FROM rpc_metrics_hourly
                WHERE hour > now() - INTERVAL ? HOUR
                GROUP BY program, provider, method
                ORDER BY total_requests DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC metrics by provider for the last N hours.
    pub async fn get_rpc_by_provider(&self, hours: u32) -> Result<Vec<RpcProviderRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    program,
                    provider,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    sum(rate_limited_count) AS rate_limited_count,
                    avg(avg_duration_ms) AS avg_duration_ms,
                    sum(total_request_bytes) AS total_request_bytes,
                    sum(total_response_bytes) AS total_response_bytes
                FROM rpc_metrics_hourly
                WHERE hour > now() - INTERVAL ? HOUR
                GROUP BY program, provider
                ORDER BY total_requests DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC errors for the last N hours.
    pub async fn get_rpc_errors(&self, hours: u32, limit: u32) -> Result<Vec<RpcErrorRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    program,
                    provider,
                    method,
                    status,
                    error_code,
                    error_message,
                    duration_ms
                FROM rpc_requests
                WHERE timestamp > now() - INTERVAL ? HOUR
                  AND status != 'success'
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC metrics time series for the last N hours (minute granularity).
    pub async fn get_rpc_timeseries(&self, hours: u32) -> Result<Vec<RpcTimeseriesRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    minute,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    avg(avg_duration_ms) AS avg_duration_ms
                FROM rpc_metrics_minute
                WHERE minute > now() - INTERVAL ? HOUR
                GROUP BY minute
                ORDER BY minute ASC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get daily RPC summary for the last N days.
    pub async fn get_rpc_daily(&self, days: u32) -> Result<Vec<RpcDailyRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    day,
                    program,
                    provider,
                    total_requests,
                    success_count,
                    error_count,
                    rate_limited_count,
                    avg_duration_ms,
                    total_request_bytes,
                    total_response_bytes
                FROM rpc_metrics_daily
                WHERE day > today() - INTERVAL ? DAY
                ORDER BY day DESC, total_requests DESC
            "#)
            .bind(days)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Rate Limit Events ==========
    
    /// Insert a rate limit event.
    pub async fn insert_rate_limit_event(&self, event: RateLimitEvent) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rate_limit_events")?;
        insert.write(&event).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Raw Transactions (Historical Backfill Only) ==========
    
    /// Create an inserter for raw transactions.
    /// Flushes every 100 rows or 500ms for efficient batch inserts.
    pub fn raw_transactions_inserter(&self) -> Result<Inserter<RawTransaction>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RawTransaction>("raw_transactions")?
            .with_max_rows(100)
            .with_period(Some(Duration::from_millis(500)));
        Ok(inserter)
    }
    
    /// Insert multiple raw transactions (for batch processing).
    pub async fn insert_raw_transactions(&self, txs: Vec<RawTransaction>) -> Result<(), ClickHouseError> {
        if txs.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("raw_transactions")?;
        for tx in txs {
            insert.write(&tx).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    /// Get all raw transactions for a round (for reconstruction).
    pub async fn get_raw_transactions_for_round(&self, round_id: u64) -> Result<Vec<RawTransaction>, ClickHouseError> {
        let results = self.client
            .query("SELECT * FROM raw_transactions FINAL WHERE round_id = ? ORDER BY slot ASC")
            .bind(round_id)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get raw transactions count for a round.
    pub async fn get_raw_transaction_count(&self, round_id: u64) -> Result<u32, ClickHouseError> {
        let count: u64 = self.client
            .query("SELECT count() FROM raw_transactions WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(count as u32)
    }
    
    // ========== Automation States ==========
    
    /// Insert an automation state snapshot.
    pub async fn insert_automation_state(&self, state: AutomationStateInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("automation_states")?;
        insert.write(&state).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Insert multiple automation states.
    pub async fn insert_automation_states(&self, states: Vec<AutomationStateInsert>) -> Result<(), ClickHouseError> {
        if states.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("automation_states")?;
        for s in states {
            insert.write(&s).await?;
        }
        insert.end().await?;
        Ok(())
    }
}

// ========== Row Types ==========

/// Request log entry for HTTP requests.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RequestLog {
    pub endpoint: String,
    pub method: String,
    pub status_code: u16,
    pub duration_ms: u32,
    pub ip_hash: String,
    #[serde(default)]
    pub user_agent: String,
}

/// Server metrics snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ServerMetrics {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_error: u64,
    pub latency_p50: f32,
    pub latency_p95: f32,
    pub latency_p99: f32,
    pub latency_avg: f32,
    pub active_connections: u32,
    pub memory_used: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Round insert data (finalized round).
/// Used for both live tracking and external API backfill.
/// 
/// For backfill from external API, some fields are defaulted:
/// - slot_hash: all zeros
/// - expires_at: ts + ~24 hours in slots  
/// - top_miner_reward: 100000000000 (1 ORE)
/// - rent_payer: empty string
/// - motherlode_hit: 1 if motherlode > 0
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RoundInsert {
    pub round_id: u64,
    
    // Timing
    pub expires_at: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    
    // Hash data
    pub slot_hash: [u8; 32],
    pub winning_square: u8,
    
    // Participants  
    pub rent_payer: String,
    pub top_miner: String,
    pub top_miner_reward: u64,
    
    // Totals
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    
    // Motherlode
    pub motherlode: u64,
    pub motherlode_hit: u8,
    
    // Stats
    pub total_deployments: u32,
    pub unique_miners: u32,
    
    // Source tracking
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "live".to_string()
}

impl RoundInsert {
    /// Create a RoundInsert from external API data with sensible defaults.
    pub fn from_backfill(
        round_id: u64,
        start_slot: u64,
        end_slot: u64,
        winning_square: u8,
        top_miner: String,
        total_deployed: u64,
        total_vaulted: u64,
        total_winnings: u64,
        motherlode: u64,
        unique_miners: u32,
        timestamp_secs: u64,
    ) -> Self {
        // Approximate expires_at: ts + 24 hours worth of slots (~216000 at 400ms/slot)
        let expires_at = end_slot.saturating_add(216000);
        
        Self {
            round_id,
            expires_at,
            start_slot,
            end_slot,
            slot_hash: [0u8; 32],  // Unknown for backfill
            winning_square,
            rent_payer: String::new(),  // Unknown for backfill
            top_miner,
            top_miner_reward: 100_000_000_000,  // 1 ORE in atomic units
            total_deployed,
            total_vaulted,
            total_winnings,
            motherlode,
            motherlode_hit: if motherlode > 0 { 1 } else { 0 },
            total_deployments: 0,  // Will be updated after reconstruction
            unique_miners,
            source: "backfill".to_string(),
        }
    }
}

/// Deployment insert data.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct DeploymentInsert {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub square_id: u8,
    pub amount: u64,
    pub deployed_slot: u64,  // 0 if unknown from websocket mismatch
    pub ore_earned: u64,
    pub sol_earned: u64,
    pub is_winner: u8,
    pub is_top_miner: u8,
}

/// Treasury snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct TreasurySnapshot {
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
    #[serde(default)]
    pub round_id: u64,
}

/// Miner snapshot at round end.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct MinerSnapshot {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub unclaimed_ore: u64,
    pub refined_ore: u64,
    pub lifetime_sol: u64,
    pub lifetime_ore: u64,
}

/// RPC request metrics.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcRequestInsert {
    pub program: String,
    pub provider: String,
    pub api_key_id: String,
    pub method: String,
    pub is_batch: u8,
    pub batch_size: u32,
    pub status: String,
    pub duration_ms: u32,
    pub request_size: u64,
    pub response_size: u64,
    pub rate_limit_remaining: i32,
}

/// Rate limit event for admin monitoring.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RateLimitEvent {
    pub ip_hash: String,
    pub endpoint: String,
    pub requests_in_window: u32,
    pub window_seconds: u16,
}

/// Raw transaction for historical rebuild.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RawTransaction {
    pub signature: String,
    pub slot: u64,
    pub block_time: i64,
    #[serde(default)]
    pub round_id: u64,
    pub tx_type: String,
    pub raw_json: String,
    pub signer: String,
    #[serde(default)]
    pub authority: String,
}

/// Automation state snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct AutomationStateInsert {
    pub authority: String,
    pub round_id: u64,
    pub active: u8,
    pub executor: String,
    pub amount: u64,
    pub fee: u64,
    pub strategy: u8,
    pub mask: u64,
    pub last_updated_slot: u64,
}

// ============================================================================
// RPC Metrics Query Results
// ============================================================================

/// Summary row for RPC metrics (grouped by program, provider, method).
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcSummaryRow {
    pub program: String,
    pub provider: String,
    pub method: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub rate_limited_count: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u32,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
}

/// Provider-level summary row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcProviderRow {
    pub program: String,
    pub provider: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub rate_limited_count: u64,
    pub avg_duration_ms: f64,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
}

/// Individual RPC error row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcErrorRow {
    pub timestamp: String, // DateTime64 comes as string
    pub program: String,
    pub provider: String,
    pub method: String,
    pub status: String,
    pub error_code: String,
    pub error_message: String,
    pub duration_ms: u32,
}

/// Time series row for RPC metrics (minute granularity).
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcTimeseriesRow {
    pub minute: String, // DateTime comes as string
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub avg_duration_ms: f64,
}

/// Daily summary row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcDailyRow {
    pub day: String, // Date comes as string
    pub program: String,
    pub provider: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub rate_limited_count: u64,
    pub avg_duration_ms: f64,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_log_serialization() {
        let log = RequestLog {
            endpoint: "/api/round".to_string(),
            method: "GET".to_string(),
            status_code: 200,
            duration_ms: 15,
            ip_hash: "abc123".to_string(),
            user_agent: "test-agent".to_string(),
        };
        
        // Just verify it serializes without error
        let _ = serde_json::to_string(&log).unwrap();
    }
}

