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

use crate::app_state::apply_refined_ore_fix;
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
/// 
/// Uses two RPC clients:
/// - `flux_client` - for getAccountInfo, getMultipleAccounts (GMA), getProgramAccounts (GPA)
/// - `helius_client` - for getBalance, getSlot, getSignatureStatuses, and as backup
pub struct AppRpc {
    /// Flux RPC client for account data fetching (GMA, GPA, getAccountInfo)
    flux_client: RpcClient,
    flux_url: String,
    flux_provider_name: String,
    flux_api_key_id: String,
    flux_last_request_at: RwLock<Instant>,
    
    /// Helius RPC client for other calls (getBalance, getSlot, etc.)
    helius_client: RpcClient,
    helius_url: String,
    helius_provider_name: String,
    helius_api_key_id: String,
    helius_last_request_at: RwLock<Instant>,
    
    // Metrics tracking
    clickhouse: Option<Arc<ClickHouseClient>>,
    program_name: String,
}

impl AppRpc {
    /// Create a new AppRpc instance with both Helius and Flux RPC clients.
    /// 
    /// # Arguments
    /// * `helius_rpc_url` - The Helius RPC URL (with or without https:// prefix)
    /// * `flux_rpc_url` - The Flux RPC URL (with or without https:// prefix)
    /// * `clickhouse` - Optional ClickHouse client for metrics logging
    pub fn new(helius_rpc_url: String, flux_rpc_url: String, clickhouse: Option<Arc<ClickHouseClient>>) -> Self {
        // Normalize Helius URL
        let helius_url = if helius_rpc_url.starts_with("http") {
            helius_rpc_url.clone()
        } else {
            format!("https://{}", helius_rpc_url)
        };
        
        // Normalize Flux URL
        let flux_url = if flux_rpc_url.starts_with("http") {
            flux_rpc_url.clone()
        } else {
            format!("https://{}", flux_rpc_url)
        };
        
        // Extract provider names and API keys for metrics
        let helius_provider_name = extract_provider_name(&helius_url);
        let helius_api_key_id = extract_api_key_id(&helius_url);
        let flux_provider_name = extract_provider_name(&flux_url);
        let flux_api_key_id = extract_api_key_id(&flux_url);
        
        // Create Helius client
        let helius_client = RpcClient::new_with_commitment(
            helius_url.clone(),
            CommitmentConfig { commitment: CommitmentLevel::Confirmed },
        );
        
        // Create Flux client
        let flux_client = RpcClient::new_with_commitment(
            flux_url.clone(),
            CommitmentConfig { commitment: CommitmentLevel::Confirmed },
        );
        
        tracing::info!(
            "AppRpc initialized: Helius={} ({}), Flux={} ({})",
            helius_provider_name, helius_url, flux_provider_name, flux_url
        );
        
        Self {
            flux_client,
            flux_url,
            flux_provider_name,
            flux_api_key_id,
            flux_last_request_at: RwLock::new(Instant::now()),
            helius_client,
            helius_url,
            helius_provider_name,
            helius_api_key_id,
            helius_last_request_at: RwLock::new(Instant::now()),
            clickhouse,
            program_name: "ore-stats".to_string(),
        }
    }
    
    /// Rate limit for Flux client: wait if we're calling too fast
    async fn rate_limit_flux(&self) {
        let mut last = self.flux_last_request_at.write().await;
        let elapsed = last.elapsed().as_millis() as u64;
        if elapsed < MIN_REQUEST_INTERVAL_MS {
            let wait = MIN_REQUEST_INTERVAL_MS - elapsed;
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        *last = Instant::now();
    }
    
    /// Rate limit for Helius client: wait if we're calling too fast
    async fn rate_limit_helius(&self) {
        let mut last = self.helius_last_request_at.write().await;
        let elapsed = last.elapsed().as_millis() as u64;
        if elapsed < MIN_REQUEST_INTERVAL_MS {
            let wait = MIN_REQUEST_INTERVAL_MS - elapsed;
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        *last = Instant::now();
    }
    
    /// Log successful RPC call to ClickHouse for Flux
    async fn log_flux_success(&self, ctx: &RpcContext, duration_ms: u32, result_count: u32, response_size: u32) {
        self.log_success_internal(&self.flux_provider_name, &self.flux_api_key_id, ctx, duration_ms, result_count, response_size).await;
    }
    
    /// Log successful RPC call to ClickHouse for Helius
    async fn log_helius_success(&self, ctx: &RpcContext, duration_ms: u32, result_count: u32, response_size: u32) {
        self.log_success_internal(&self.helius_provider_name, &self.helius_api_key_id, ctx, duration_ms, result_count, response_size).await;
    }
    
    /// Internal log success implementation
    async fn log_success_internal(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32, result_count: u32, response_size: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                provider_name,
                api_key_id,
                &ctx.method,
                &ctx.target_type,
            )
            .with_target(&ctx.target_address)
            .with_batch(ctx.batch_size)
            .success(duration_ms, result_count, response_size);
            
            tracing::debug!(
                "Logging RPC metric: provider={} method={} target_type={} result_count={} duration_ms={}",
                provider_name, ctx.method, ctx.target_type, result_count, duration_ms
            );
            
            // Fire and forget - don't block on metrics logging
            let ch = ch.clone();
            let method = ctx.method.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::error!("Failed to log RPC metric for {}: {}", method, e);
                }
            });
        } else {
            tracing::debug!("No ClickHouse client - skipping RPC metric for {}", ctx.method);
        }
    }
    
    /// Log error RPC call to ClickHouse for Flux
    async fn log_flux_error(&self, ctx: &RpcContext, duration_ms: u32, error: &str) {
        self.log_error_internal(&self.flux_provider_name, &self.flux_api_key_id, ctx, duration_ms, error).await;
    }
    
    /// Log error RPC call to ClickHouse for Helius
    async fn log_helius_error(&self, ctx: &RpcContext, duration_ms: u32, error: &str) {
        self.log_error_internal(&self.helius_provider_name, &self.helius_api_key_id, ctx, duration_ms, error).await;
    }
    
    /// Internal log error implementation
    async fn log_error_internal(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32, error: &str) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                provider_name,
                api_key_id,
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
    
    /// Log not found RPC call to ClickHouse for Flux
    async fn log_flux_not_found(&self, ctx: &RpcContext, duration_ms: u32) {
        self.log_not_found_internal(&self.flux_provider_name, &self.flux_api_key_id, ctx, duration_ms).await;
    }
    
    /// Log not found RPC call to ClickHouse for Helius
    async fn log_helius_not_found(&self, ctx: &RpcContext, duration_ms: u32) {
        self.log_not_found_internal(&self.helius_provider_name, &self.helius_api_key_id, ctx, duration_ms).await;
    }
    
    /// Internal log not found implementation
    async fn log_not_found_internal(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                &self.program_name,
                provider_name,
                api_key_id,
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
    
    // ========== ORE Account Fetching (via Flux RPC) ==========
    
    /// Get the Board account (uses Flux RPC)
    pub async fn get_board(&self) -> Result<Board> {
        self.rate_limit_flux().await;
        let start = Instant::now();
        
        let address = board_pda().0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "board".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.flux_client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_flux_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let board = Board::try_from_bytes(&data)?;
                Ok(*board)
            }
            Err(e) => {
                self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get a Round account by ID (uses Flux RPC)
    pub async fn get_round(&self, round_id: u64) -> Result<Round> {
        self.rate_limit_flux().await;
        let start = Instant::now();
        
        let address = round_pda(round_id).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "round".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.flux_client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_flux_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let round = Round::try_from_bytes(&data)?;
                Ok(*round)
            }
            Err(e) => {
                self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get the Treasury account (uses Flux RPC)
    pub async fn get_treasury(&self) -> Result<Treasury> {
        self.rate_limit_flux().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "treasury".to_string(),
            target_address: TREASURY_ADDRESS.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.flux_client.get_account_data(&TREASURY_ADDRESS).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_flux_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let treasury = Treasury::try_from_bytes(&data)?;
                Ok(*treasury)
            }
            Err(e) => {
                self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get a Miner account by authority (uses Flux RPC)
    pub async fn get_miner(&self, authority: &Pubkey) -> Result<Option<Miner>> {
        self.rate_limit_flux().await;
        let start = Instant::now();
        
        let address = miner_pda(*authority).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "miner".to_string(),
            target_address: authority.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.flux_client.get_account_data(&address).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(data) => {
                self.log_flux_success(&ctx, duration_ms, 1, data.len() as u32).await;
                let miner = Miner::try_from_bytes(&data)?;
                Ok(Some(*miner))
            }
            Err(e) => {
                // Account not found is not an error for optional miner
                if e.to_string().contains("AccountNotFound") {
                    self.log_flux_not_found(&ctx, duration_ms).await;
                    Ok(None)
                } else {
                    self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                    Err(e.into())
                }
            }
        }
    }
    
    // ========== Other RPC calls (via Helius RPC) ==========
    
    /// Get SOL balance for an account (uses Helius RPC)
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.rate_limit_helius().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getBalance".to_string(),
            target_type: "balance".to_string(),
            target_address: pubkey.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.helius_client.get_balance(pubkey).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(balance) => {
                self.log_helius_success(&ctx, duration_ms, 1, 8).await;
                Ok(balance)
            }
            Err(e) => {
                self.log_helius_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get current slot (uses Helius RPC)
    pub async fn get_slot(&self) -> Result<u64> {
        self.rate_limit_helius().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getSlot".to_string(),
            target_type: "slot".to_string(),
            target_address: String::new(),
            is_batch: false,
            batch_size: 1,
        };
        
        let result = self.helius_client.get_slot().await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(slot) => {
                self.log_helius_success(&ctx, duration_ms, 1, 8).await;
                Ok(slot)
            }
            Err(e) => {
                self.log_helius_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get multiple accounts at once (uses Flux RPC - GMA)
    pub async fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Vec<u8>>>> {
        self.rate_limit_flux().await;
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getMultipleAccounts".to_string(),
            target_type: "batch".to_string(),
            target_address: String::new(),
            is_batch: true,
            batch_size: pubkeys.len() as u16,
        };
        
        let result = self.flux_client.get_multiple_accounts(pubkeys).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(accounts) => {
                let response_size: u32 = accounts.iter()
                    .filter_map(|a| a.as_ref())
                    .map(|a| a.data.len() as u32)
                    .sum();
                let found_count = accounts.iter().filter(|a| a.is_some()).count() as u32;
                    
                self.log_flux_success(&ctx, duration_ms, found_count, response_size).await;
                
                Ok(accounts.into_iter().map(|a| a.map(|acc| acc.data)).collect())
            }
            Err(e) => {
                self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get signature statuses for transaction confirmations (uses Helius RPC)
    /// Returns Vec<Option<SignatureStatus>> where:
    /// - None = not found yet
    /// - Some(status) = found with confirmation details
    pub async fn get_signature_statuses(&self, signatures: &[String]) -> Result<Vec<Option<SignatureStatus>>> {
        self.rate_limit_helius().await;
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
            .post(&self.helius_url)
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
                    self.log_helius_error(&ctx, duration_ms, &error_msg).await;
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
                self.log_helius_success(&ctx, duration_ms, confirmed_count, 0).await;
                Ok(statuses)
            }
            Err(e) => {
                self.log_helius_error(&ctx, duration_ms, &e.to_string()).await;
                Err(e.into())
            }
        }
    }
    
    /// Get all ORE Miner accounts using standard getProgramAccounts RPC (uses Flux RPC - GPA)
    /// This is the source of truth for miner data - more reliable than v2 endpoint
    /// Returns a HashMap keyed by authority pubkey string
    /// If treasury is provided, applies refined_ore calculation immediately
    pub async fn get_all_miners_gpa(&self, treasury: Option<&Treasury>) -> Result<std::collections::HashMap<String, Miner>> {
        self.get_all_miners_gpa_with_client(treasury, false).await
    }
    
    /// Get all ORE Miner accounts using either Flux or Helius RPC
    /// `use_helius` - if true, uses Helius client; if false, uses Flux client
    pub async fn get_all_miners_gpa_with_client(&self, treasury: Option<&Treasury>, use_helius: bool) -> Result<std::collections::HashMap<String, Miner>> {
        use solana_client::rpc_config::{RpcProgramAccountsConfig, RpcAccountInfoConfig};
        use solana_client::rpc_filter::RpcFilterType;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let (client, provider_name) = if use_helius {
            self.rate_limit_helius().await;
            (&self.helius_client, "Helius")
        } else {
            self.rate_limit_flux().await;
            (&self.flux_client, "Flux")
        };
        
        let start = Instant::now();
        
        let ctx = RpcContext {
            method: "getProgramAccounts".to_string(),
            target_type: "miner".to_string(),
            target_address: evore::ore_api::PROGRAM_ID.to_string(),
            is_batch: true,
            batch_size: 0, // Unknown until we get results
        };
        
        // Filter for Miner accounts by size (size_of::<Miner>() + 8 for discriminator)
        let miner_size = std::mem::size_of::<Miner>() as u64 + 8;
        
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(miner_size)]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };
        
        let result = client
            .get_program_accounts_with_config(&evore::ore_api::PROGRAM_ID, config)
            .await;
        
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(accounts) => {
                let mut miners = std::collections::HashMap::new();
                let mut total_size = 0u32;
                
                for (_pubkey, account) in &accounts {
                    total_size += account.data.len() as u32;
                    
                    if let Ok(miner) = Miner::try_from_bytes(&account.data) {
                        // Apply refined_ore fix if treasury is available
                        let fixed_miner = if let Some(t) = treasury {
                            apply_refined_ore_fix(miner, t)
                        } else {
                            *miner
                        };
                        miners.insert(fixed_miner.authority.to_string(), fixed_miner);
                    }
                }
                
                tracing::info!(
                    "GPA miners snapshot ({}): {} accounts fetched, {} miners parsed in {}ms",
                    provider_name, accounts.len(), miners.len(), duration_ms
                );
                
                if use_helius {
                    self.log_helius_success(&ctx, duration_ms, miners.len() as u32, total_size).await;
                } else {
                    self.log_flux_success(&ctx, duration_ms, miners.len() as u32, total_size).await;
                }
                Ok(miners)
            }
            Err(e) => {
                if use_helius {
                    self.log_helius_error(&ctx, duration_ms, &e.to_string()).await;
                } else {
                    self.log_flux_error(&ctx, duration_ms, &e.to_string()).await;
                }
                Err(e.into())
            }
        }
    }
}

/// Extract provider name from RPC URL for metrics
fn extract_provider_name(url: &str) -> String {
    if url.contains("helius") {
        "helius".to_string()
    } else if url.contains("flux") {
        "flux".to_string()
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
        assert_eq!(extract_provider_name("https://rpc.flux.dev"), "flux");
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


