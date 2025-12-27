//! WebSocket subscriptions for real-time data
//!
//! Provides:
//! - Slot subscription for live slot tracking
//! - Account subscriptions for SSE broadcasting

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use anyhow::Result;
use futures_util::StreamExt;
use solana_client::nonblocking::pubsub_client::PubsubClient;
use solana_sdk::commitment_config::CommitmentConfig;
use tokio::sync::RwLock;
use tokio::time::{interval, Instant};

use crate::app_state::{AppState, LiveBroadcastData, LiveDeployment, LiveRound};
use crate::clickhouse::{ClickHouseClient, WsEventInsert, WsThroughputInsert};

/// WebSocket manager for all subscriptions
pub struct WebSocketManager {
    ws_url: String,
    clickhouse: Option<Arc<ClickHouseClient>>,
    provider_name: String,
}

impl WebSocketManager {
    pub fn new(ws_url: String) -> Self {
        Self::with_clickhouse(ws_url, None)
    }
    
    pub fn with_clickhouse(ws_url: String, clickhouse: Option<Arc<ClickHouseClient>>) -> Self {
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
        
        let provider_name = extract_provider_name(&ws_url);
        
        Self { ws_url, clickhouse, provider_name }
    }
    
    /// Log a WebSocket event to ClickHouse
    fn log_ws_event(&self, subscription_type: &str, subscription_key: &str, event: &str, 
                    error_message: &str, disconnect_reason: &str, uptime_seconds: u32,
                    messages_received: u64, reconnect_count: u16) {
        if let Some(ref ch) = self.clickhouse {
            let insert = WsEventInsert {
                program: "ore-stats".to_string(),
                provider: self.provider_name.clone(),
                subscription_type: subscription_type.to_string(),
                subscription_key: subscription_key.to_string(),
                event: event.to_string(),
                error_message: error_message.to_string(),
                disconnect_reason: disconnect_reason.to_string(),
                uptime_seconds,
                messages_received,
                reconnect_count,
            };
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_ws_event(insert).await {
                    tracing::warn!("Failed to log WS event: {}", e);
                }
            });
        }
    }
    
    /// Start the slot subscription task
    /// Updates the slot cache in AppState
    pub fn spawn_slot_subscription(
        &self,
        slot_cache: Arc<RwLock<u64>>,
    ) -> tokio::task::JoinHandle<()> {
        let ws_url = self.ws_url.clone();
        let clickhouse = self.clickhouse.clone();
        let provider_name = self.provider_name.clone();
        
        tokio::spawn(async move {
            let mut reconnect_count: u16 = 0;
            
            loop {
                tracing::info!("Connecting to slot subscription at {}", ws_url);
                
                // Log connect event
                log_ws_event_async(&clickhouse, &provider_name, "slot", "", "connecting", "", "", 0, 0, reconnect_count);
                
                let start_time = Instant::now();
                let messages_received = Arc::new(AtomicU64::new(0));
                let messages_ref = messages_received.clone();
                
                match subscribe_to_slot_with_metrics(&ws_url, slot_cache.clone(), messages_ref, &clickhouse, &provider_name).await {
                    Ok(_) => {
                        let uptime = start_time.elapsed().as_secs() as u32;
                        let msgs = messages_received.load(Ordering::Relaxed);
                        log_ws_event_async(&clickhouse, &provider_name, "slot", "", "disconnected", "", "stream_ended", uptime, msgs, reconnect_count);
                        tracing::warn!("Slot subscription ended unexpectedly, reconnecting...");
                    }
                    Err(e) => {
                        let uptime = start_time.elapsed().as_secs() as u32;
                        let msgs = messages_received.load(Ordering::Relaxed);
                        log_ws_event_async(&clickhouse, &provider_name, "slot", "", "error", &e.to_string(), "error", uptime, msgs, reconnect_count);
                        tracing::error!("Slot subscription error: {}, reconnecting in 5s...", e);
                    }
                }
                
                reconnect_count = reconnect_count.saturating_add(1);
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

/// Helper to log WS events from async contexts
fn log_ws_event_async(
    clickhouse: &Option<Arc<ClickHouseClient>>,
    provider_name: &str,
    subscription_type: &str,
    subscription_key: &str,
    event: &str,
    error_message: &str,
    disconnect_reason: &str,
    uptime_seconds: u32,
    messages_received: u64,
    reconnect_count: u16,
) {
    if let Some(ref ch) = clickhouse {
        let insert = WsEventInsert {
            program: "ore-stats".to_string(),
            provider: provider_name.to_string(),
            subscription_type: subscription_type.to_string(),
            subscription_key: subscription_key.to_string(),
            event: event.to_string(),
            error_message: error_message.to_string(),
            disconnect_reason: disconnect_reason.to_string(),
            uptime_seconds,
            messages_received,
            reconnect_count,
        };
        
        let ch = ch.clone();
        tokio::spawn(async move {
            if let Err(e) = ch.insert_ws_event(insert).await {
                tracing::warn!("Failed to log WS event: {}", e);
            }
        });
    }
}

/// Extract provider name from WS URL for metrics
fn extract_provider_name(url: &str) -> String {
    if url.contains("helius") {
        "helius".to_string()
    } else if url.contains("quicknode") {
        "quicknode".to_string()
    } else if url.contains("alchemy") {
        "alchemy".to_string()
    } else if url.contains("triton") {
        "triton".to_string()
    } else if url.contains("localhost") || url.contains("127.0.0.1") {
        "localhost".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Subscribe to slot updates via WebSocket with message counting
async fn subscribe_to_slot_with_metrics(
    ws_url: &str,
    slot_cache: Arc<RwLock<u64>>,
    messages_received: Arc<AtomicU64>,
    clickhouse: &Option<Arc<ClickHouseClient>>,
    provider_name: &str,
) -> Result<()> {
    let client = PubsubClient::new(ws_url).await?;
    
    let (mut stream, _unsub) = client.slot_subscribe().await?;
    
    // Log connected event
    log_ws_event_async(clickhouse, provider_name, "slot", "", "connected", "", "", 0, 0, 0);
    tracing::info!("Slot subscription established");
    
    while let Some(slot_info) = stream.next().await {
        let slot = slot_info.slot;
        messages_received.fetch_add(1, Ordering::Relaxed);
        
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
    rpc_url: &str,
    state: Arc<AppState>,
    clickhouse: Option<Arc<ClickHouseClient>>,
) -> Result<()> {
    use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
    use evore::ore_api::{Miner, Round, id as ore_program_id};
    use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
    use solana_account_decoder_client_types::UiAccountEncoding;
    use steel::AccountDeserialize;
    
    // Convert RPC URL to WebSocket URL
    let ws_url = if rpc_url.starts_with("wss://") || rpc_url.starts_with("ws://") {
        rpc_url.to_string()
    } else if rpc_url.starts_with("https://") {
        rpc_url.replace("https://", "wss://")
    } else if rpc_url.starts_with("http://") {
        rpc_url.replace("http://", "ws://")
    } else {
        format!("wss://{}", rpc_url)
    };
    
    let provider_name = extract_provider_name(&ws_url);
    let start_time = Instant::now();
    let messages_received = AtomicU64::new(0);
    
    // Log connecting event
    log_ws_event_async(&clickhouse, &provider_name, "program", &evore::ore_api::PROGRAM_ID.to_string(), 
                       "connecting", "", "", 0, 0, 0);
    
    let client = PubsubClient::new(&ws_url).await?;
    
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
    
    log_ws_event_async(&clickhouse, &provider_name, "program", &evore::ore_api::PROGRAM_ID.to_string(), 
                       "connected", "", "", 0, 0, 0);
    tracing::info!("ORE program subscription established");
    
    // Wait for RPC polling to initialize pending_round_id (max 30 seconds)
    // This ensures we don't miss deployments due to race condition at startup
    let mut current_round_id: u64 = 0;
    for wait_attempt in 1..=15 {
        current_round_id = *state.pending_round_id.read().await;
        if current_round_id > 0 {
            tracing::info!(
                "WebSocket deployment tracking initialized with round_id={} (attempt {})",
                current_round_id, wait_attempt
            );
            break;
        }
        tracing::debug!(
            "Waiting for RPC polling to set pending_round_id (attempt {}/15)...",
            wait_attempt
        );
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    
    if current_round_id == 0 {
        tracing::warn!(
            "WebSocket starting without valid round_id - deployments won't be tracked until Round update received"
        );
    }
    
    // Throughput sampling interval (every 10 seconds)
    let mut throughput_interval = tokio::time::interval(Duration::from_secs(10));
    throughput_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut last_message_count: u64 = 0;
    let throughput_subscription = "program".to_string();
    
    // Sync interval (every 5 seconds) - ensures we stay in sync with RPC polling
    let mut sync_interval = tokio::time::interval(Duration::from_secs(5));
    sync_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    
    loop {
        tokio::select! {
            Some(response) = stream.next() => {
                messages_received.fetch_add(1, Ordering::Relaxed);
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
            // Only process if this is a NEW round (id > current)
            // Ignore old rounds from checkpoints
            if round.id > current_round_id {
                tracing::info!("New round detected: {} -> {}", current_round_id, round.id);
                current_round_id = round.id;
                
                // Update state and clear deployment tracking for new round
                *state.pending_round_id.write().await = round.id;
                state.pending_deployments.write().await.clear();
            }
            
            // Only update round cache if this is the current round
            if round.id == current_round_id {
                if let Some(board) = state.board_cache.read().await.as_ref() {
                    let pending = state.pending_deployments.read().await;
                    let mut live_round = LiveRound::from_board_and_round(board, round);
                    live_round.unique_miners = pending.len() as u32;
                    drop(pending);
                    
                    let mut cache = state.round_cache.write().await;
                    *cache = Some(live_round);
                }
            }
            // Ignore old round updates (checkpoints, etc.)
        }
        // Try to parse as Miner
        else if let Ok(miner) = Miner::try_from_bytes(&data) {
            // Only process if this miner deployed in the current round
            if miner.round_id == current_round_id && current_round_id > 0 {
                let miner_pubkey = miner.authority.to_string();
                
                // Get pending deployments for this miner
                let mut pending = state.pending_deployments.write().await;
                let miner_squares = pending
                    .entry(miner_pubkey.clone())
                    .or_insert_with(std::collections::HashMap::new);
                
                // Check each square for NEW deployments
                for (square_id, &amount) in miner.deployed.iter().enumerate() {
                    if amount > 0 {
                        let square_id_u8 = square_id as u8;
                        
                        // Only broadcast if this is a NEW deployment on this square
                        // (miner can only deploy once per square per round)
                        if !miner_squares.contains_key(&square_id_u8) {
                            // Record this deployment with slot (for Phase 2 finalization)
                            miner_squares.insert(square_id_u8, (amount, slot));
                            
                            let deployment = LiveDeployment {
                                round_id: current_round_id,
                                miner_pubkey: miner_pubkey.clone(),
                                square_id: square_id_u8,
                                amount,
                                slot,
                            };
                            
                            // Broadcast deployment
                            let _ = state.deployment_broadcast.send(
                                LiveBroadcastData::Deployment(deployment)
                            );
                        }
                    }
                }
                
                // Update unique miners count
                let unique_count = pending.len();
                drop(pending);
                
                if let Some(round) = state.round_cache.write().await.as_mut() {
                    round.unique_miners = unique_count as u32;
                }
            }
            
            // Always update miner cache (for any round - keeps cache fresh)
            // BTreeMap keyed by authority string for sorted pagination
            let mut cache = state.miners_cache.write().await;
            cache.insert(miner.authority.to_string(), *miner);
        }
            }
            _ = sync_interval.tick() => {
                // Sync current_round_id with state (in case RPC polling detected a transition we missed)
                let state_round_id = *state.pending_round_id.read().await;
                if state_round_id > current_round_id && state_round_id > 0 {
                    tracing::info!(
                        "WebSocket syncing round_id from state: {} -> {}",
                        current_round_id, state_round_id
                    );
                    current_round_id = state_round_id;
                }
            }
            _ = throughput_interval.tick() => {
                // Sample throughput every 10 seconds
                let current_count = messages_received.load(Ordering::Relaxed);
                let messages_in_window = current_count.saturating_sub(last_message_count);
                last_message_count = current_count;
                
                let elapsed = start_time.elapsed().as_secs();
                let avg_rate = if elapsed > 0 {
                    current_count as f64 / elapsed as f64
                } else {
                    0.0
                };
                
                // Log throughput to ClickHouse
                if let Some(ref ch) = clickhouse {
                    let throughput = WsThroughputInsert {
                        program: "ore-stats".to_string(),
                        provider: provider_name.clone(),
                        subscription_type: throughput_subscription.clone(),
                        messages_received: messages_in_window as u32,
                        bytes_received: 0, // Not tracked at this level
                        avg_process_time_us: 0, // Not tracking processing time
                    };
                    
                    if let Err(e) = ch.insert_ws_throughput(throughput).await {
                        tracing::warn!("Failed to log WS throughput: {}", e);
                    }
                }
                
                tracing::debug!(
                    "WS throughput: {} msgs in 10s window, avg {:.2} msg/s",
                    messages_in_window, avg_rate
                );
            }
            else => {
                // Stream closed
                break;
            }
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

