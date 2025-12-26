//! ORE Token Holders Cache
//!
//! Maintains a live cache of all ORE token holders using Helius v2 API.
//! - Initial load: Fetches all holders via getProgramAccountsV2 with dataSlice
//! - Incremental updates: Uses changedSinceSlot for efficient updates

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use steel::Pubkey;
use tokio::sync::RwLock;
use tokio::time::interval;

use crate::helius_api::HeliusApi;

/// ORE token mint address (from evore::ore_api::MINT_ADDRESS)
pub fn ore_mint() -> Pubkey {
    evore::ore_api::MINT_ADDRESS
}

/// Cache for ORE token holders
pub struct OreTokenCache {
    helius: Arc<RwLock<HeliusApi>>,
    holders: Arc<RwLock<HashMap<Pubkey, u64>>>,
    /// Uses the slot cache from WebSocket for tracking changes
    slot_cache: Arc<RwLock<u64>>,
    /// Last slot we synced at
    last_sync_slot: Arc<RwLock<u64>>,
}

impl OreTokenCache {
    /// Create a new cache with shared state from AppState
    pub fn new(
        helius: Arc<RwLock<HeliusApi>>,
        holders: Arc<RwLock<HashMap<Pubkey, u64>>>,
        slot_cache: Arc<RwLock<u64>>,
    ) -> Self {
        Self {
            helius,
            holders,
            slot_cache,
            last_sync_slot: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Initial full load of all ORE token holders
    pub async fn initial_load(&self) -> Result<usize> {
        tracing::info!("Starting initial load of ORE token holders...");
        
        // Get current slot before fetch (for tracking)
        let current_slot = *self.slot_cache.read().await;
        
        let balances = {
            let mut helius = self.helius.write().await;
            helius.get_ore_token_balances(&ore_mint(), Some(5000)).await?
        };
        
        tracing::info!("Loaded {} ORE token holders", balances.len());
        
        // Update cache
        let mut cache = self.holders.write().await;
        cache.clear();
        for balance in &balances {
            cache.insert(balance.owner, balance.amount);
        }
        
        // Update last sync slot
        let mut sync_slot = self.last_sync_slot.write().await;
        *sync_slot = current_slot;
        
        Ok(balances.len())
    }
    
    /// Incremental update using changedSinceSlot
    pub async fn incremental_update(&self) -> Result<usize> {
        let since_slot = *self.last_sync_slot.read().await;
        
        if since_slot == 0 {
            // No initial load done yet
            return Ok(0);
        }
        
        // Get current slot before fetch
        let current_slot = *self.slot_cache.read().await;
        
        // No point fetching if slot hasn't advanced
        if current_slot <= since_slot {
            return Ok(0);
        }
        
        let changes = {
            let mut helius = self.helius.write().await;
            helius.get_ore_token_balances_changed_since(&ore_mint(), since_slot, Some(5000)).await?
        };
        
        if changes.is_empty() {
            // Update sync slot even if no changes
            let mut sync_slot = self.last_sync_slot.write().await;
            *sync_slot = current_slot;
            return Ok(0);
        }
        
        tracing::debug!("Updating {} ORE token holder balances (slot {} -> {})", 
            changes.len(), since_slot, current_slot);
        
        // Update cache
        let mut cache = self.holders.write().await;
        
        for balance in &changes {
            if balance.amount > 0 {
                cache.insert(balance.owner, balance.amount);
            } else {
                // Zero balance - could mean account closed
                // Remove from cache to save memory
                cache.remove(&balance.owner);
            }
        }
        
        let count = changes.len();
        
        // Update sync slot
        drop(cache);
        let mut sync_slot = self.last_sync_slot.write().await;
        *sync_slot = current_slot;
        
        Ok(count)
    }
    
    /// Get balance for a single owner
    pub async fn get_balance(&self, owner: &Pubkey) -> Option<u64> {
        let cache = self.holders.read().await;
        cache.get(owner).copied()
    }
    
    /// Get all holders as a list (optionally filtered by minimum balance)
    pub async fn get_all_holders(&self, min_balance: Option<u64>) -> Vec<(Pubkey, u64)> {
        let cache = self.holders.read().await;
        let min = min_balance.unwrap_or(0);
        
        cache.iter()
            .filter(|(_, &balance)| balance >= min)
            .map(|(&owner, &amount)| (owner, amount))
            .collect()
    }
    
    /// Get holders with pagination
    pub async fn get_holders_paginated(
        &self,
        offset: usize,
        limit: usize,
        min_balance: Option<u64>,
        sort_by_balance: bool,
    ) -> (Vec<(Pubkey, u64)>, usize) {
        let cache = self.holders.read().await;
        let min = min_balance.unwrap_or(0);
        
        let mut holders: Vec<_> = cache.iter()
            .filter(|(_, &balance)| balance >= min)
            .map(|(&owner, &amount)| (owner, amount))
            .collect();
        
        let total = holders.len();
        
        if sort_by_balance {
            holders.sort_by(|a, b| b.1.cmp(&a.1));
        }
        
        let page = holders
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();
        
        (page, total)
    }
    
    /// Get count of holders
    pub async fn holder_count(&self) -> usize {
        self.holders.read().await.len()
    }
    
    /// Get count of holders with non-zero balance
    pub async fn active_holder_count(&self) -> usize {
        self.holders.read().await
            .values()
            .filter(|&&b| b > 0)
            .count()
    }
    
    /// Spawn background update task
    pub fn spawn_update_task(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            // Wait a bit for WebSocket to establish and get slot
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            // First do initial load
            match self.initial_load().await {
                Ok(count) => {
                    tracing::info!("Initial ORE token holders load complete: {} holders", count);
                }
                Err(e) => {
                    tracing::error!("Failed initial ORE token holders load: {}", e);
                    // Retry after delay
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    if let Err(e) = self.initial_load().await {
                        tracing::error!("Retry of initial load also failed: {}", e);
                    }
                }
            }
            
            // Then run incremental updates every 10 seconds
            let mut ticker = interval(Duration::from_secs(10));
            
            loop {
                ticker.tick().await;
                
                match self.incremental_update().await {
                    Ok(count) if count > 0 => {
                        tracing::debug!("Updated {} ORE token holder balances", count);
                    }
                    Err(e) => {
                        tracing::warn!("ORE token holder update error: {}", e);
                    }
                    _ => {}
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ore_mint() {
        // Just verify the mint address parses correctly
        assert_ne!(ore_mint(), Pubkey::default());
    }
}
