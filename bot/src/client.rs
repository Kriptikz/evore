//! EvoreClient - Unified RPC client with RPS tracking
//!
//! All RPC calls should go through this client to ensure proper tracking.
//! The RpsTracker can be shared with other components for monitoring.

use solana_client::{
    rpc_client::RpcClient,
    rpc_config::RpcSendTransactionConfig,
};
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    hash::Hash,
    pubkey::Pubkey,
    signature::Signature,
    transaction::{Transaction, TransactionError},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use evore::ore_api::{board_pda, miner_pda, round_pda, Board, Miner, Round, Treasury, TREASURY_ADDRESS};
use evore::state::{managed_miner_auth_pda, Manager};
use steel::AccountDeserialize;

/// Transaction status result
#[derive(Debug, Clone)]
pub struct TxStatusResult {
    pub err: Option<TransactionError>,
    pub slot: u64,
}

/// RPS (requests per second) tracker
/// Uses a 10-second sliding window for accurate averaging
#[derive(Debug)]
pub struct RpsTracker {
    /// Total requests made
    total_requests: AtomicU64,
    /// Requests in the current 10-second window
    requests_in_window: AtomicU64,
    /// Last calculated RPS (requests per second averaged over 10s)
    last_rps: AtomicU64,
    /// Start time for the current 10-second window
    window_start: std::sync::RwLock<Instant>,
}

/// Window duration for RPS calculation (10 seconds)
const RPS_WINDOW_SECS: u64 = 10;

impl RpsTracker {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            requests_in_window: AtomicU64::new(0),
            last_rps: AtomicU64::new(0),
            window_start: std::sync::RwLock::new(Instant::now()),
        }
    }
    
    /// Record a request
    pub fn record_request(&self) {
        self.total_requests.fetch_add(1, Ordering::Relaxed);
        
        // Check if we need to roll over to a new window
        let now = Instant::now();
        let window_start = *self.window_start.read().unwrap();
        let elapsed_secs = now.duration_since(window_start).as_secs();
        
        if elapsed_secs >= RPS_WINDOW_SECS {
            // Calculate RPS from the completed window
            let count = self.requests_in_window.swap(1, Ordering::Relaxed);
            // Average over the window period
            let rps = count / RPS_WINDOW_SECS;
            self.last_rps.store(rps, Ordering::Relaxed);
            
            // Update window start
            if let Ok(mut ws) = self.window_start.write() {
                *ws = now;
            }
        } else {
            self.requests_in_window.fetch_add(1, Ordering::Relaxed);
        }
    }
    
    /// Get current RPS (averaged over 10-second window)
    pub fn get_rps(&self) -> u32 {
        let now = Instant::now();
        let window_start = *self.window_start.read().unwrap();
        let elapsed_secs = now.duration_since(window_start).as_secs();
        
        if elapsed_secs >= RPS_WINDOW_SECS {
            // Window expired, calculate from current count
            let count = self.requests_in_window.load(Ordering::Relaxed);
            (count / RPS_WINDOW_SECS) as u32
        } else if elapsed_secs > 0 {
            // Mid-window: calculate current rate
            let count = self.requests_in_window.load(Ordering::Relaxed);
            (count / elapsed_secs) as u32
        } else {
            // Very start of window, return last known RPS
            self.last_rps.load(Ordering::Relaxed) as u32
        }
    }
    
    /// Get total requests
    pub fn get_total(&self) -> u64 {
        self.total_requests.load(Ordering::Relaxed)
    }
}

impl Default for RpsTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Unified RPC client with request tracking
/// 
/// All RPC calls go through this client to ensure proper RPS tracking.
/// The `rpc` field is private - use the provided methods instead.
pub struct EvoreClient {
    rpc: RpcClient,
    pub rps_tracker: Arc<RpsTracker>,
}

impl EvoreClient {
    pub fn new(rpc_url: &str) -> Self {
        let rpc = RpcClient::new_with_timeout_and_commitment(
            rpc_url.to_string(),
            Duration::from_secs(30),
            CommitmentConfig::confirmed(),
        );
        Self { 
            rpc,
            rps_tracker: Arc::new(RpsTracker::new()),
        }
    }
    
    /// Create with a shared RPS tracker (for sharing across multiple clients)
    pub fn new_with_tracker(rpc_url: &str, rps_tracker: Arc<RpsTracker>) -> Self {
        let rpc = RpcClient::new_with_timeout_and_commitment(
            rpc_url.to_string(),
            Duration::from_secs(30),
            CommitmentConfig::confirmed(),
        );
        Self { rpc, rps_tracker }
    }
    
    /// Create with processed commitment (for blockhash fetching)
    pub fn new_processed(rpc_url: &str, rps_tracker: Arc<RpsTracker>) -> Self {
        let rpc = RpcClient::new_with_timeout_and_commitment(
            rpc_url.to_string(),
            Duration::from_secs(30),
            CommitmentConfig::processed(),
        );
        Self { rpc, rps_tracker }
    }
    
    /// Get the RPS tracker for monitoring
    pub fn get_rps_tracker(&self) -> Arc<RpsTracker> {
        Arc::clone(&self.rps_tracker)
    }

    // =========================================================================
    // Core RPC Methods (all track RPS)
    // =========================================================================

    /// Get current slot
    pub fn get_slot(&self) -> Result<u64, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.get_slot()?)
    }
    
    /// Get latest blockhash
    pub fn get_latest_blockhash(&self) -> Result<Hash, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.get_latest_blockhash()?)
    }
    
    /// Get account balance in lamports
    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.get_balance(pubkey)?)
    }
    
    /// Get raw account data
    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Account, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.get_account(pubkey)?)
    }
    
    /// Get raw account data, returns None if not found
    pub fn get_account_optional(&self, pubkey: &Pubkey) -> Result<Option<Account>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        match self.rpc.get_account(pubkey) {
            Ok(account) => Ok(Some(account)),
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("AccountNotFound") || err_str.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get multiple accounts at once (efficient batch call)
    pub fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.get_multiple_accounts(pubkeys)?)
    }

    /// Send transaction without waiting for confirmation
    /// Skips preflight and sets 0 retries - we handle retries manually
    pub fn send_transaction_no_wait(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let config = RpcSendTransactionConfig {
            skip_preflight: true,
            max_retries: Some(0),
            ..Default::default()
        };
        let signature = self.rpc.send_transaction_with_config(transaction, config)?;
        Ok(signature)
    }
    
    /// Send transaction and wait for confirmation
    pub fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        Ok(self.rpc.send_and_confirm_transaction(transaction)?)
    }

    /// Simple confirmation check (returns bool)
    pub fn confirm_transaction(&self, signature: &Signature) -> Result<bool, Box<dyn std::error::Error>> {
        match self.get_transaction_status(signature)? {
            Some(status) => Ok(status.err.is_none()),
            None => Ok(false),
        }
    }
    
    /// Get transaction status - returns confirmation and any error
    /// Returns Ok(Some(status)) if tx found, Ok(None) if not found/expired
    pub fn get_transaction_status(&self, signature: &Signature) -> Result<Option<TxStatusResult>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let statuses = self.rpc.get_signature_statuses(&[*signature])?;
        
        match statuses.value.first() {
            Some(Some(status)) => {
                Ok(Some(TxStatusResult {
                    err: status.err.clone(),
                    slot: status.slot,
                }))
            }
            _ => Ok(None),
        }
    }
    
    /// Batch get signature statuses (more efficient for multiple txs)
    pub fn get_signature_statuses_batch(&self, signatures: &[Signature]) -> Result<Vec<Option<TxStatusResult>>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let response = self.rpc.get_signature_statuses(signatures)?;
        
        Ok(response.value.into_iter().map(|opt| {
            opt.map(|status| TxStatusResult {
                err: status.err,
                slot: status.slot,
            })
        }).collect())
    }

    // =========================================================================
    // Evore-specific Methods
    // =========================================================================

    /// Get board state (contains current round_id and end_slot)
    pub fn get_board(&self) -> Result<Board, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let board_address = board_pda().0;
        let account = self.rpc.get_account(&board_address)?;
        
        // try_from_bytes handles discriminator
        let board = Board::try_from_bytes(&account.data)?;
        Ok(*board)
    }

    /// Get round state
    pub fn get_round(&self, round_id: u64) -> Result<Round, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let round_address = round_pda(round_id).0;
        let account = self.rpc.get_account(&round_address)?;
        
        // try_from_bytes handles discriminator
        let round = Round::try_from_bytes(&account.data)?;
        Ok(*round)
    }
    
    /// Get Treasury account
    pub fn get_treasury(&self) -> Result<Treasury, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let account = self.rpc.get_account(&TREASURY_ADDRESS)?;
        let treasury = Treasury::try_from_bytes(&account.data)?;
        Ok(*treasury)
    }

    /// Get managed miner auth PDA address for a manager and auth_id
    pub fn get_managed_miner_auth_address(manager: &Pubkey, auth_id: u64) -> (Pubkey, u8) {
        managed_miner_auth_pda(*manager, auth_id)
    }
    
    /// Get Manager account data (returns None if account doesn't exist)
    pub fn get_manager(&self, manager_address: &Pubkey) -> Result<Option<Manager>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        match self.rpc.get_account(manager_address) {
            Ok(account) => {
                let manager = Manager::try_from_bytes(&account.data)?;
                Ok(Some(*manager))
            }
            Err(e) => {
                // Check if it's an "account not found" error
                let err_str = e.to_string();
                if err_str.contains("AccountNotFound") || err_str.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get ORE Miner account for an authority (returns None if doesn't exist)
    pub fn get_miner(&self, authority: &Pubkey) -> Result<Option<Miner>, Box<dyn std::error::Error>> {
        self.rps_tracker.record_request();
        let (miner_address, _) = miner_pda(*authority);
        match self.rpc.get_account(&miner_address) {
            Ok(account) => {
                let miner = Miner::try_from_bytes(&account.data)?;
                Ok(Some(*miner))
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("AccountNotFound") || err_str.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get multiple Miner accounts at once (for miner tracker)
    pub fn get_miners(&self, authorities: &[Pubkey]) -> Result<Vec<Option<Miner>>, Box<dyn std::error::Error>> {
        let miner_addresses: Vec<Pubkey> = authorities.iter()
            .map(|auth| miner_pda(*auth).0)
            .collect();
        
        self.rps_tracker.record_request();
        let accounts = self.rpc.get_multiple_accounts(&miner_addresses)?;
        
        Ok(accounts.into_iter().map(|opt| {
            opt.and_then(|account| {
                Miner::try_from_bytes(&account.data).ok().copied()
            })
        }).collect())
    }
}

/// Display helper for managed miner auth PDA
pub fn print_managed_miner_info(manager: &Pubkey, auth_id: u64) {
    let (pda, bump) = EvoreClient::get_managed_miner_auth_address(manager, auth_id);
    println!("Manager:              {}", manager);
    println!("Auth ID:              {}", auth_id);
    println!("Managed Miner Auth:   {}", pda);
    println!("Bump:                 {}", bump);
    println!();
}
