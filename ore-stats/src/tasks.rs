//! Background tasks for ore-stats
//!
//! - RPC polling (every 2 seconds)
//! - Miners polling (every 30 seconds with incremental updates)
//! - Round transition detection and finalization
//! - Metrics snapshots

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use evore::ore_api::Miner;
use steel::Pubkey;
use tokio::time::interval;

use crate::app_state::{AppState, LiveRound};
use crate::finalization::{capture_round_snapshot, finalize_round};
use crate::helius_api::ProgramAccountV2;

/// Spawn the RPC polling task
/// Updates Board, Treasury, Round, and Miners caches every 2 seconds
/// Also handles round transition detection, snapshot capture, and finalization
pub fn spawn_rpc_polling(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_secs(2));
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
                            });
                        } else {
                            tracing::warn!(
                                "Snapshot round_id {} doesn't match expected {}",
                                snapshot.round_id, last_round_id
                            );
                        }
                    } else {
                        tracing::warn!("No snapshot available for round {} finalization", last_round_id);
                    }
                    
                    // Clear deployment tracking for new round
                    state.pending_deployments.write().await.clear();
                    *state.pending_round_id.write().await = current_round_id;
                    round_ending_detected = false;
                }
                
                last_round_id = current_round_id;
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
                // Incremental update - only fetches miners changed since last_slot
                let slot_delta = current_slot - last_slot;
                match fetch_miners_changed_since(&state, last_slot).await {
                    Ok(count) => {
                        // Only log if there were changes (reduces noise)
                        if count > 0 {
                            tracing::info!(
                                "Miner cache: +{} changed (slots {} → {}, Δ{})",
                                count, last_slot, current_slot, slot_delta
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
/// Public for use by finalization module
pub async fn fetch_all_miners(state: &AppState) -> anyhow::Result<usize> {
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

// ============================================================================
// EVORE Account Polling (Phase 1b)
// ============================================================================

/// Spawn the EVORE accounts polling task
/// Fetches Managers, Deployers, and Auth balances
/// - Initial: Full fetch of all accounts
/// - Subsequent: Incremental updates using changedSinceSlot (every 5s)
pub fn spawn_evore_polling(state: Arc<AppState>) -> tokio::task::JoinHandle<()> {
    use crate::evore_cache::{
        parse_manager, parse_deployer, managed_miner_auth_pda, CachedAuthBalance,
    };
    
    tokio::spawn(async move {
        // Initial delay to let other caches populate first
        tokio::time::sleep(Duration::from_secs(5)).await;
        
        let mut ticker = interval(Duration::from_secs(5));
        let mut initial_load_done = false;
        let mut last_sync_slot: u64 = 0;
        
        loop {
            ticker.tick().await;
            
            // Fetch EVORE accounts
            let helius = state.helius.clone();
            
            // Get current slot for tracking what we've synced
            let current_slot = *state.slot_cache.read().await;
            
            // Fetch Managers (full or incremental)
            let managers_result = {
                let mut api = helius.write().await;
                if initial_load_done && last_sync_slot > 0 {
                    api.get_evore_managers_changed_since(last_sync_slot, None).await
                } else {
                    api.get_all_evore_managers(None).await
                }
            };
            
            match managers_result {
                Ok(accounts) => {
                    let mut cache = state.evore_cache.write().await;
                    let mut count = 0;
                    
                    for acc in &accounts {
                        // Decode account data
                        if let Some(data_b64) = acc.account.data.first() {
                            if let Ok(data) = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                data_b64,
                            ) {
                                if let Some(manager) = parse_manager(&acc.pubkey, &data) {
                                    cache.upsert_manager(manager);
                                    count += 1;
                                }
                            }
                        }
                    }
                    
                    if !initial_load_done {
                        tracing::info!("EVORE: Loaded {} managers (full)", count);
                    } else if count > 0 {
                        tracing::debug!("EVORE: Updated {} managers (incremental)", count);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch EVORE managers: {}", e);
                }
            }
            
            // Fetch Deployers (full or incremental)
            let deployers_result = {
                let mut api = helius.write().await;
                if initial_load_done && last_sync_slot > 0 {
                    api.get_evore_deployers_changed_since(last_sync_slot, None).await
                } else {
                    api.get_all_evore_deployers(None).await
                }
            };
            
            match deployers_result {
                Ok(accounts) => {
                    let mut cache = state.evore_cache.write().await;
                    let mut count = 0;
                    
                    for acc in &accounts {
                        if let Some(data_b64) = acc.account.data.first() {
                            if let Ok(data) = base64::Engine::decode(
                                &base64::engine::general_purpose::STANDARD,
                                data_b64,
                            ) {
                                if let Some(deployer) = parse_deployer(&acc.pubkey, &data) {
                                    cache.upsert_deployer(deployer);
                                    count += 1;
                                }
                            }
                        }
                    }
                    
                    if !initial_load_done {
                        tracing::info!("EVORE: Loaded {} deployers (full)", count);
                    } else if count > 0 {
                        tracing::debug!("EVORE: Updated {} deployers (incremental)", count);
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch EVORE deployers: {}", e);
                }
            }
            
            // Update last sync slot after managers/deployers are loaded
            if current_slot > 0 {
                last_sync_slot = current_slot;
            }
            
            if !initial_load_done {
                initial_load_done = true;
                tracing::info!(
                    "EVORE: Initial load complete (will use changedSinceSlot for future updates)"
                );
            }
            
            // Fetch Auth balances for all managers
            {
                let cache = state.evore_cache.read().await;
                let managers: Vec<(String, Pubkey)> = cache.managers
                    .iter()
                    .filter_map(|(addr, _)| {
                        Pubkey::try_from(addr.as_str()).ok().map(|p| (addr.clone(), p))
                    })
                    .collect();
                drop(cache);
                
                if !managers.is_empty() {
                    // Derive PDAs and fetch balances in batches
                    let mut pdas: Vec<(String, Pubkey)> = Vec::new();
                    for (manager_addr, manager_pubkey) in &managers {
                        let (pda, _) = managed_miner_auth_pda(manager_pubkey, 0);
                        pdas.push((manager_addr.clone(), pda));
                    }
                    
                    // Fetch balances in batches of 100
                    let mut all_balances: Vec<(String, Pubkey, Option<u64>)> = Vec::new();
                    
                    for chunk in pdas.chunks(100) {
                        let addresses: Vec<Pubkey> = chunk.iter().map(|(_, p)| *p).collect();
                        
                        let balances_result = {
                            let mut api = helius.write().await;
                            api.get_multiple_account_balances(&addresses).await
                        };
                        
                        match balances_result {
                            Ok(balances) => {
                                for (i, (pubkey, balance)) in balances.into_iter().enumerate() {
                                    let manager_addr = chunk[i].0.clone();
                                    all_balances.push((manager_addr, pubkey, balance));
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Failed to fetch auth balances: {}", e);
                            }
                        }
                    }
                    
                    // Update cache
                    let mut cache = state.evore_cache.write().await;
                    let mut count = 0;
                    
                    for (manager_addr, pda, balance) in all_balances {
                        if let Some(lamports) = balance {
                            cache.upsert_auth_balance(CachedAuthBalance {
                                address: pda.to_string(),
                                manager: manager_addr,
                                auth_id: 0,
                                balance: lamports,
                            });
                            count += 1;
                        }
                    }
                    
                    // Update last slot
                    cache.last_updated_slot = *state.slot_cache.read().await;
                    
                    if !initial_load_done {
                        tracing::info!("EVORE: Loaded {} auth balances", count);
                    }
                }
            }
            
            if !initial_load_done {
                let cache = state.evore_cache.read().await;
                let stats = cache.stats();
                tracing::info!(
                    "EVORE cache initialized: {} managers, {} deployers, {} auth balances",
                    stats.managers_count, stats.deployers_count, stats.auth_balances_count
                );
                initial_load_done = true;
            }
        }
    })
}

