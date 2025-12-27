//! Background tasks for ore-stats
//!
//! - RPC polling (every 2 seconds)
//! - Miners polling (every 30 seconds with incremental updates)
//! - Round transition detection
//! - Metrics snapshots

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use evore::ore_api::Miner;
use steel::Pubkey;
use tokio::time::interval;

use crate::app_state::{AppState, LiveRound};
use crate::helius_api::ProgramAccountV2;

/// Spawn the RPC polling task
/// Updates Board, Treasury, Round, and Miners caches every 2 seconds
pub fn spawn_rpc_polling(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        let mut last_round_id: u64 = 0;
        
        loop {
            ticker.tick().await;
            
            // Fetch Board
            match state.rpc.get_board().await {
                Ok(board) => {
                    let current_round_id = board.round_id;
                    
                    // Detect round transition
                    if last_round_id != 0 && current_round_id != last_round_id {
                        tracing::info!(
                            "Round transition detected: {} -> {}",
                            last_round_id, current_round_id
                        );
                        // TODO: Trigger round finalization for last_round_id
                    }
                    last_round_id = current_round_id;
                    
                    let mut cache = state.board_cache.write().await;
                    *cache = Some(board);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch board: {}", e);
                }
            }
            
            // Fetch Treasury
            match state.rpc.get_treasury().await {
                Ok(treasury) => {
                    let mut cache = state.treasury_cache.write().await;
                    *cache = Some(treasury);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch treasury: {}", e);
                }
            }
            
            // Fetch Round (if we have board)
            if let Some(board) = state.board_cache.read().await.as_ref() {
                let round_id = board.round_id;
                match state.rpc.get_round(round_id).await {
                    Ok(round) => {
                        let current_slot = *state.slot_cache.read().await;
                        let live_round = LiveRound::from_board_and_round(board, &round);
                        
                        let mut cache = state.round_cache.write().await;
                        let mut live = live_round;
                        live.update_slots_remaining(current_slot);
                        *cache = Some(live);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch round {}: {}", round_id, e);
                    }
                }
            }
            
            // Note: Miners are fetched by spawn_miners_polling() separately
        }
    })
}

/// Spawn miners polling task
/// Uses Helius v2 getProgramAccountsV2 for efficient bulk fetching
/// - Initial: Full fetch of all miners
/// - Subsequent: Incremental updates using changedSinceSlot (every 2s)
pub fn spawn_miners_polling(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        // Wait for slot to be available first
        tokio::time::sleep(Duration::from_secs(3)).await;
        
        let mut ticker = interval(Duration::from_secs(2));
        let mut initial_load_done = false;
        
        loop {
            ticker.tick().await;
            
            let current_slot = *state.slot_cache.read().await;
            let last_slot = *state.miners_last_slot.read().await;
            
            if !initial_load_done {
                // Initial full load
                tracing::info!("Starting initial miners cache load...");
                
                match fetch_all_miners(&state).await {
                    Ok(count) => {
                        tracing::info!("Initial miners cache loaded: {} miners at slot {}", count, current_slot);
                        initial_load_done = true;
                        
                        // Update last slot
                        let mut slot = state.miners_last_slot.write().await;
                        *slot = current_slot;
                    }
                    Err(e) => {
                        tracing::error!("Failed to load miners: {}", e);
                    }
                }
            } else if current_slot > last_slot {
                // Incremental update
                match fetch_miners_changed_since(&state, last_slot).await {
                    Ok(count) => {
                        if count > 0 {
                            tracing::debug!(
                                "Updated {} miners (slot {} -> {})",
                                count, last_slot, current_slot
                            );
                        }
                        
                        // Update last slot
                        let mut slot = state.miners_last_slot.write().await;
                        *slot = current_slot;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch miner updates: {}", e);
                    }
                }
            }
        }
    })
}

/// Fetch all miners using Helius v2
async fn fetch_all_miners(state: &AppState) -> anyhow::Result<usize> {
    let accounts = {
        let mut helius = state.helius.write().await;
        helius.get_all_ore_miners(Some(5000)).await?
    };
    
    // Use BTreeMap with String keys for sorted pagination
    let mut miners_map = BTreeMap::new();
    
    for acc in &accounts {
        if let Some((authority, miner)) = parse_miner_account(acc) {
            miners_map.insert(authority.to_string(), miner);
        }
    }
    
    let count = miners_map.len();
    
    // Update cache
    let mut cache = state.miners_cache.write().await;
    *cache = miners_map;
    
    Ok(count)
}

/// Fetch miners that changed since a slot
async fn fetch_miners_changed_since(state: &AppState, since_slot: u64) -> anyhow::Result<usize> {
    let accounts = {
        let mut helius = state.helius.write().await;
        helius.get_ore_miners_changed_since(since_slot, Some(5000)).await?
    };
    
    if accounts.is_empty() {
        return Ok(0);
    }
    
    let mut cache = state.miners_cache.write().await;
    let mut count = 0;
    
    for acc in &accounts {
        if let Some((authority, miner)) = parse_miner_account(acc) {
            // Insert with String key for sorted BTreeMap
            cache.insert(authority.to_string(), miner);
            count += 1;
        }
    }
    
    Ok(count)
}

/// Parse a Miner account from program account data
fn parse_miner_account(acc: &ProgramAccountV2) -> Option<(Pubkey, Miner)> {
    use base64::Engine as _;
    use steel::AccountDeserialize;
    
    // Get base64 data
    let data_b64 = acc.account.data.first()?;
    let data = base64::engine::general_purpose::STANDARD
        .decode(data_b64)
        .ok()?;
    
    // Parse miner - skip 8-byte discriminator
    if data.len() < 8 {
        return None;
    }
    
    let miner = Miner::try_from_bytes(&data).ok()?;
    
    // The authority is stored in the Miner account
    Some((miner.authority, *miner))
}

/// Spawn the metrics snapshot task
/// Stores server metrics to ClickHouse periodically
pub fn spawn_metrics_snapshot(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    use crate::clickhouse::ServerMetrics;
    
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        
        loop {
            ticker.tick().await;
            
            // Gather current cache sizes
            let miners_count = state.miners_cache.read().await.len();
            let ore_holders_count = state.ore_holders_cache.read().await.len();
            let slot = *state.slot_cache.read().await;
            
            // Get memory usage from /proc/self/statm (Linux) or process info
            let memory_used = get_memory_usage();
            
            // Query recent request stats from ClickHouse (last minute)
            let (requests_total, requests_success, requests_error, avg_latency) = 
                get_request_stats(&state.clickhouse).await.unwrap_or((0, 0, 0, 0.0));
            
            let metrics = ServerMetrics {
                requests_total,
                requests_success,
                requests_error,
                latency_p50: 0.0, // Would need histogram for percentiles
                latency_p95: 0.0,
                latency_p99: 0.0,
                latency_avg: avg_latency,
                active_connections: 0, // Would need connection tracking
                memory_used,
                cache_hits: (miners_count + ore_holders_count) as u64,
                cache_misses: 0,
            };
            
            if let Err(e) = state.clickhouse.insert_server_metrics(metrics).await {
                tracing::warn!("Failed to insert server metrics: {}", e);
            } else {
                tracing::debug!(
                    "Metrics snapshot: slot={}, miners={}, ore_holders={}, requests={}, mem={}MB",
                    slot, miners_count, ore_holders_count, requests_total, memory_used / 1024 / 1024
                );
            }
        }
    })
}

/// Get current process memory usage in bytes
fn get_memory_usage() -> u64 {
    #[cfg(target_os = "linux")]
    {
        // Read from /proc/self/statm for Linux
        if let Ok(content) = std::fs::read_to_string("/proc/self/statm") {
            let parts: Vec<&str> = content.split_whitespace().collect();
            if parts.len() >= 2 {
                // Second field is resident set size in pages
                if let Ok(rss_pages) = parts[1].parse::<u64>() {
                    let page_size = 4096u64; // Standard page size
                    return rss_pages * page_size;
                }
            }
        }
        0
    }
    
    #[cfg(not(target_os = "linux"))]
    {
        // Fallback for non-Linux systems
        0
    }
}

/// Get request stats from the last minute
async fn get_request_stats(clickhouse: &crate::clickhouse::ClickHouseClient) -> anyhow::Result<(u64, u64, u64, f32)> {
    let stats = clickhouse.get_recent_request_stats().await?;
    Ok((stats.total, stats.success, stats.errors, stats.avg_duration))
}

/// Spawn task to clean up stale data
pub fn spawn_cleanup_task(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(300)); // Every 5 minutes
        
        loop {
            ticker.tick().await;
            
            // Clean up miners that haven't played in a while
            // This is optional - depends on memory constraints
            
            tracing::debug!("Cleanup task completed");
        }
    })
}

