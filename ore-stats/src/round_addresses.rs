//! Round addresses backfill task
//!
//! Background task to populate the round_addresses table with PDA mappings.
//! This enables transaction lookups by round_id since raw_transactions_v2 uses accounts.
//!
//! Features:
//! - Restart-safe: checks what's already populated
//! - Throttled: doesn't overload CPU, inserts slowly
//! - Self-terminating: stops when all rounds are populated

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use crate::app_state::AppState;

/// Spawn background task to backfill round_addresses table.
/// 
/// This task will:
/// 1. Check if all rounds from 1 to current live round have addresses
/// 2. If not, slowly fill in missing round PDAs
/// 3. Stop when complete (won't restart on subsequent server starts if done)
pub fn spawn_round_addresses_backfill(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("Starting round addresses backfill task...");
        
        // Wait for startup to complete
        tokio::time::sleep(Duration::from_secs(10)).await;
        
        loop {
            match backfill_round_addresses(&state).await {
                Ok(BackfillResult::Complete) => {
                    tracing::info!("Round addresses backfill complete!");
                    break;
                }
                Ok(BackfillResult::Progress(count)) => {
                    tracing::debug!("Backfilled {} round addresses", count);
                }
                Err(e) => {
                    tracing::error!("Round addresses backfill error: {}", e);
                    // Wait before retrying on error
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
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

