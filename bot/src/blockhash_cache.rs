//! BlockhashCache - Periodic RPC fetch for latest blockhash
//!
//! Features:
//! - Polls every 1 second for fresh blockhash
//! - Uses shared RPS tracker for unified request monitoring
//! - Shared via Arc for all bots

use solana_sdk::hash::Hash;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use crate::client::{EvoreClient, RpsTracker};

/// Cache for latest blockhash
pub struct BlockhashCache {
    blockhash: Arc<RwLock<Hash>>,
    last_update: Arc<RwLock<Instant>>,
    rpc_url: String,
    rps_tracker: Arc<RpsTracker>,
}

impl BlockhashCache {
    pub fn new(rpc_url: &str, rps_tracker: Arc<RpsTracker>) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            blockhash: Arc::new(RwLock::new(Hash::default())),
            last_update: Arc::new(RwLock::new(Instant::now())),
            rps_tracker,
        }
    }

    /// Get latest cached blockhash
    pub fn get_blockhash(&self) -> Hash {
        *self.blockhash.read().unwrap()
    }

    /// Start background polling thread
    /// Quietly handles RPC errors and continues polling
    pub fn start_polling(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let blockhash = Arc::clone(&self.blockhash);
        let last_update = Arc::clone(&self.last_update);
        let rpc_url = self.rpc_url.clone();
        let rps_tracker = Arc::clone(&self.rps_tracker);

        std::thread::spawn(move || {
            // Create client with processed commitment for fresher blockhash
            let client = EvoreClient::new_processed(&rpc_url, rps_tracker);

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

                // Poll every 1 second
                std::thread::sleep(Duration::from_millis(1000));
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
        let tracker = Arc::new(RpsTracker::new());
        let cache = BlockhashCache::new("https://example.com", tracker);
        assert_eq!(cache.get_blockhash(), Hash::default());
    }
}
