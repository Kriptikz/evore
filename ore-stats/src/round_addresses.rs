//! Round addresses and transaction stats backfill task
//!
//! Background task to populate:
//! 1. round_addresses table - PDA mappings for each round
//! 2. round_transaction_stats table - pre-computed transaction counts per round
//!
//! This enables fast transaction lookups by round_id.
//!
//! Features:
//! - Restart-safe: checks what's already populated
//! - Throttled: doesn't overload CPU, inserts slowly
//! - Self-terminating: stops when all data is backfilled
//! - Incremental: can pick up new transactions added later

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app_state::AppState;

/// Spawn background task to backfill round_addresses and round_transaction_stats tables.
/// 
/// This task will:
/// 1. First, fill in all missing round_addresses from 1 to current live round
/// 2. Then, backfill round_transaction_stats sequentially from round 1
/// 3. Stop when we reach the current live round
pub fn spawn_round_addresses_backfill(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("Starting round addresses/stats backfill task...");
        
        // Wait for startup to complete
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        let mut addresses_complete = false;
        let mut stats_current_round: u64 = 1;
        
        loop {
            // Phase 1: Backfill round_addresses
            if !addresses_complete {
                match backfill_round_addresses(&state).await {
                    Ok(BackfillResult::Complete) => {
                        tracing::info!("Round addresses backfill complete!");
                        addresses_complete = true;
                    }
                    Ok(BackfillResult::Progress(count)) => {
                        tracing::debug!("Backfilled {} round addresses", count);
                    }
                    Err(e) => {
                        tracing::error!("Round addresses backfill error: {}", e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
            }
            
            // Phase 2: Backfill round_transaction_stats sequentially
            if addresses_complete {
                match backfill_round_stats_sequential(&state, &mut stats_current_round).await {
                    Ok(BackfillResult::Complete) => {
                        tracing::info!("Round transaction stats backfill complete at round {}!", stats_current_round);
                        // Don't break - keep running to pick up new rounds
                        // Sleep longer since we're caught up
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        continue;
                    }
                    Ok(BackfillResult::Progress(count)) => {
                        if count > 0 {
                            tracing::debug!("Round {} stats: {} transactions", stats_current_round - 1, count);
                        }
                    }
                    Err(e) => {
                        tracing::error!("Round {} stats backfill error: {}", stats_current_round, e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
            }
            
            // Small delay between rounds
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
}

enum BackfillResult {
    Complete,
    Progress(usize),
}

async fn backfill_round_addresses(state: &AppState) -> anyhow::Result<BackfillResult> {
    // Get the current live round from cache
    let live_round_id = {
        let board = state.board_cache.read().await;
        if let Some(board) = board.as_ref() {
            board.round_id
        } else {
            // If no board cache, try to fetch it
            match state.rpc.get_board().await {
                Ok(board) => board.round_id,
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to get board for round ID: {}", e));
                }
            }
        }
    };
    
    // Check if already complete
    if state.clickhouse.all_round_addresses_complete(live_round_id).await? {
        return Ok(BackfillResult::Complete);
    }
    
    // Get missing round IDs (limited to 1000 per batch)
    let missing = state.clickhouse.get_missing_round_address_ids(live_round_id).await?;
    
    if missing.is_empty() {
        return Ok(BackfillResult::Complete);
    }
    
    // Calculate PDAs for missing rounds (batch of up to 100 at a time to be gentle)
    let batch_size = 100.min(missing.len());
    let batch: Vec<(u64, String)> = missing[..batch_size]
        .iter()
        .map(|&round_id| {
            let (pda, _) = evore::ore_api::round_pda(round_id);
            (round_id, pda.to_string())
        })
        .collect();
    
    let count = batch.len();
    
    // Insert batch
    state.clickhouse.insert_round_addresses(batch).await?;
    
    Ok(BackfillResult::Progress(count))
}

/// Backfill stats for a single round, then increment the round counter.
/// 
/// For each round:
/// 1. Check if round exists in round_addresses (if not, we're caught up)
/// 2. Get current stats for the round (if any)
/// 3. Query v2 transactions for that round
/// 4. Update stats based on what we find
async fn backfill_round_stats_sequential(
    state: &AppState,
    current_round: &mut u64,
) -> anyhow::Result<BackfillResult> {
    let round_id = *current_round;
    
    // Check if this round exists in round_addresses
    let round_address = match state.clickhouse.get_round_address(round_id).await? {
        Some(addr) => addr,
        None => {
            // Round doesn't exist in addresses yet - we're caught up
            return Ok(BackfillResult::Complete);
        }
    };
    
    // Get current stats for this round (if any)
    let current_stats = state.clickhouse.get_round_stats(round_id).await?;
    
    // Determine what to query based on current stats
    let (txn_count, min_slot, max_slot) = if let Some(stats) = &current_stats {
        if stats.transaction_count > 0 {
            // Already have stats - check for new transactions after max_slot
            let new_txns = state.clickhouse
                .get_v2_txn_stats_after_slot(&round_address, stats.max_slot)
                .await?;
            
            if new_txns.count == 0 {
                // No new transactions - move to next round
                *current_round += 1;
                return Ok(BackfillResult::Progress(0));
            }
            
            // Have new transactions - add to existing stats
            (
                stats.transaction_count + new_txns.count,
                stats.min_slot,
                new_txns.max_slot,
            )
        } else {
            // Stats exist but count is 0 - get all transactions
            let txn_stats = state.clickhouse
                .get_v2_txn_stats_for_round(&round_address)
                .await?;
            (txn_stats.count, txn_stats.min_slot, txn_stats.max_slot)
        }
    } else {
        // No stats yet - get all transactions
        let txn_stats = state.clickhouse
            .get_v2_txn_stats_for_round(&round_address)
            .await?;
        (txn_stats.count, txn_stats.min_slot, txn_stats.max_slot)
    };
    
    // Insert/update stats
    state.clickhouse
        .upsert_round_transaction_stats(round_id, &round_address, txn_count, min_slot, max_slot)
        .await?;
    
    // Move to next round
    *current_round += 1;
    
    Ok(BackfillResult::Progress(txn_count as usize))
}

/// Insert a single round address (called during finalization).
pub async fn insert_round_address(state: &AppState, round_id: u64) -> anyhow::Result<()> {
    // Check if already exists
    if state.clickhouse.round_address_exists(round_id).await? {
        return Ok(());
    }
    
    let (pda, _) = evore::ore_api::round_pda(round_id);
    state.clickhouse.insert_round_address(round_id, &pda.to_string()).await?;
    
    tracing::debug!("Inserted round address for round {}: {}", round_id, pda);
    Ok(())
}
