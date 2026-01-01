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

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app_state::AppState;

/// Spawn background task to backfill round_addresses and round_transaction_stats tables.
/// 
/// This task will:
/// 1. First, fill in all missing round_addresses from 1 to current live round
/// 2. Then, backfill round_transaction_stats for rounds that have v2 transactions
/// 3. Stop when both are complete
pub fn spawn_round_addresses_backfill(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("Starting round addresses/stats backfill task...");
        
        // Wait for startup to complete
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        let mut addresses_complete = false;
        let mut stats_complete = false;
        
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
            
            // Phase 2: Backfill round_transaction_stats (only after addresses are done)
            if addresses_complete && !stats_complete {
                match backfill_round_transaction_stats(&state).await {
                    Ok(BackfillResult::Complete) => {
                        tracing::info!("Round transaction stats backfill complete!");
                        stats_complete = true;
                    }
                    Ok(BackfillResult::Progress(count)) => {
                        tracing::debug!("Backfilled stats for {} rounds", count);
                    }
                    Err(e) => {
                        tracing::error!("Round transaction stats backfill error: {}", e);
                        tokio::time::sleep(Duration::from_secs(30)).await;
                        continue;
                    }
                }
            }
            
            // Both complete - exit the loop
            if addresses_complete && stats_complete {
                tracing::info!("All round backfills complete!");
                break;
            }
            
            // Slow down to avoid overloading - wait between batches
            tokio::time::sleep(Duration::from_millis(500)).await;
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

async fn backfill_round_transaction_stats(state: &AppState) -> anyhow::Result<BackfillResult> {
    tracing::info!("Checking round transaction stats backfill...");
    
    // Check if already complete
    if state.clickhouse.all_round_stats_complete().await? {
        tracing::info!("all_round_stats_complete returned true");
        return Ok(BackfillResult::Complete);
    }
    
    // Get rounds that need stats (have addresses + v2 transactions but no stats)
    let rounds_needing_stats = state.clickhouse.get_rounds_needing_stats_backfill().await?;
    
    tracing::info!("Found {} rounds needing stats backfill", rounds_needing_stats.len());
    
    if rounds_needing_stats.is_empty() {
        tracing::info!("No rounds needing stats, marking complete");
        return Ok(BackfillResult::Complete);
    }
    
    // Process in smaller batches to avoid timeouts
    let batch_size = 50.min(rounds_needing_stats.len());
    let batch = &rounds_needing_stats[..batch_size];
    
    tracing::info!("Backfilling stats for rounds: {:?}", &batch[..batch.len().min(5)]);
    
    let count = state.clickhouse.backfill_round_transaction_stats(batch).await?;
    
    tracing::info!("Backfilled stats for {} rounds", count);
    
    Ok(BackfillResult::Progress(count))
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

