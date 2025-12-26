//! WebSocket subscriptions for real-time data
//!
//! Provides:
//! - Slot subscription for live slot tracking
//! - Account subscriptions for SSE broadcasting

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::commitment_config::CommitmentConfig;
use steel::Pubkey;
use tokio::sync::RwLock;
use tokio::time::interval;

use crate::app_state::{AppState, LiveBroadcastData, LiveDeployment, LiveRound};

/// WebSocket manager for all subscriptions
pub struct WebSocketManager {
    ws_url: String,
}

impl WebSocketManager {
    pub fn new(ws_url: String) -> Self {
        // Convert RPC URL to WebSocket URL if needed
        let ws_url = if ws_url.starts_with("wss://") || ws_url.starts_with("ws://") {
            ws_url
        } else if ws_url.starts_with("https://") {
            ws_url.replace("https://", "wss://")
        } else if ws_url.starts_with("http://") {
            ws_url.replace("http://", "ws://")
        } else {
            format!("wss://{}", ws_url)
        };
        
        Self { ws_url }
    }
    
    /// Start the slot subscription task
    /// Updates the slot cache in AppState
    pub fn spawn_slot_subscription(
        &self,
        slot_cache: Arc<RwLock<u64>>,
    ) -> tokio::task::JoinHandle<()> {
        let ws_url = self.ws_url.clone();
        
        tokio::spawn(async move {
            loop {
                tracing::info!("Connecting to slot subscription at {}", ws_url);
                
                match subscribe_to_slot(&ws_url, slot_cache.clone()).await {
                    Ok(_) => {
                        tracing::warn!("Slot subscription ended unexpectedly, reconnecting...");
                    }
                    Err(e) => {
                        tracing::error!("Slot subscription error: {}, reconnecting in 5s...", e);
                    }
                }
                
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        })
    }
    
    /// Start the round broadcast task
    /// Sends round updates to SSE clients at a throttled rate
    pub fn spawn_round_broadcaster(
        &self,
        state: Arc<AppState>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(500));
            
            loop {
                interval.tick().await;
                
                // Read current round and slot
                let round_opt = state.round_cache.read().await.clone();
                let current_slot = *state.slot_cache.read().await;
                
                if let Some(mut round) = round_opt {
                    round.update_slots_remaining(current_slot);
                    
                    // Broadcast to SSE subscribers
                    let _ = state.round_broadcast.send(LiveBroadcastData::Round(round));
                }
            }
        })
    }
}

/// Subscribe to slot updates via WebSocket
async fn subscribe_to_slot(
    ws_url: &str,
    slot_cache: Arc<RwLock<u64>>,
) -> Result<()> {
    let client = PubsubClient::new(ws_url).await?;
    
    let (mut stream, _unsub) = client.slot_subscribe().await?;
    
    tracing::info!("Slot subscription established");
    
    while let Some(slot_info) = stream.next().await {
        let slot = slot_info.slot;
        
        // Update the cache
        let mut cache = slot_cache.write().await;
        *cache = slot;
        
        // Log occasionally for monitoring
        if slot % 100 == 0 {
            tracing::debug!("Current slot: {}", slot);
        }
    }
    
    Ok(())
}

/// Subscribe to ORE program account changes
/// Used for SSE deployment broadcasting
pub async fn subscribe_to_program_accounts(
    ws_url: &str,
    state: Arc<AppState>,
) -> Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use evore::ore_api::{Miner, Round, id as ore_program_id};
    use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
    use solana_account_decoder_client_types::UiAccountEncoding;
    use steel::AccountDeserialize;
    
    let client = PubsubClient::new(ws_url).await?;
    
    let config = RpcProgramAccountsConfig {
        filters: None,
        account_config: RpcAccountInfoConfig {
            encoding: Some(UiAccountEncoding::Base64),
            data_slice: None,
            commitment: Some(CommitmentConfig::confirmed()),
            min_context_slot: None,
        },
        with_context: Some(true),
        sort_results: None,
    };
    
    let (mut stream, _unsub) = client
        .program_subscribe(&ore_program_id(), Some(config))
        .await?;
    
    tracing::info!("ORE program subscription established");
    
    // Track unique miners per round for live count
    let mut current_round_id: u64 = 0;
    let mut round_miners: std::collections::HashSet<String> = std::collections::HashSet::new();
    
    while let Some(response) = stream.next().await {
        let slot = response.context.slot;
        let account = response.value;
        
        // Decode base64 account data
        let data = match &account.account.data {
            solana_account_decoder_client_types::UiAccountData::Binary(b64, _) => {
                match BASE64.decode(b64) {
                    Ok(bytes) => bytes,
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        
        // Try to parse as Round
        if let Ok(round) = Round::try_from_bytes(&data) {
            // Check if new round started
            if round.id != current_round_id {
                current_round_id = round.id;
                round_miners.clear();
                tracing::info!("New round detected: {}", round.id);
            }
            
            // Update round cache
            if let Some(board) = state.board_cache.read().await.as_ref() {
                let mut live_round = LiveRound::from_board_and_round(board, round);
                live_round.unique_miners = round_miners.len() as u32;
                
                let mut cache = state.round_cache.write().await;
                *cache = Some(live_round);
            }
        }
        // Try to parse as Miner
        else if let Ok(miner) = Miner::try_from_bytes(&data) {
            // Check if this miner deployed in current round
            if miner.round_id == current_round_id {
                let miner_pubkey = miner.authority.to_string();
                let is_new = round_miners.insert(miner_pubkey.clone());
                
                // Find which square they deployed to
                for (square_id, &amount) in miner.deployed.iter().enumerate() {
                    if amount > 0 {
                        let deployment = LiveDeployment {
                            round_id: current_round_id,
                            miner_pubkey: miner_pubkey.clone(),
                            square_id: square_id as u8,
                            amount,
                            slot,
                        };
                        
                        // Broadcast deployment
                        let _ = state.deployment_broadcast.send(
                            LiveBroadcastData::Deployment(deployment)
                        );
                    }
                }
                
                // Update unique miners count if new
                if is_new {
                    if let Some(round) = state.round_cache.write().await.as_mut() {
                        round.unique_miners = round_miners.len() as u32;
                    }
                }
            }
            
            // Update miner cache
            let mut cache = state.miners_cache.write().await;
            cache.insert(miner.authority, *miner);
        }
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_ws_url_conversion() {
        let manager = WebSocketManager::new("https://rpc.helius.xyz".to_string());
        assert!(manager.ws_url.starts_with("wss://"));
        
        let manager = WebSocketManager::new("wss://rpc.helius.xyz".to_string());
        assert!(manager.ws_url.starts_with("wss://"));
        
        let manager = WebSocketManager::new("rpc.helius.xyz".to_string());
        assert!(manager.ws_url.starts_with("wss://"));
    }
}

