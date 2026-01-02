//! AppRPC - Central RPC gateway with metrics tracking
//!
//! All RPC calls from ore-stats should go through this module.
//! Provides:
//! - Rate limiting per provider
//! - Round-robin retry with fallback across providers
//! - Round-robin distribution for transaction fetching (to maximize throughput)
//! - Request/response timing
//! - Metrics logging to ClickHouse
//! - Error tracking

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};

use crate::custom_rpc::CustomRpcClient;
use solana_sdk::pubkey::Pubkey;
use steel::AccountDeserialize;
use tokio::sync::RwLock;
use tokio::time::Instant;

use evore::ore_api::{
    Board, Miner, Round, Treasury,
    TREASURY_ADDRESS, MINT_ADDRESS, board_pda, miner_pda, round_pda,
};

use crate::app_state::apply_refined_ore_fix;
use crate::clickhouse::{ClickHouseClient, RpcRequestInsert};

/// Retry configuration
const MAX_RETRIES: usize = 10;
const RETRY_DELAY_MS: u64 = 500;

/// Rate limits per provider (milliseconds between requests)
const FLUX_MIN_INTERVAL_MS: u64 = 33;    // ~30 rps
const HELIUS_MIN_INTERVAL_MS: u64 = 40;  // ~25 rps
const TRITON_MIN_INTERVAL_MS: u64 = 20;  // ~50 rps

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

/// An RPC provider with its own client and rate limiter
pub struct RpcProvider {
    pub name: String,
    pub url: String,
    pub api_key_id: String,
    pub client: CustomRpcClient,
    pub last_request_at: RwLock<Instant>,
    pub min_request_interval_ms: u64,
}

impl RpcProvider {
    /// Create a new RPC provider
    pub fn new(name: &str, url: &str, min_interval_ms: u64) -> Self {
        // Normalize URL
        let normalized_url = if url.starts_with("http") {
            url.to_string()
        } else {
            format!("https://{}", url)
        };
        
        let client = CustomRpcClient::new(&normalized_url);
        let api_key_id = extract_api_key_id(&normalized_url);
        
        Self {
            name: name.to_string(),
            url: normalized_url,
            api_key_id,
            client,
            last_request_at: RwLock::new(Instant::now()),
            min_request_interval_ms: min_interval_ms,
        }
    }
    
    /// Rate limit: wait if we're calling too fast
    pub async fn rate_limit(&self) {
        let mut last = self.last_request_at.write().await;
        let elapsed = last.elapsed().as_millis() as u64;
        if elapsed < self.min_request_interval_ms {
            let wait = self.min_request_interval_ms - elapsed;
            tokio::time::sleep(Duration::from_millis(wait)).await;
        }
        *last = Instant::now();
    }
}

/// Central RPC gateway with metrics tracking
/// 
/// Uses multiple RPC providers with round-robin retry on failure:
/// - First attempt always uses provider[0] (Flux) for most calls
/// - Transaction fetching uses round-robin across ALL providers for max throughput
/// - On failure, rotates to next provider (round-robin)
/// - 10 total attempts with 500ms delay between retries
pub struct AppRpc {
    /// RPC providers in priority order: [Flux, Helius, Triton, ...]
    providers: Vec<RpcProvider>,
    
    /// Atomic counter for round-robin provider selection (used for transaction fetching)
    round_robin_counter: AtomicUsize,
    
    // Metrics tracking
    clickhouse: Option<Arc<ClickHouseClient>>,
    program_name: String,
}

impl AppRpc {
    /// Create a new AppRpc instance with Flux as primary and Helius/Triton as backups.
    /// 
    /// # Arguments
    /// * `helius_rpc_url` - The Helius RPC URL (with or without https:// prefix)
    /// * `flux_rpc_url` - The Flux RPC URL (with or without https:// prefix)
    /// * `triton_rpc_url` - Optional Triton RPC URL (with or without https:// prefix)
    /// * `clickhouse` - Optional ClickHouse client for metrics logging
    pub fn new(
        helius_rpc_url: String,
        flux_rpc_url: String,
        triton_rpc_url: Option<String>,
        clickhouse: Option<Arc<ClickHouseClient>>,
    ) -> Self {
        // Create providers in priority order: Flux first, then Helius, then Triton
        let flux_provider = RpcProvider::new("flux", &flux_rpc_url, FLUX_MIN_INTERVAL_MS);
        let helius_provider = RpcProvider::new("helius", &helius_rpc_url, HELIUS_MIN_INTERVAL_MS);
        
        let mut providers = vec![flux_provider, helius_provider];
        
        // Add Triton if URL is provided
        if let Some(triton_url) = triton_rpc_url {
            let triton_provider = RpcProvider::new("triton", &triton_url, TRITON_MIN_INTERVAL_MS);
            tracing::info!(
                "AppRpc initialized with {} providers: Flux ({}ms), Helius ({}ms), Triton ({}ms)",
                3,
                FLUX_MIN_INTERVAL_MS,
                HELIUS_MIN_INTERVAL_MS,
                TRITON_MIN_INTERVAL_MS
            );
            providers.push(triton_provider);
        } else {
            tracing::info!(
                "AppRpc initialized with {} providers: Flux ({}ms), Helius ({}ms)",
                2,
                FLUX_MIN_INTERVAL_MS,
                HELIUS_MIN_INTERVAL_MS
            );
        }
        
        Self {
            providers,
            round_robin_counter: AtomicUsize::new(0),
            clickhouse,
            program_name: "ore-stats".to_string(),
        }
    }
    
    /// Get the primary provider (Flux)
    fn primary_provider(&self) -> &RpcProvider {
        &self.providers[0]
    }
    
    /// Get provider by index (wraps around)
    fn provider(&self, index: usize) -> &RpcProvider {
        &self.providers[index % self.providers.len()]
    }
    
    /// Get the next provider in round-robin order (for load distribution)
    /// Returns the provider index and a reference to the provider
    fn next_round_robin_provider(&self) -> (usize, &RpcProvider) {
        let index = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
        let provider_index = index % self.providers.len();
        (provider_index, &self.providers[provider_index])
    }
    
    /// Log successful RPC call to ClickHouse
    async fn log_success(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32, result_count: u32, response_size: u32) {
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
    
    /// Log error RPC call to ClickHouse
    async fn log_error(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32, error: &str) {
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
    
    /// Log not found RPC call to ClickHouse
    async fn log_not_found(&self, provider_name: &str, api_key_id: &str, ctx: &RpcContext, duration_ms: u32) {
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
    
    // ========== ORE Account Fetching (with retry) ==========
    
    /// Get the Board account (with retry across providers)
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_board(&self) -> Result<Board> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let address = board_pda().0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "board".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_account_with_config(&address, config.clone()).await {
                Ok(response) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    if let Some(account) = response.value {
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, response.response_size as u32).await;
                        let board = Board::try_from_bytes(&account.data)?;
                        return Ok(*board);
                    } else {
                        last_error = "Account not found".to_string();
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    }
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_board: {}", MAX_RETRIES, last_error))
    }
    
    /// Get a Round account by ID (with retry across providers)
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_round(&self, round_id: u64) -> Result<Round> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let address = round_pda(round_id).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "round".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_account_with_config(&address, config.clone()).await {
                Ok(response) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    if let Some(account) = response.value {
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, response.response_size as u32).await;
                        let round = Round::try_from_bytes(&account.data)?;
                        return Ok(*round);
                    } else {
                        last_error = "Account not found".to_string();
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    }
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_round: {}", MAX_RETRIES, last_error))
    }
    
    /// Get the Treasury account (with retry across providers)
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_treasury(&self) -> Result<Treasury> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "treasury".to_string(),
            target_address: TREASURY_ADDRESS.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_account_with_config(&TREASURY_ADDRESS, config.clone()).await {
                Ok(response) => {
        let duration_ms = start.elapsed().as_millis() as u32;
                    if let Some(account) = response.value {
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, response.response_size as u32).await;
                        let treasury = Treasury::try_from_bytes(&account.data)?;
                        return Ok(*treasury);
                    } else {
                        last_error = "Account not found".to_string();
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    }
                }
                Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_treasury: {}", MAX_RETRIES, last_error))
    }
    
    /// Get the ORE Mint supply (with retry across providers)
    /// Returns the total supply of ORE tokens in atomic units (11 decimals)
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_mint_supply(&self) -> Result<u64> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "mint".to_string(),
            target_address: MINT_ADDRESS.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
        
            match provider.client.get_account_with_config(&MINT_ADDRESS, config.clone()).await {
                Ok(response) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    if let Some(account) = response.value {
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, response.response_size as u32).await;
                        // SPL Token Mint account layout:
                        // - 36..44: supply (8 bytes, little-endian u64)
                        if account.data.len() < 44 {
                            return Err(anyhow::anyhow!("Mint account data too short: {} bytes", account.data.len()));
                        }
                        let supply = u64::from_le_bytes(account.data[36..44].try_into()?);
                        return Ok(supply);
                    } else {
                        last_error = "Mint account not found".to_string();
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    }
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_mint_supply: {}", MAX_RETRIES, last_error))
    }
    
    /// Get a Miner account by authority (with retry across providers)
    /// Returns None if miner account doesn't exist
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_miner(&self, authority: &Pubkey) -> Result<Option<Miner>> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let address = miner_pda(*authority).0;
        let ctx = RpcContext {
            method: "getAccountInfo".to_string(),
            target_type: "miner".to_string(),
            target_address: authority.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        // Use manual retry loop for special "not found" handling
        let provider = self.primary_provider();
        provider.rate_limit().await;
        let start = Instant::now();
        
        let result = provider.client.get_account_with_config(&address, config).await;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
            Ok(response) => {
                if let Some(account) = response.value {
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, response.response_size as u32).await;
                    let miner = Miner::try_from_bytes(&account.data)?;
                Ok(Some(*miner))
                } else {
                    // Account not found - return None
                    self.log_not_found(&provider.name, &provider.api_key_id, &ctx, duration_ms).await;
                    Ok(None)
                }
            }
            Err(e) => {
                // Account not found is not an error for optional miner
                if e.to_string().contains("AccountNotFound") {
                    self.log_not_found(&provider.name, &provider.api_key_id, &ctx, duration_ms).await;
                    Ok(None)
                } else {
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &e.to_string()).await;
                    Err(e.into())
                }
            }
        }
    }
    
    // ========== Other RPC calls (with retry) ==========
        
    /// Get SOL balance for an account (with retry across providers)
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let ctx = RpcContext {
            method: "getBalance".to_string(),
            target_type: "balance".to_string(),
            target_address: pubkey.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_balance(pubkey).await {
            Ok(balance) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, 8).await;
                    return Ok(balance);
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_balance: {}", MAX_RETRIES, last_error))
    }
    
    /// Get current slot (with retry across providers)
    pub async fn get_slot(&self) -> Result<u64> {
        let ctx = RpcContext {
            method: "getSlot".to_string(),
            target_type: "slot".to_string(),
            target_address: String::new(),
            is_batch: false,
            batch_size: 1,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_slot().await {
            Ok(slot) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, 8).await;
                    return Ok(slot);
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_slot: {}", MAX_RETRIES, last_error))
    }
    
    /// Get multiple accounts at once (with retry across providers - GMA)
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Vec<u8>>>> {
        use solana_client::rpc_config::RpcAccountInfoConfig;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let ctx = RpcContext {
            method: "getMultipleAccounts".to_string(),
            target_type: "batch".to_string(),
            target_address: String::new(),
            is_batch: true,
            batch_size: pubkeys.len() as u16,
        };
        
        let config = RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64Zstd),
            data_slice: None,
            commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
            min_context_slot: None,
        };
        
        let mut last_error = String::new();
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            match provider.client.get_multiple_accounts_with_config(pubkeys, config.clone()).await {
                Ok(response) => {
                    let accounts = response.value;
                    let duration_ms = start.elapsed().as_millis() as u32;
                    let found_count = accounts.iter().filter(|a| a.is_some()).count() as u32;
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, found_count, response.response_size as u32).await;
                    return Ok(accounts.into_iter().map(|a| a.map(|acc| acc.data)).collect());
            }
            Err(e) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        Err(anyhow::anyhow!("All {} attempts failed for get_multiple_accounts: {}", MAX_RETRIES, last_error))
    }
    
    /// Get signature statuses for transaction confirmations (with retry across providers)
    /// Returns Vec<Option<SignatureStatus>> where:
    /// - None = not found yet
    /// - Some(status) = found with confirmation details
    pub async fn get_signature_statuses(&self, signatures: &[String]) -> Result<Vec<Option<SignatureStatus>>> {
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
        
        // Use direct JSON-RPC call
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [
                signatures,
                { "searchTransactionHistory": false }
            ]
        });
        
        let mut last_error = String::new();
        let http_client = reqwest::Client::new();
        
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            let response = http_client
                .post(&provider.url)
            .json(&body)
            .send()
            .await;
        
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match response {
            Ok(res) => {
                    let json: serde_json::Value = match res.json().await {
                        Ok(j) => j,
                        Err(e) => {
                            last_error = format!("Failed to parse response: {}", e);
                            self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                            if attempt < MAX_RETRIES - 1 {
                                tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                            }
                            continue;
                        }
                    };
                
                if let Some(error) = json.get("error") {
                        last_error = error.to_string();
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                        if attempt < MAX_RETRIES - 1 {
                            tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                        }
                        continue;
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
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, confirmed_count, 0).await;
                    return Ok(statuses);
            }
            Err(e) => {
                    last_error = e.to_string();
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} RPC attempts failed for getSignatureStatuses: {}", MAX_RETRIES, last_error))
    }
    
    /// Get all ORE Miner accounts using standard getProgramAccounts RPC (with retry)
    /// This is the source of truth for miner data - more reliable than v2 endpoint
    /// Returns a HashMap keyed by authority pubkey string
    /// If treasury is provided, applies refined_ore calculation immediately
    pub async fn get_all_miners_gpa(&self, treasury: Option<&Treasury>) -> Result<std::collections::HashMap<String, Miner>> {
        use solana_client::rpc_config::{RpcProgramAccountsConfig, RpcAccountInfoConfig};
        use solana_client::rpc_filter::RpcFilterType;
        use solana_account_decoder_client_types::UiAccountEncoding;
        
        let ctx = RpcContext {
            method: "getProgramAccounts".to_string(),
            target_type: "miner".to_string(),
            target_address: evore::ore_api::PROGRAM_ID.to_string(),
            is_batch: true,
            batch_size: 0, // Unknown until we get results
        };
        
        // Filter for Miner accounts by size (size_of::<Miner>() + 8 for discriminator)
        let miner_size = std::mem::size_of::<Miner>() as u64 + 8;
        
        // Use Base64Zstd for compression - reduces bandwidth significantly
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(miner_size)]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                data_slice: None,
                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };
        
        let mut last_error = String::new();
        
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            let result = provider.client
                .get_program_accounts_with_config(&evore::ore_api::PROGRAM_ID, config.clone())
            .await;
        
        let duration_ms = start.elapsed().as_millis() as u32;
        
        match result {
                Ok((accounts, response_size)) => {
                let mut miners = std::collections::HashMap::new();
                
                    for (_pubkey, account) in &accounts {
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
                    
                    // Validate: must have at least 10,000 miners to be a successful snapshot
                    if miners.len() < 10_000 {
                        last_error = format!(
                            "Insufficient miners: got {} but need at least 10,000",
                            miners.len()
                        );
                        tracing::warn!(
                            "GPA miners snapshot ({}) failed validation: {} (attempt {}/{})",
                            provider.name, last_error, attempt + 1, MAX_RETRIES
                        );
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                        if attempt < MAX_RETRIES - 1 {
                            tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                        }
                        continue;
                }
                
                tracing::info!(
                        "GPA miners snapshot ({}): {} accounts fetched, {} miners parsed in {}ms",
                        provider.name, accounts.len(), miners.len(), duration_ms
                );
                
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, miners.len() as u32, response_size as u32).await;
                    return Ok(miners);
            }
            Err(e) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        "GPA miners snapshot ({}) failed (attempt {}/{}): {}",
                        provider.name, attempt + 1, MAX_RETRIES, last_error
                    );
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} GPA attempts failed: {}", MAX_RETRIES, last_error))
    }
    
    /// Get all EVORE Manager accounts via GPA (with retry)
    /// Returns Vec of (address, account_data) tuples
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_evore_managers_gpa(&self) -> Result<Vec<(Pubkey, Vec<u8>)>> {
        use solana_client::rpc_config::{RpcProgramAccountsConfig, RpcAccountInfoConfig};
        use solana_client::rpc_filter::RpcFilterType;
        use solana_account_decoder_client_types::UiAccountEncoding;
        use crate::evore_cache::MANAGER_SIZE;
        
        let ctx = RpcContext {
            method: "getProgramAccounts".to_string(),
            target_type: "evore_manager".to_string(),
            target_address: evore::ID.to_string(),
            is_batch: true,
            batch_size: 0,
        };
        
        // Use Base64Zstd for compression - reduces bandwidth significantly
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(MANAGER_SIZE as u64)]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                data_slice: None,
                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };
        
        let mut last_error = String::new();
        
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            let result = provider.client
                .get_program_accounts_with_config(&evore::ID, config.clone())
                .await;
            
            let duration_ms = start.elapsed().as_millis() as u32;
            
            match result {
                Ok((accounts, response_size)) => {
                    let result: Vec<(Pubkey, Vec<u8>)> = accounts
                        .into_iter()
                        .map(|(pk, acc)| (pk, acc.data))
                        .collect();
                    
                    tracing::info!(
                        "GPA EVORE managers ({}): {} accounts in {}ms",
                        provider.name, result.len(), duration_ms
                    );
                    
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, result.len() as u32, response_size as u32).await;
                    return Ok(result);
                }
                Err(e) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        "GPA EVORE managers ({}) failed (attempt {}/{}): {}",
                        provider.name, attempt + 1, MAX_RETRIES, last_error
                    );
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} EVORE managers GPA attempts failed: {}", MAX_RETRIES, last_error))
    }
    
    /// Get all EVORE Deployer accounts via GPA (with retry)
    /// Returns Vec of (address, account_data) tuples
    /// Uses Base64Zstd encoding for bandwidth efficiency
    pub async fn get_evore_deployers_gpa(&self) -> Result<Vec<(Pubkey, Vec<u8>)>> {
        use solana_client::rpc_config::{RpcProgramAccountsConfig, RpcAccountInfoConfig};
        use solana_client::rpc_filter::RpcFilterType;
        use solana_account_decoder_client_types::UiAccountEncoding;
        use crate::evore_cache::DEPLOYER_SIZE;
        
        let ctx = RpcContext {
            method: "getProgramAccounts".to_string(),
            target_type: "evore_deployer".to_string(),
            target_address: evore::ID.to_string(),
            is_batch: true,
            batch_size: 0,
        };
        
        // Use Base64Zstd for compression - reduces bandwidth significantly
        let config = RpcProgramAccountsConfig {
            filters: Some(vec![RpcFilterType::DataSize(DEPLOYER_SIZE as u64)]),
            account_config: RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64Zstd),
                data_slice: None,
                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None,
        };
        
        let mut last_error = String::new();
        
        for attempt in 0..MAX_RETRIES {
            let provider = self.provider(attempt);
            provider.rate_limit().await;
            let start = Instant::now();
            
            let result = provider.client
                .get_program_accounts_with_config(&evore::ID, config.clone())
                .await;
            
            let duration_ms = start.elapsed().as_millis() as u32;
            
            match result {
                Ok((accounts, response_size)) => {
                    let result: Vec<(Pubkey, Vec<u8>)> = accounts
                        .into_iter()
                        .map(|(pk, acc)| (pk, acc.data))
                        .collect();
                    
                    tracing::info!(
                        "GPA EVORE deployers ({}): {} accounts in {}ms",
                        provider.name, result.len(), duration_ms
                    );
                    
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, result.len() as u32, response_size as u32).await;
                    return Ok(result);
                }
                Err(e) => {
                    last_error = e.to_string();
                    tracing::warn!(
                        "GPA EVORE deployers ({}) failed (attempt {}/{}): {}",
                        provider.name, attempt + 1, MAX_RETRIES, last_error
                    );
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} EVORE deployers GPA attempts failed: {}", MAX_RETRIES, last_error))
    }
    
    // ========== Transaction Fetching ==========
    
    /// Get signatures for an address with pagination
    /// Returns up to 1000 signatures per call
    /// Uses Confirmed commitment
    pub async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        before: Option<&str>,
        _until: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature>> {
        use solana_sdk::signature::Signature;
        
        let ctx = RpcContext {
            method: "getSignaturesForAddress".to_string(),
            target_type: "signatures".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let before_sig = before.and_then(|s| s.parse::<Signature>().ok());
        
        let mut last_error = String::new();
        
        for attempt in 0..MAX_RETRIES {
            let provider_idx = attempt % self.providers.len();
            let provider = &self.providers[provider_idx];
            
            provider.rate_limit().await;
            let start = Instant::now();
            
            // Use the simpler method that accepts (address, before, limit, commitment)
            // Note: until is not used as most RPCs don't support it consistently
            let result = match (before_sig, limit) {
                (Some(before), Some(limit)) => {
                    provider.client.get_signatures_for_address_with_config(
                        address,
                        solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                            before: Some(before),
                            until: None,
                            limit: Some(limit),
                            commitment: Some(CommitmentConfig {
                                commitment: CommitmentLevel::Confirmed,
                            }),
                        },
                    ).await
                }
                (None, Some(limit)) => {
                    provider.client.get_signatures_for_address_with_config(
                        address,
                        solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                            before: None,
                            until: None,
                            limit: Some(limit),
                            commitment: Some(CommitmentConfig {
                                commitment: CommitmentLevel::Confirmed,
                            }),
                        },
                    ).await
                }
                _ => {
                    provider.client.get_signatures_for_address(address).await
                }
            };
            
            match result {
                Ok((sigs, response_size)) => {
                    let duration_ms = start.elapsed().as_millis() as u32;
                    self.log_success(
                        &provider.name, 
                        &provider.api_key_id, 
                        &ctx, 
                        duration_ms,
                        sigs.len() as u32, 
                        response_size as u32
                    ).await;
                    return Ok(sigs);
                }
                Err(e) => {
                    last_error = e.to_string();
                    let duration_ms = start.elapsed().as_millis() as u32;
                    tracing::warn!(
                        "getSignaturesForAddress ({}) failed (attempt {}/{}): {}",
                        provider.name, attempt + 1, MAX_RETRIES, last_error
                    );
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} getSignaturesForAddress attempts failed: {}", MAX_RETRIES, last_error))
    }
    
    /// Get all signatures for an address (handles pagination automatically)
    /// Fetches up to 1000 signatures per call, continues until no more pages
    pub async fn get_all_signatures_for_address(
        &self,
        address: &Pubkey,
    ) -> Result<Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature>> {
        let mut all_sigs = Vec::new();
        let mut before: Option<String> = None;
        
        loop {
            let sigs = self.get_signatures_for_address(
                address,
                before.as_deref(),
                None,
                Some(1000),
            ).await?;
            
            let count = sigs.len();
            if let Some(last) = sigs.last() {
                before = Some(last.signature.clone());
            }
            all_sigs.extend(sigs);
            
            if count < 1000 {
                break; // No more pages
            }
        }
        
        Ok(all_sigs)
    }
    
    /// Get all signatures for an address, retrying with different providers if 0 are found.
    /// This is for backfill operations where 0 signatures indicates an RPC issue, not missing data.
    /// Tries each provider in round-robin order until signatures are found or all fail.
    pub async fn get_all_signatures_for_address_with_retry(
        &self,
        address: &Pubkey,
    ) -> Result<Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature>> {
        let ctx = RpcContext {
            method: "getSignaturesForAddress".to_string(),
            target_type: "signatures_backfill".to_string(),
            target_address: address.to_string(),
            is_batch: false,
            batch_size: 0,
        };
        
        // Try each provider
        for attempt in 0..self.providers.len() {
            let provider_idx = attempt % self.providers.len();
            let provider = &self.providers[provider_idx];
            
            tracing::debug!("Trying {} for signatures of {}", provider.name, address);
            
            // Fetch signatures using this provider directly
            let mut all_sigs = Vec::new();
            let mut before: Option<String> = None;
            let mut fetch_failed = false;
            
            loop {
                provider.rate_limit().await;
                let start = Instant::now();
                
                let before_sig = if let Some(ref sig_str) = before {
                    sig_str.parse::<solana_sdk::signature::Signature>().ok()
                } else {
                    None
                };
                
                let result = match before_sig {
                    Some(before) => {
                        provider.client.get_signatures_for_address_with_config(
                            address,
                            solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                                before: Some(before),
                                until: None,
                                limit: Some(1000),
                                commitment: Some(solana_sdk::commitment_config::CommitmentConfig {
                                    commitment: solana_sdk::commitment_config::CommitmentLevel::Confirmed,
                                }),
                            },
                        ).await
                    }
                    None => {
                        provider.client.get_signatures_for_address_with_config(
                            address,
                            solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config {
                                before: None,
                                until: None,
                                limit: Some(1000),
                                commitment: Some(solana_sdk::commitment_config::CommitmentConfig {
                                    commitment: solana_sdk::commitment_config::CommitmentLevel::Confirmed,
                                }),
                            },
                        ).await
                    }
                };
                
                match result {
                    Ok((sigs, _response_size)) => {
                        let count = sigs.len();
                        if let Some(last) = sigs.last() {
                            before = Some(last.signature.clone());
                        }
                        all_sigs.extend(sigs);
                        
                        if count < 1000 {
                            break; // No more pages
                        }
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis() as u32;
                        tracing::warn!(
                            "getSignaturesForAddress ({}) failed: {}",
                            provider.name, e
                        );
                        self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &e.to_string()).await;
                        fetch_failed = true;
                        break;
                    }
                }
            }
            
            if fetch_failed {
                continue; // Try next provider
            }
            
            // If we got signatures, return them
            if !all_sigs.is_empty() {
                tracing::debug!("Got {} signatures from {}", all_sigs.len(), provider.name);
                return Ok(all_sigs);
            }
            
            // 0 signatures - try next provider
            tracing::warn!(
                "Got 0 signatures from {} for {}, trying next provider",
                provider.name, address
            );
        }
        
        // All providers returned 0 or failed
        Err(anyhow::anyhow!(
            "All {} providers returned 0 signatures for {}",
            self.providers.len(), address
        ))
    }
    
    /// Get a full transaction by signature
    /// Uses Confirmed commitment
    /// Returns the raw JSON transaction
    /// 
    /// Uses round-robin provider selection for the first attempt to distribute
    /// load across all providers. Retries continue round-robin from there.
    pub async fn get_transaction(&self, signature: &str) -> Result<Option<TransactionResult>> {
        use solana_sdk::signature::Signature;
        
        let sig = signature.parse::<Signature>()
            .map_err(|e| anyhow::anyhow!("Invalid signature: {}", e))?;
        
        let ctx = RpcContext {
            method: "getTransaction".to_string(),
            target_type: "transaction".to_string(),
            target_address: signature.to_string(),
            is_batch: false,
            batch_size: 1,
        };
        
        let mut last_error = String::new();
        
        // Get starting provider via round-robin (distributes load across all providers)
        let (start_idx, _) = self.next_round_robin_provider();
        
        for attempt in 0..MAX_RETRIES {
            let provider_idx = (start_idx + attempt) % self.providers.len();
            let provider = &self.providers[provider_idx];
            
            provider.rate_limit().await;
            let start = Instant::now();
            
            // Use send to call getTransaction RPC directly to avoid crate version conflicts
            use solana_client::rpc_request::RpcRequest;
            use serde_json::json;
            
            let params = json!([
                sig.to_string(),
                {
                    "encoding": "json",
                    "commitment": "confirmed",
                    "maxSupportedTransactionVersion": 0
                }
            ]);
            
            let result: Result<serde_json::Value, _> = provider.client
                .send(RpcRequest::GetTransaction, params)
                .await;
            
            match result {
                Ok(tx_json) => {
                    // Check if transaction was found (null response means not found)
                    if tx_json.is_null() {
                        let duration_ms = start.elapsed().as_millis() as u32;
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 0, 0).await;
                        return Ok(None);
                    }
                    
                    let duration_ms = start.elapsed().as_millis() as u32;
                    
                    // Extract slot and block_time from JSON
                    let slot = tx_json.get("slot").and_then(|s| s.as_u64()).unwrap_or(0);
                    let block_time = tx_json.get("blockTime").and_then(|t| t.as_i64());
                    
                    // Serialize to JSON string for storage
                    let raw_json = serde_json::to_string(&tx_json)
                        .map_err(|e| anyhow::anyhow!("Failed to serialize transaction: {}", e))?;
                    
                    self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 1, raw_json.len() as u32).await;
                    
                    return Ok(Some(TransactionResult {
                        signature: signature.to_string(),
                        slot,
                        block_time,
                        raw_json,
                    }));
                }
                Err(e) => {
                    last_error = e.to_string();
                    let duration_ms = start.elapsed().as_millis() as u32;
                    
                    // If transaction not found, that's not an error for retrying
                    if last_error.contains("not found") || last_error.contains("Transaction version") {
                        self.log_success(&provider.name, &provider.api_key_id, &ctx, duration_ms, 0, 0).await;
                        return Ok(None);
                    }
                    
                    tracing::warn!(
                        "getTransaction ({}) failed (attempt {}/{}): {}",
                        provider.name, attempt + 1, MAX_RETRIES, last_error
                    );
                    self.log_error(&provider.name, &provider.api_key_id, &ctx, duration_ms, &last_error).await;
                    if attempt < MAX_RETRIES - 1 {
                        tokio::time::sleep(Duration::from_millis(RETRY_DELAY_MS)).await;
                    }
                }
            }
        }
        
        Err(anyhow::anyhow!("All {} getTransaction attempts failed: {}", MAX_RETRIES, last_error))
    }
}

/// Result from get_transaction containing the raw JSON and metadata
#[derive(Debug, Clone)]
pub struct TransactionResult {
    pub signature: String,
    pub slot: u64,
    pub block_time: Option<i64>,
    pub raw_json: String,
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


