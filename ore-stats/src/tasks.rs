//! Background tasks for ore-stats
//!
//! - RPC polling (every 2 seconds) - Board, Treasury, Round
//! - Round transition detection and finalization
//! - Metrics snapshots
//! - EVORE accounts polling

use std::sync::Arc;
use std::time::Duration;

use tokio::time::interval;

use crate::app_state::{AppState, LiveRound};
use crate::finalization::{capture_round_snapshot, finalize_round};

/// Spawn the RPC polling task
/// Updates Board, Treasury, Round caches every 2 seconds
/// Also handles round transition detection, snapshot capture, and finalization
pub fn spawn_rpc_polling(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
        // Skip missed ticks during long operations like GPA snapshots
        // to avoid burst of RPC calls when the operation completes
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        
        let mut last_round_id: u64 = 0;
        let mut round_ending_detected = false;
        let mut initialized = false;
        
        loop {
            ticker.tick().await;
            
            // Fetch Board
            let board_result = state.rpc.get_board().await;
            let current_board = match board_result {
                Ok(board) => {
                    let mut cache = state.board_cache.write().await;
                    *cache = Some(board);
                    Some(board)
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch board: {}", e);
                    None
                }
            };
            
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
            
            // Fetch Round and handle transitions
            if let Some(board) = current_board {
                let current_round_id = board.round_id;
                
                // Initialize pending_round_id on first successful fetch
                // This is critical for WebSocket deployment tracking
                if !initialized && current_round_id > 0 {
                    *state.pending_round_id.write().await = current_round_id;
                    last_round_id = current_round_id;
                    initialized = true;
                    tracing::info!(
                        "Initialized pending_round_id to {} for deployment tracking",
                        current_round_id
                    );
                }
                
                match state.rpc.get_round(current_round_id).await {
                    Ok(round) => {
                        let current_slot = *state.slot_cache.read().await;
                        let live_round = LiveRound::from_board_and_round(&board, &round);
                        
                        let mut cache = state.round_cache.write().await;
                        let mut live = live_round;
                        live.update_slots_remaining(current_slot);
                        
                        // Check if round is ending (slots_remaining <= 0)
                        if live.slots_remaining <= 0 && !round_ending_detected {
                            tracing::info!(
                                "Round {} ending detected (slots_remaining={}), capturing snapshot...",
                                current_round_id, live.slots_remaining
                            );
                            round_ending_detected = true;
                            
                            // Capture snapshot before round resets
                            drop(cache); // Release lock before async operation
                            if let Some(snapshot) = capture_round_snapshot(&state).await {
                                let mut snapshot_cache = state.round_snapshot.write().await;
                                *snapshot_cache = Some(snapshot);
                                tracing::info!("Round {} snapshot captured", current_round_id);
                            } else {
                                tracing::warn!("Failed to capture snapshot for round {}", current_round_id);
                            }
                            
                            // Re-acquire cache lock
                            cache = state.round_cache.write().await;
                        }
                        
                        *cache = Some(live);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch round {}: {}", current_round_id, e);
                    }
                }
                
                // Detect round transition (new round started)
                if last_round_id != 0 && current_round_id != last_round_id {
                    tracing::info!(
                        "Round transition detected: {} -> {}",
                        last_round_id, current_round_id
                    );
                    
                    // Finalize the previous round using captured snapshot
                    let snapshot_opt = {
                        let mut snapshot_cache = state.round_snapshot.write().await;
                        snapshot_cache.take()
                    };
                    
                    if let Some(snapshot) = snapshot_opt {
                        if snapshot.round_id == last_round_id {
                            // Spawn finalization in background (don't block polling)
                            let state_clone = state.clone();
                            tokio::spawn(async move {
                                // Wait a moment for slot_hash to be populated
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                
                                match finalize_round(&state_clone, snapshot).await {
                                    Ok(()) => {
                                        tracing::info!("Round {} finalized successfully", last_round_id);
                                    }
                                    Err(e) => {
                                        tracing::error!("Failed to finalize round {}: {}", last_round_id, e);
                                    }
                                }
                                
                                // Always refresh EVORE cache and fetch transactions,
                                // regardless of finalization success/failure
                                crate::evore_cache::refresh_evore_cache(&state_clone).await;
                                
                                // Fetch and store transactions for the round
                                let state_for_txns = state_clone.clone();
                                let round_for_txns = last_round_id;
                                tokio::spawn(async move {
                                    if let Err(e) = crate::finalization::fetch_and_store_round_transactions(&state_for_txns, round_for_txns).await {
                                        tracing::warn!("Failed to fetch transactions for round {}: {}", round_for_txns, e);
                                    } else {
                                        tracing::info!("Stored transactions for round {}", round_for_txns);
                                    }
                                });
                            });
                        } else {
                            tracing::warn!(
                                "Snapshot round_id {} doesn't match expected {}",
                                snapshot.round_id, last_round_id
                            );
                        }
                    } else {
                        tracing::error!(
                            "CRITICAL: Failed to capture snapshot for round {}! \
                             GPA miners snapshot failed after all retries. \
                             Round will need to be reconstructed via admin panel.",
                            last_round_id
                        );
                    }
                    
                    // Clear deployment tracking for new round
                    state.pending_deployments.write().await.clear();
                    *state.pending_round_id.write().await = current_round_id;
                    
                    // Clear deployments cache for new round
                    state.deployments_cache.write().await.clear();
                    *state.deployments_cache_round_id.write().await = current_round_id;
                    
                    round_ending_detected = false;
                }
                
                last_round_id = current_round_id;
            }
            
            // Note: Miners cache is populated at startup and refreshed during GPA snapshot
        }
    })
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
            let stats = state.clickhouse.get_recent_request_stats().await
                .unwrap_or_default();
            
            let metrics = ServerMetrics {
                requests_total: stats.total,
                requests_success: stats.success,
                requests_error: stats.errors,
                latency_p50: stats.p50,
                latency_p95: stats.p95,
                latency_p99: stats.p99,
                latency_avg: stats.avg_duration,
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
                    slot, miners_count, ore_holders_count, stats.total, memory_used / 1024 / 1024
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
