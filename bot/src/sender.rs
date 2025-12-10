//! Fast Sender - Uses Helius and Jito endpoints for faster transaction sending
//!
//! Features:
//! - Uses both East and West region Helius endpoints for geographic distribution
//! - Jito block engine endpoints (NY and SLC) with 1 RPS rate limiting
//! - Automatic retry queue: each transaction is sent multiple times across endpoints
//! - Round-robin sending: cycles through all queued transactions
//! - Automatic Jito tip inclusion for MEV protection
//! - Periodic ping measurement for latency monitoring

use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::collections::VecDeque;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

// =============================================================================
// Jito Tip Accounts (mainnet-beta)
// =============================================================================

pub const JITO_TIP_ACCOUNTS: [&str; 10] = [
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

/// Default minimum Jito tip (0.0002 SOL = 200,000 lamports)
pub const DEFAULT_JITO_TIP: u64 = 200_000;

/// SWQOS-only minimum tip (0.000005 SOL = 5,000 lamports)
pub const SWQOS_ONLY_TIP: u64 = 5_000;

/// Helius fast sender endpoint - East region (Newark)
pub const HELIUS_EAST_ENDPOINT: &str = "http://ewr-sender.helius-rpc.com/fast";

/// Helius fast sender endpoint - West region (Salt Lake City)
pub const HELIUS_WEST_ENDPOINT: &str = "http://slc-sender.helius-rpc.com/fast";

/// Helius ping endpoint - East region
pub const HELIUS_EAST_PING: &str = "http://ewr-sender.helius-rpc.com/ping";

/// Helius ping endpoint - West region
pub const HELIUS_WEST_PING: &str = "http://slc-sender.helius-rpc.com/ping";

/// Jito block engine endpoint - East region (New York)
pub const JITO_EAST_ENDPOINT: &str = "https://ny.mainnet.block-engine.jito.wtf/api/v1/transactions";

/// Jito block engine endpoint - West region (Salt Lake City)
pub const JITO_WEST_ENDPOINT: &str = "https://slc.mainnet.block-engine.jito.wtf/api/v1/transactions";

/// Number of times to send each transaction via Helius (2 = 1x East + 1x West)
const HELIUS_SENDS_PER_TX: u8 = 2;

/// Interval between Helius sends in milliseconds (fast)
const HELIUS_SEND_INTERVAL_MS: u64 = 100;

/// Interval between Jito sends in milliseconds (1 RPS = 1000ms per endpoint)
const JITO_SEND_INTERVAL_MS: u64 = 1000;

/// Interval between ping checks in seconds
const PING_INTERVAL_SECS: u64 = 10;

// =============================================================================
// Ping Stats (Shared State)
// =============================================================================

/// Window duration for RPS calculation (10 seconds)
const RPS_WINDOW_SECS: u64 = 10;

/// Shared ping statistics and RPS for sender endpoints
/// Updated by the background tasks, read by TUI
#[derive(Debug)]
pub struct PingStats {
    /// East endpoint latency in milliseconds (0 = failed/unknown)
    pub east_latency_ms: AtomicU32,
    /// West endpoint latency in milliseconds (0 = failed/unknown)
    pub west_latency_ms: AtomicU32,
    /// East endpoint status: 0=unknown, 1=connected, 2=disconnected
    pub east_status: AtomicU32,
    /// West endpoint status: 0=unknown, 1=connected, 2=disconnected
    pub west_status: AtomicU32,
    /// Last successful ping timestamp (unix millis)
    pub last_ping: AtomicU64,
    /// Total HTTP sends made
    pub total_sends: AtomicU64,
    /// Timestamps of recent sends (for RPS calculation)
    send_timestamps: std::sync::Mutex<Vec<Instant>>,
}

impl PingStats {
    pub fn new() -> Self {
        Self {
            east_latency_ms: AtomicU32::new(0),
            west_latency_ms: AtomicU32::new(0),
            east_status: AtomicU32::new(0),
            west_status: AtomicU32::new(0),
            last_ping: AtomicU64::new(0),
            total_sends: AtomicU64::new(0),
            send_timestamps: std::sync::Mutex::new(Vec::new()),
        }
    }
    
    /// Record a send request
    pub fn record_send(&self) {
        self.total_sends.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut timestamps) = self.send_timestamps.lock() {
            timestamps.push(Instant::now());
        }
    }
    
    /// Get sender RPS (sends in last 10 seconds / 10)
    pub fn get_sender_rps(&self) -> u32 {
        if let Ok(mut timestamps) = self.send_timestamps.lock() {
            let cutoff = Instant::now() - Duration::from_secs(RPS_WINDOW_SECS);
            // Remove old timestamps
            timestamps.retain(|t| *t > cutoff);
            // RPS = count / window_size
            (timestamps.len() as u64 / RPS_WINDOW_SECS) as u32
        } else {
            0
        }
    }
    
    /// Get total sends made
    pub fn get_total_sends(&self) -> u64 {
        self.total_sends.load(Ordering::Relaxed)
    }
    
    /// Get East latency (None if failed/unknown)
    pub fn get_east_latency(&self) -> Option<u32> {
        let ms = self.east_latency_ms.load(Ordering::Relaxed);
        if ms > 0 { Some(ms) } else { None }
    }
    
    /// Get West latency (None if failed/unknown)
    pub fn get_west_latency(&self) -> Option<u32> {
        let ms = self.west_latency_ms.load(Ordering::Relaxed);
        if ms > 0 { Some(ms) } else { None }
    }
    
    /// Check if East endpoint is connected
    pub fn is_east_connected(&self) -> bool {
        self.east_status.load(Ordering::Relaxed) == 1
    }
    
    /// Check if West endpoint is connected
    pub fn is_west_connected(&self) -> bool {
        self.west_status.load(Ordering::Relaxed) == 1
    }
}

impl Default for PingStats {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Queued Transaction
// =============================================================================

/// Transaction in the send queue with send count
struct QueuedTx {
    /// Serialized transaction (base64)
    base64_tx: String,
    /// Number of times this transaction has been sent
    send_count: u8,
}

// =============================================================================
// Sender Client
// =============================================================================

/// Fast sender client using Helius and Jito endpoints with automatic retry queue
/// 
/// Helius endpoints: Each transaction sent 2 times (1x East, 1x West) every 100ms
/// Jito endpoints: Each transaction sent once to each endpoint, rate limited to 1 RPS
/// Includes ping monitoring for latency tracking.
pub struct FastSender {
    /// Channel to send transactions to the Helius background worker
    helius_queue_tx: mpsc::UnboundedSender<String>,
    /// Channel to send transactions to the Jito background worker
    jito_queue_tx: mpsc::UnboundedSender<String>,
    /// Shared ping statistics (readable by TUI)
    pub ping_stats: Arc<PingStats>,
}

impl FastSender {
    /// Create new sender with Helius and Jito endpoints
    /// Spawns background tasks for transaction sending and ping monitoring
    pub fn new() -> Self {
        let (helius_queue_tx, helius_queue_rx) = mpsc::unbounded_channel();
        let (jito_queue_tx, jito_queue_rx) = mpsc::unbounded_channel();
        let ping_stats = Arc::new(PingStats::new());
        
        // Spawn background Helius sender loop (fast, multiple sends)
        let stats_for_helius = Arc::clone(&ping_stats);
        tokio::spawn(async move {
            helius_sender_loop(helius_queue_rx, stats_for_helius).await;
        });
        
        // Spawn background Jito sender loop (rate limited, 1 RPS per endpoint)
        let stats_for_jito = Arc::clone(&ping_stats);
        tokio::spawn(async move {
            jito_sender_loop(jito_queue_rx, stats_for_jito).await;
        });
        
        // Spawn background ping monitor
        let ping_stats_clone = Arc::clone(&ping_stats);
        tokio::spawn(async move {
            ping_monitor_loop(ping_stats_clone).await;
        });
        
        Self { helius_queue_tx, jito_queue_tx, ping_stats }
    }

    /// Queue a transaction to be sent via all endpoints:
    /// - Helius: 2 times (1x East, 1x West) at 100ms interval
    /// - Jito: 1 time (alternates between NY and SLC) at 1 RPS rate limit
    /// Returns the signature immediately (extracted from signed transaction)
    pub fn send_transaction(&self, transaction: &Transaction) -> Result<Signature, SendError> {
        use base64::Engine;
        
        // Get signature from the signed transaction
        let signature = transaction.signatures.first()
            .ok_or_else(|| SendError::Serialization("Transaction has no signatures".to_string()))?;
        
        // Serialize transaction to base64
        let serialized = bincode::serialize(transaction)
            .map_err(|e| SendError::Serialization(e.to_string()))?;
        let base64_tx = base64::engine::general_purpose::STANDARD.encode(&serialized);
        
        // Queue for Helius sending (fast)
        self.helius_queue_tx.send(base64_tx.clone())
            .map_err(|e| SendError::Network(format!("Failed to queue for Helius: {}", e)))?;
        
        // Queue for Jito sending (rate limited)
        self.jito_queue_tx.send(base64_tx)
            .map_err(|e| SendError::Network(format!("Failed to queue for Jito: {}", e)))?;
        
        Ok(*signature)
    }
    
    /// Get current ping stats
    pub fn get_ping_stats(&self) -> &Arc<PingStats> {
        &self.ping_stats
    }
}

impl Default for FastSender {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Background Sender Loops
// =============================================================================

/// Helius background loop that processes the transaction queue
/// - Receives new transactions and adds them to the queue
/// - Every 100ms, sends the next transaction in the queue
/// - Each transaction is sent 2 times: even sends go East, odd sends go West
async fn helius_sender_loop(mut rx: mpsc::UnboundedReceiver<String>, stats: Arc<PingStats>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client");
    
    let client = Arc::new(client);
    let east_endpoint = Arc::new(HELIUS_EAST_ENDPOINT.to_string());
    let west_endpoint = Arc::new(HELIUS_WEST_ENDPOINT.to_string());
    
    let mut queue: VecDeque<QueuedTx> = VecDeque::new();
    let mut interval = tokio::time::interval(Duration::from_millis(HELIUS_SEND_INTERVAL_MS));
    
    loop {
        tokio::select! {
            // Receive new transactions (non-blocking check)
            result = rx.recv() => {
                match result {
                    Some(base64_tx) => {
                        queue.push_back(QueuedTx {
                            base64_tx,
                            send_count: 0,
                        });
                    }
                    None => {
                        // Channel closed, exit loop
                        break;
                    }
                }
            }
            
            // Send on interval
            _ = interval.tick() => {
                if let Some(mut queued) = queue.pop_front() {
                    // Alternate between East and West based on send_count
                    // Even (0) -> East, Odd (1) -> West
                    let endpoint = if queued.send_count % 2 == 0 {
                        Arc::clone(&east_endpoint)
                    } else {
                        Arc::clone(&west_endpoint)
                    };
                    
                    // Record the send for RPS tracking
                    stats.record_send();
                    
                    // Fire off the send (don't wait for response)
                    let client = Arc::clone(&client);
                    let base64_tx = queued.base64_tx.clone();
                    
                    tokio::spawn(async move {
                        let _ = send_helius_transaction(&client, &endpoint, &base64_tx).await;
                    });
                    
                    queued.send_count += 1;
                    
                    // Re-add to back of queue if not done
                    if queued.send_count < HELIUS_SENDS_PER_TX {
                        queue.push_back(queued);
                    }
                }
            }
        }
    }
}

/// Jito background loop that processes the transaction queue
/// - Rate limited to 1 RPS (alternates between NY and SLC endpoints)
/// - Each transaction is sent once, endpoint alternates per transaction
async fn jito_sender_loop(mut rx: mpsc::UnboundedReceiver<String>, stats: Arc<PingStats>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client for Jito");
    
    let client = Arc::new(client);
    let east_endpoint = Arc::new(JITO_EAST_ENDPOINT.to_string());
    let west_endpoint = Arc::new(JITO_WEST_ENDPOINT.to_string());
    
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut interval = tokio::time::interval(Duration::from_millis(JITO_SEND_INTERVAL_MS));
    let mut endpoint_toggle = false; // false = east, true = west
    
    loop {
        tokio::select! {
            // Receive new transactions (non-blocking check)
            result = rx.recv() => {
                match result {
                    Some(base64_tx) => {
                        queue.push_back(base64_tx);
                    }
                    None => {
                        // Channel closed, exit loop
                        break;
                    }
                }
            }
            
            // Send on interval (1 RPS)
            _ = interval.tick() => {
                if let Some(base64_tx) = queue.pop_front() {
                    // Alternate between East (NY) and West (SLC)
                    let endpoint = if endpoint_toggle {
                        Arc::clone(&west_endpoint)
                    } else {
                        Arc::clone(&east_endpoint)
                    };
                    endpoint_toggle = !endpoint_toggle;
                    
                    // Record the send for RPS tracking
                    stats.record_send();
                    
                    // Fire off the send (don't wait for response)
                    let client = Arc::clone(&client);
                    
                    tokio::spawn(async move {
                        let _ = send_jito_transaction(&client, &endpoint, &base64_tx).await;
                    });
                    
                    // Transaction sent once, don't re-add to queue
                }
            }
        }
    }
}

/// Send a transaction to Helius endpoint (JSON-RPC format with options)
async fn send_helius_transaction(
    client: &reqwest::Client,
    endpoint: &str,
    base64_tx: &str,
) -> Result<(), SendError> {
    let request_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string();

    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": request_id,
        "method": "sendTransaction",
        "params": [
            base64_tx,
            {
                "encoding": "base64",
                "skipPreflight": true,
                "maxRetries": 0
            }
        ]
    });

    // Fire and forget - we don't care about the response
    let _ = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    Ok(())
}

/// Send a transaction to Jito block engine endpoint (JSON-RPC format)
async fn send_jito_transaction(
    client: &reqwest::Client,
    endpoint: &str,
    base64_tx: &str,
) -> Result<(), SendError> {
    let request_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();

    let body = serde_json::json!({
        "id": request_id,
        "jsonrpc": "2.0",
        "method": "sendTransaction",
        "params": [
            base64_tx,
            {
                "encoding": "base64"
            }
        ]
    });

    // Fire and forget - we don't care about the response
    let _ = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await;

    Ok(())
}

// =============================================================================
// Ping Monitor
// =============================================================================

/// Background loop that periodically pings both sender endpoints
/// Updates the shared PingStats with latency measurements
async fn ping_monitor_loop(stats: Arc<PingStats>) {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("Failed to create HTTP client for ping monitor");
    
    let mut interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL_SECS));
    
    loop {
        interval.tick().await;
        
        // Ping East endpoint
        let east_result = ping_endpoint(&client, HELIUS_EAST_PING).await;
        match east_result {
            Ok(latency_ms) => {
                stats.east_latency_ms.store(latency_ms, Ordering::Relaxed);
                stats.east_status.store(1, Ordering::Relaxed); // Connected
            }
            Err(_) => {
                stats.east_latency_ms.store(0, Ordering::Relaxed);
                stats.east_status.store(2, Ordering::Relaxed); // Disconnected
            }
        }
        
        // Ping West endpoint
        let west_result = ping_endpoint(&client, HELIUS_WEST_PING).await;
        match west_result {
            Ok(latency_ms) => {
                stats.west_latency_ms.store(latency_ms, Ordering::Relaxed);
                stats.west_status.store(1, Ordering::Relaxed); // Connected
            }
            Err(_) => {
                stats.west_latency_ms.store(0, Ordering::Relaxed);
                stats.west_status.store(2, Ordering::Relaxed); // Disconnected
            }
        }
        
        // Update last ping timestamp
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        stats.last_ping.store(now, Ordering::Relaxed);
    }
}

/// Ping a single endpoint and return latency in milliseconds
async fn ping_endpoint(client: &reqwest::Client, endpoint: &str) -> Result<u32, SendError> {
    let start = Instant::now();
    
    let response = client
        .get(endpoint)
        .send()
        .await
        .map_err(|e| SendError::Network(e.to_string()))?;
    
    // Check for successful response
    if !response.status().is_success() {
        return Err(SendError::Network(format!("Ping failed with status: {}", response.status())));
    }
    
    let elapsed = start.elapsed();
    let latency_ms = elapsed.as_millis() as u32;
    
    Ok(latency_ms)
}

// =============================================================================
// Helper Functions
// =============================================================================

use std::sync::atomic::AtomicUsize;

/// Counter for better randomization across rapid calls
static TIP_ACCOUNT_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Get a random Jito tip account pubkey
/// Uses atomic counter + nanos for better distribution across rapid calls
pub fn get_random_tip_account() -> Pubkey {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    let counter = TIP_ACCOUNT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as usize;
    
    // Combine counter and nanos for better distribution
    let index = (counter.wrapping_add(nanos)) % JITO_TIP_ACCOUNTS.len();
    Pubkey::from_str(JITO_TIP_ACCOUNTS[index]).unwrap()
}

/// Create a Jito tip instruction (simple SOL transfer to tip account)
pub fn create_tip_instruction(
    from: &Pubkey,
    tip_amount: u64,
) -> solana_sdk::instruction::Instruction {
    let tip_account = get_random_tip_account();
    solana_sdk::system_instruction::transfer(from, &tip_account, tip_amount)
}

// =============================================================================
// Error Types
// =============================================================================

#[derive(Debug, Clone)]
pub enum SendError {
    Serialization(String),
    Network(String),
    Parse(String),
    RpcError(String),
}

impl std::fmt::Display for SendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendError::Serialization(e) => write!(f, "Serialization error: {}", e),
            SendError::Network(e) => write!(f, "Network error: {}", e),
            SendError::Parse(e) => write!(f, "Parse error: {}", e),
            SendError::RpcError(e) => write!(f, "RPC error: {}", e),
        }
    }
}

impl std::error::Error for SendError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_random_tip_account() {
        let account = get_random_tip_account();
        let account_str = account.to_string();
        
        // Verify it's one of our tip accounts
        assert!(JITO_TIP_ACCOUNTS.contains(&account_str.as_str()));
    }

    #[test]
    fn test_tip_accounts_valid() {
        for account_str in JITO_TIP_ACCOUNTS.iter() {
            let result = Pubkey::from_str(account_str);
            assert!(result.is_ok(), "Invalid pubkey: {}", account_str);
        }
    }
}
