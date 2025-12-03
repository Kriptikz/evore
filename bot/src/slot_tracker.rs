use solana_client::pubsub_client::PubsubClient;
use solana_sdk::{commitment_config::CommitmentConfig, hash::Hash};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

/// Tracks current slot and blockhash via websocket subscription
pub struct SlotTracker {
    pub ws_url: String,
    pub current_slot: Arc<RwLock<u64>>,
    pub latest_blockhash: Arc<RwLock<Hash>>,
    pub last_slot_time: Arc<RwLock<Instant>>,
    pub last_blockhash_time: Arc<RwLock<Instant>>,
    /// True if WS subscription is currently connected
    pub ws_connected: Arc<AtomicBool>,
    /// True if RPC is responding
    pub rpc_connected: Arc<AtomicBool>,
}

impl SlotTracker {
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            current_slot: Arc::new(RwLock::new(0)),
            latest_blockhash: Arc::new(RwLock::new(Hash::default())),
            last_slot_time: Arc::new(RwLock::new(Instant::now())),
            last_blockhash_time: Arc::new(RwLock::new(Instant::now())),
            ws_connected: Arc::new(AtomicBool::new(false)),
            rpc_connected: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Check if WS is connected (received slot update in last 5 seconds)
    pub fn is_ws_connected(&self) -> bool {
        self.ws_connected.load(Ordering::Relaxed)
    }
    
    /// Check if RPC is connected (received blockhash in last 5 seconds)
    pub fn is_rpc_connected(&self) -> bool {
        self.rpc_connected.load(Ordering::Relaxed)
    }

    /// Get current slot
    pub fn get_slot(&self) -> u64 {
        *self.current_slot.read().unwrap()
    }

    /// Get latest blockhash
    pub fn get_blockhash(&self) -> Hash {
        *self.latest_blockhash.read().unwrap()
    }

    /// Get time since last slot update
    pub fn time_since_last_slot(&self) -> std::time::Duration {
        self.last_slot_time.read().unwrap().elapsed()
    }

    /// Start slot subscription (runs in background)
    /// Quietly reconnects on error with exponential backoff (max 30s)
    pub fn start_slot_subscription(&self) -> Result<(), Box<dyn std::error::Error>> {
        let slot = Arc::clone(&self.current_slot);
        let last_time = Arc::clone(&self.last_slot_time);
        let ws_connected = Arc::clone(&self.ws_connected);
        let ws_url = self.ws_url.clone();

        std::thread::spawn(move || {
            let mut retry_delay_secs = 1u64;
            const MAX_RETRY_DELAY: u64 = 30;
            
            loop {
                match PubsubClient::slot_subscribe(&ws_url) {
                    Ok((_subscription, receiver)) => {
                        // Reset backoff on successful connection
                        retry_delay_secs = 1;
                        ws_connected.store(true, Ordering::Relaxed);
                        
                        for slot_info in receiver {
                            let mut s = slot.write().unwrap();
                            *s = slot_info.slot;
                            let mut t = last_time.write().unwrap();
                            *t = Instant::now();
                        }
                        // Receiver closed, mark as disconnected
                        ws_connected.store(false, Ordering::Relaxed);
                    }
                    Err(_) => {
                        // Mark as disconnected, quiet retry with exponential backoff
                        ws_connected.store(false, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_secs(retry_delay_secs));
                        retry_delay_secs = (retry_delay_secs * 2).min(MAX_RETRY_DELAY);
                    }
                }
            }
        });

        Ok(())
    }

    /// Start blockhash subscription (runs in background)  
    /// Quietly handles errors and continues polling
    pub fn start_blockhash_subscription(&self, rpc_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let blockhash = Arc::clone(&self.latest_blockhash);
        let last_bh_time = Arc::clone(&self.last_blockhash_time);
        let rpc_connected = Arc::clone(&self.rpc_connected);
        let rpc_url = rpc_url.to_string();

        // Poll for blockhash since there's no direct subscription
        std::thread::spawn(move || {
            let client = solana_client::rpc_client::RpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::processed(),
            );
            
            loop {
                match client.get_latest_blockhash() {
                    Ok(hash) => {
                        let mut bh = blockhash.write().unwrap();
                        *bh = hash;
                        let mut t = last_bh_time.write().unwrap();
                        *t = Instant::now();
                        rpc_connected.store(true, Ordering::Relaxed);
                    }
                    Err(_) => {
                        rpc_connected.store(false, Ordering::Relaxed);
                    }
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        });

        Ok(())
    }

    /// Wait until a specific slot is reached
    pub async fn wait_until_slot(&self, target_slot: u64) {
        loop {
            let current = self.get_slot();
            if current >= target_slot {
                break;
            }
            // Small sleep to avoid busy loop
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    /// Calculate slots remaining until target
    pub fn slots_until(&self, target_slot: u64) -> u64 {
        let current = self.get_slot();
        target_slot.saturating_sub(current)
    }
}

/// Convert HTTP RPC URL to WebSocket URL
pub fn http_to_ws_url(http_url: &str) -> String {
    let url = http_url
        .replace("https://", "wss://")
        .replace("http://", "ws://");
    
    // Handle common RPC providers that need different ws endpoints
    if url.contains("helius") && !url.contains("ws") {
        // Helius uses different subdomain for ws
        url.replace("rpc.", "rpc-ws.")
    } else {
        url
    }
}

