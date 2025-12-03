//! BlockhashCache - Periodic RPC fetch for latest blockhash
//!
//! Features:
//! - Normal mode: fetches every 2 seconds
//! - Fast mode: fetches every 500ms when slots_left < 10
//! - Shared via Arc for all bots

use solana_client::rpc_client::RpcClient;
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::hash::Hash;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

/// Cache for latest blockhash with adaptive refresh rate
pub struct BlockhashCache {
    rpc_url: String,
    blockhash: Arc<RwLock<Hash>>,
    last_update: Arc<RwLock<Instant>>,
    /// When true, uses fast refresh (500ms instead of 2s)
    fast_mode: Arc<AtomicBool>,
    /// Current end_slot for calculating slots_left
    end_slot: Arc<AtomicU64>,
    /// Current slot for calculating slots_left
    current_slot: Arc<AtomicU64>,
}

impl BlockhashCache {
    pub fn new(rpc_url: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            blockhash: Arc::new(RwLock::new(Hash::default())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            fast_mode: Arc::new(AtomicBool::new(false)),
            end_slot: Arc::new(AtomicU64::new(u64::MAX)),
            current_slot: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get latest cached blockhash
    pub fn get_blockhash(&self) -> Hash {
        *self.blockhash.read().unwrap()
    }

    /// Get time since last blockhash update
    pub fn time_since_update(&self) -> Duration {
        self.last_update.read().unwrap().elapsed()
    }

    /// Update current slot (call this from SlotTracker updates)
    pub fn set_current_slot(&self, slot: u64) {
        self.current_slot.store(slot, Ordering::Relaxed);
        self.update_fast_mode();
    }

    /// Update end slot (call this from BoardTracker updates)
    pub fn set_end_slot(&self, end_slot: u64) {
        self.end_slot.store(end_slot, Ordering::Relaxed);
        self.update_fast_mode();
    }

    /// Check and update fast mode based on slots_left
    fn update_fast_mode(&self) {
        let end = self.end_slot.load(Ordering::Relaxed);
        let current = self.current_slot.load(Ordering::Relaxed);
        
        // Fast mode when within 10 slots of end
        let slots_left = end.saturating_sub(current);
        let should_be_fast = slots_left < 10 && end != u64::MAX;
        
        self.fast_mode.store(should_be_fast, Ordering::Relaxed);
    }

    /// Check if in fast mode
    pub fn is_fast_mode(&self) -> bool {
        self.fast_mode.load(Ordering::Relaxed)
    }

    /// Start background polling thread
    /// Quietly handles RPC errors and continues polling
    pub fn start_polling(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let blockhash = Arc::clone(&self.blockhash);
        let last_update = Arc::clone(&self.last_update);
        let fast_mode = Arc::clone(&self.fast_mode);
        let rpc_url = self.rpc_url.clone();

        std::thread::spawn(move || {
            let client = RpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::processed(),
            );

            loop {
                // Fetch blockhash - silently ignore errors (will retry on next poll)
                if let Ok(hash) = client.get_latest_blockhash() {
                    {
                        let mut bh = blockhash.write().unwrap();
                        *bh = hash;
                    }
                    {
                        let mut t = last_update.write().unwrap();
                        *t = Instant::now();
                    }
                }

                // Adaptive sleep based on fast mode
                let sleep_ms = if fast_mode.load(Ordering::Relaxed) {
                    500
                } else {
                    2000
                };
                std::thread::sleep(Duration::from_millis(sleep_ms));
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blockhash_cache_new() {
        let cache = BlockhashCache::new("https://example.com");
        assert_eq!(cache.get_blockhash(), Hash::default());
        assert!(!cache.is_fast_mode());
    }

    #[test]
    fn test_fast_mode_activation() {
        let cache = BlockhashCache::new("https://example.com");
        
        // Not fast when end_slot is MAX
        cache.set_end_slot(u64::MAX);
        cache.set_current_slot(100);
        assert!(!cache.is_fast_mode());

        // Fast when within 10 slots
        cache.set_end_slot(105);
        cache.set_current_slot(100);
        assert!(cache.is_fast_mode());

        // Not fast when more than 10 slots away
        cache.set_current_slot(90);
        assert!(!cache.is_fast_mode());
    }
}
