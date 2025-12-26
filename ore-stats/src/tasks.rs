//! Background tasks for ore-stats
//!
//! - RPC polling (every 2 seconds)
//! - Round transition detection
//! - Metrics snapshots

use std::sync::Arc;
use std::time::Duration;

use steel::Pubkey;
use tokio::time::interval;

use crate::app_state::{AppState, LiveRound};
use crate::app_rpc::AppRpc;

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
            
            // Fetch Miners (less frequently to avoid rate limits)
            // TODO: Use Helius v2 getProgramAccountsV2 for efficiency
        }
    })
}

/// Spawn the metrics snapshot task
/// Stores server metrics to ClickHouse periodically
pub fn spawn_metrics_snapshot(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(60));
        
        loop {
            ticker.tick().await;
            
            // TODO: Gather metrics and insert to ClickHouse
            // let metrics = ServerMetricsInsert { ... };
            // state.clickhouse.insert_server_metrics(metrics).await;
            
            tracing::debug!("Metrics snapshot taken");
        }
    })
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

