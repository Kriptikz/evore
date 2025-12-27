//! AppRPC - Central RPC gateway with metrics tracking
//!
//! All RPC calls from ore-stats should go through this module.
//! Provides:
//! - Rate limiting
//! - Request/response timing
//! - Metrics logging to ClickHouse
//! - Error tracking

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use solana_sdk::pubkey::Pubkey;
use steel::AccountDeserialize;
use tokio::sync::RwLock;
use tokio::time::Instant;

use evore::ore_api::{
    Board, Miner, Round, Treasury,
    TREASURY_ADDRESS, board_pda, miner_pda, round_pda,
};

use crate::clickhouse::{ClickHouseClient, RpcRequestInsert};

/// Minimum time between RPC requests (rate limiting)
const MIN_REQUEST_INTERVAL_MS: u64 = 40; // 25 req/s max

/// RPC metrics context for a single request
#[derive(Debug, Clone)]
pub struct RpcContext {
    pub method: String,
    pub target_type: String,
    pub target_address: String,
    pub is_batch: bool,
    pub batch_size: u16,
}

/// Signature status from getSignatureStatuses RPC
#[derive(Debug, Clone)]
pub struct SignatureStatus {
    pub slot: Option<u64>,
    pub confirmations: Option<usize>,
    pub err: Option<String>,
    pub confirmation_status: Option<String>, // "processed", "confirmed", "finalized"
}

/// Central RPC gateway with metrics tracking
pub struct AppRpc {
    client: RpcClient,
    rpc_url: String,
    last_request_at: RwLock<Instant>,
    
    // Metrics tracking
    clickhouse: Option<Arc<ClickHouseClient>>,
    program_name: String,
    provider_name: String,
    api_key_id: String,
}

impl AppRpc {
    /// Create a new AppRpc instance.
    /// 
    /// # Arguments
    /// * `rpc_url` - The RPC URL (without https:// prefix)
    /// * `clickhouse` - Optional ClickHouse client for metrics logging
    pub fn new(rpc_url: String, clickhouse: Option<Arc<ClickHouseClient>>) -> Self {
        let full_url = if rpc_url.starts_with("http") {
            rpc_url.clone()
        } else {
            format!("https://{}", rpc_url)
        };
        
        // Extract provider name from URL for metrics
        let provider_name = extract_provider_name(&full_url);
        
        // Extract API key ID if present (for Helius URLs like xxx?api-key=abc)
        let api_key_id = extract_api_key_id(&full_url);
        
        let client = RpcClient::new_with_commitment(
            full_url.clone(),
            CommitmentConfig { commitment: CommitmentLevel::Confirmed },
        );
        
        Self {
            client,
            rpc_url: full_url,
            last_request_at: RwLock::new(Instant::now()),
            clickhouse,
            program_name: "ore-stats".to_string(),
            provider_name,
            api_key_id,
        }
    }
    
    /// Rate limit: wait if we're calling too fast
    async fn rate_limit(&self) {
        let mut last = self.last_request_at.write().await;
        let elapsed = last.elapsed().as_millis() as u64;
        if elapsed < MIN_REQUEST_INTERVAL_MS {
            let wait = MIN_REQUEST_INTERVAL_MS - elapsed;
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        *last = Instant::now();
    }
    
    /// Log successful RPC call to ClickHouse
    async fn log_success(&self, ctx: &RpcContext, duration_ms: u32, result_count: u32, response_size: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                &self.provider_name,
                &self.api_key_id,
                &ctx.method,
                &ctx.target_type,
            )
            .with_target(&ctx.target_address)
            .with_batch(ctx.batch_size)
            .success(duration_ms, result_count, response_size);
            
            // Fire and forget - don't block on metrics logging
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log RPC metrics: {}", e);
                }
            });
        }
    }
    
    /// Log error RPC call to ClickHouse
    async fn log_error(&self, ctx: &RpcContext, duration_ms: u32, error: &str) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                &self.provider_name,
                &self.api_key_id,
                &ctx.method,
                &ctx.target_type,
            )
            .with_target(&ctx.target_address)
            .with_batch(ctx.batch_size)
            .error(duration_ms, "", error);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log RPC metrics: {}", e);
                }
            });
        }
    }
    
    /// Log not found RPC call to ClickHouse
    async fn log_not_found(&self, ctx: &RpcContext, duration_ms: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                &self.provider_name,
                &self.api_key_id,
                &ctx.method,
                &ctx.target_type,
            )
            .with_target(&ctx.target_address)
            .with_batch(ctx.batch_size)
            .not_found(duration_ms);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log RPC metrics: {}", e);
                }
            });
        }
    }
    
    // ========== ORE Account Fetching ==========
    
    /// Get the Board account
    pub async fn get_board(&self) -> Result<Board> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let address = board_pda().0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "board".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let board = Board::try_from_bytes(&data)?;
                Ok(*board)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get a Round account by ID
    pub async fn get_round(&self, round_id: u64) -> Result<Round> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let address = round_pda(round_id).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "round".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let round = Round::try_from_bytes(&data)?;
                Ok(*round)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get the Treasury account
    pub async fn get_treasury(&self) -> Result<Treasury> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "treasury".to_string(),
            target_address: TREASURY_ADDRESS.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_account_data(&TREASURY_ADDRESS).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let treasury = Treasury::try_from_bytes(&data)?;
                Ok(*treasury)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get a Miner account by authority
    pub async fn get_miner(&self, authority: &Pubkey) -> Result<Option<Miner>> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let address = miner_pda(*authority).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "miner".to_string(),
            target_address: authority.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let miner = Miner::try_from_bytes(&data)?;
                Ok(Some(*miner))
            }
            Err(e) => {
                // Account not found is not an error for optional miner
                if e.to_string().contains("AccountNotFound") {
                    self.log_not_found(&ctx, duration_ms).await;
                    Ok(None)
                } else {
                    self.log_error(&ctx, duration_ms, &e.to_string()).await;
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get SOL balance for an account (for frontend RPC proxy)
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getBalance".to_string(),
            target_type: "balance".to_string(),
            target_address: pubkey.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_balance(pubkey).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(balance) => {
                self.log_success(&ctx, duration_ms, 1, 8).await;
                Ok(balance)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get current slot
    pub async fn get_slot(&self) -> Result<u64> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getSlot".to_string(),
            target_type: "slot".to_string(),
            target_address: String::new(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.client.get_slot().await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(slot) => {
                self.log_success(&ctx, duration_ms, 1, 8).await;
                Ok(slot)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get multiple accounts at once
    pub async fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Vec<u8>>>> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getMultipleAccounts".to_string(),
            target_type: "batch".to_string(),
            target_address: String::new(),
            is_batch: true,
            batch_size: pubkeys.len() as u16,
        };
        
        let result = self.client.get_multiple_accounts(pubkeys).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(accounts) => {
                let response_size: u32 = accounts.iter()
                    .filter_map(|a| a.as_ref())
                    .map(|a| a.data.len() as u32)
                    .sum();
                let found_count = accounts.iter().filter(|a| a.is_some()).count() as u32;
                    
                self.log_success(&ctx, duration_ms, found_count, response_size).await;
                
                Ok(accounts.into_iter().map(|a| a.map(|acc| acc.data)).collect())
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get signature statuses for transaction confirmations
    /// Returns Vec<Option<SignatureStatus>> where:
    /// - None = not found yet
    /// - Some(status) = found with confirmation details
    pub async fn get_signature_statuses(&self, signatures: &[String]) -> Result<Vec<Option<SignatureStatus>>> {
        self.rate_limit().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getSignatureStatuses".to_string(),
            target_type: "signature".to_string(),
            target_address: if signatures.len() == 1 { 
                signatures[0].clone()
            } else { 
                String::new() 
            },
            is_batch: signatures.len() > 1,
            batch_size: signatures.len() as u16,
        };
        
        // Use direct JSON-RPC call like the crank does
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [
                signatures,
                { "searchTransactionHistory": false }
            ]
        });
        
        let client = reqwest::Client::new();
        let response = client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await;
        
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match response {
            Ok(res) => {
                let json: serde_json::Value = res.json().await
                    .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;
                
                if let Some(error) = json.get("error") {
                    let error_msg = error.to_string();
                    self.log_error(&ctx, duration_ms, &error_msg).await;
                    return Err(anyhow::anyhow!("RPC error: {}", error_msg));
                }
                
                let statuses: Vec<Option<SignatureStatus>> = json["result"]["value"]
                    .as_array()
                    .map(|arr| {
                        arr.iter().map(|v| {
                            if v.is_null() {
                                None
                            } else {
                                Some(SignatureStatus {
                                    slot: v["slot"].as_u64(),
                                    confirmations: v["confirmations"].as_u64().map(|c| c as usize),
                                    err: v["err"].as_str().map(|s| s.to_string()),
                                    confirmation_status: v["confirmationStatus"].as_str().map(|s| s.to_string()),
                                })
                            }
                        }).collect()
                    })
                    .unwrap_or_default();
                
                let confirmed_count = statuses.iter().filter(|s| s.is_some()).count() as u32;
                self.log_success(&ctx, duration_ms, confirmed_count, 0).await;
                Ok(statuses)
            }
            Err(e) => {
                self.log_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
}

/// Extract provider name from RPC URL for metrics
fn extract_provider_name(url: &str) -> String {
    if url.contains("helius") {
        "helius".to_string()
    } else if url.contains("quicknode") {
        "quicknode".to_string()
    } else if url.contains("alchemy") {
        "alchemy".to_string()
    } else if url.contains("triton") {
        "triton".to_string()
    } else if url.contains("rpcpool") {
        "rpcpool".to_string()
    } else if url.contains("localhost") || url.contains("127.0.0.1") {
        "localhost".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extract API key ID from URL (e.g., for Helius ?api-key=xxx)
fn extract_api_key_id(url: &str) -> String {
    // Look for api-key or api_key parameter
    if let Some(idx) = url.find("api-key=").or_else(|| url.find("api_key=")) {
        let start = idx + 8;
        let end = url[start..].find('&').map(|i| start + i).unwrap_or(url.len());
        let key = &url[start..end];
        // Return first 8 chars as ID (don't log full key)
        if key.len() >= 8 {
            format!("{}...", &key[..8])
        } else {
            key.to_string()
        }
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_provider_name() {
        assert_eq!(extract_provider_name("https://mainnet.helius-rpc.com"), "helius");
        assert_eq!(extract_provider_name("https://api.quicknode.com/xxx"), "quicknode");
        assert_eq!(extract_provider_name("http://localhost:8899"), "localhost");
        assert_eq!(extract_provider_name("https://some-random-rpc.com"), "unknown");
    }
    
    #[test]
    fn test_extract_api_key_id() {
        assert_eq!(extract_api_key_id("https://rpc.helius.xyz?api-key=abcdefghij123"), "abcdefgh...");
        assert_eq!(extract_api_key_id("https://rpc.helius.xyz"), "");
        assert_eq!(extract_api_key_id("https://rpc.helius.xyz?api_key=12345678"), "12345678...");
    }
}

