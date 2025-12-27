//! Round finalization logic
//!
//! Captures round snapshots when rounds end and finalizes them
//! after the round resets (when slot_hash and top_miner become available).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use evore::ore_api::{Miner, Round, Treasury};
use steel::Pubkey;
use tracing;

use crate::app_state::{AppState, LiveBroadcastData, RoundSnapshot};
use crate::clickhouse::{
    ClickHouseClient, DeploymentInsert, MinerSnapshot, RoundInsert, TreasurySnapshot,
};
use crate::tasks::fetch_all_miners;

/// Capture a snapshot of the current round state
/// Called ~5 seconds after round ends to ensure all miner data is settled
/// Does a FULL miner cache refresh (websocket data is unreliable)
pub async fn capture_round_snapshot(state: &AppState) -> Option<RoundSnapshot> {
    // Get current round info
    let round_cache = state.round_cache.read().await;
    let live_round = round_cache.as_ref()?;
    let round_id = live_round.round_id;
    let start_slot = live_round.start_slot;
    let end_slot = live_round.end_slot;
    drop(round_cache);
    
    tracing::info!("Waiting 5 seconds before capturing miner snapshot for round {}...", round_id);
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // Do a FULL miner cache refresh - websocket data is unreliable
    tracing::info!("Refreshing full miner cache before snapshot...");
    match fetch_all_miners(state).await {
        Ok(count) => {
            tracing::info!("Refreshed miner cache: {} miners", count);
        }
        Err(e) => {
            tracing::error!("Failed to refresh miner cache: {}", e);
            // Continue anyway with existing cache data
        }
    }
    
    // Get pending deployments (per-miner, per-square)
    let deployments = state.pending_deployments.read().await.clone();
    
    // Get miners who participated in this round (from freshly refreshed cache)
    let all_miners = state.miners_cache.read().await;
    let round_miners: HashMap<String, Miner> = all_miners
        .iter()
        .filter(|(_, m)| m.round_id == round_id)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    drop(all_miners);
    
    // Get treasury state
    let treasury = state.treasury_cache.read().await.clone()?;
    
    // Get round state (may not have slot_hash yet)
    let round = state.rpc.get_round(round_id).await.ok()?;
    
    let snapshot = RoundSnapshot {
        round_id,
        start_slot,
        end_slot,
        deployments,
        miners: round_miners,
        treasury,
        round,
        captured_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    
    tracing::info!(
        "Captured snapshot for round {}: {} miners, {} deployment entries",
        round_id,
        snapshot.miners.len(),
        snapshot.deployments.len()
    );
    
    Some(snapshot)
}

/// Finalize a round after reset
/// Called after detecting board.round_id has incremented
/// Waits until both slot_hash AND top_miner are populated
pub async fn finalize_round(
    state: &AppState,
    snapshot: RoundSnapshot,
) -> anyhow::Result<()> {
    let round_id = snapshot.round_id;
    
    tracing::info!("Finalizing round {}...", round_id);
    
    // Poll until round has both slot_hash AND top_miner populated
    let finalized_round = wait_for_round_finalization(state, round_id).await?;
    
    // Verify we have the slot_hash
    let rng = finalized_round.rng().ok_or_else(|| {
        anyhow::anyhow!("Round {} still has no slot_hash after waiting", round_id)
    })?;
    
    let winning_square = finalized_round.winning_square(rng) as u8;
    let total_winnings = finalized_round.total_winnings;
    let is_split_reward = finalized_round.is_split_reward(rng);
    
    // Use top_miner from the on-chain Round account (authoritative)
    let top_miner_pubkey = finalized_round.top_miner.to_string();
    
    tracing::info!(
        "Round {} winning_square={}, top_miner={}, total_winnings={}, is_split={}",
        round_id, winning_square, top_miner_pubkey, total_winnings, is_split_reward
    );
    
    // Build deployments for ClickHouse
    let mut all_deployments = Vec::new();
    
    for (miner_pubkey, squares) in &snapshot.deployments {
        // Check if this miner is the top_miner (from on-chain data)
        let is_this_top_miner = *miner_pubkey == top_miner_pubkey;
        
        for (&square_id, &(amount, slot)) in squares {
            let is_winner = square_id == winning_square;
            
            // Calculate rewards for winning square
            let (sol_earned, ore_earned) = if is_winner && total_winnings > 0 {
                calculate_rewards(
                    amount,
                    &finalized_round,
                    winning_square,
                    total_winnings,
                    is_split_reward,
                )
            } else {
                (0, 0)
            };
            
            all_deployments.push(DeploymentInsert {
                round_id,
                miner_pubkey: miner_pubkey.clone(),
                square_id,
                amount,
                deployed_slot: slot,
                sol_earned,
                ore_earned,
                is_winner: if is_winner { 1 } else { 0 },
                is_top_miner: if is_winner && is_this_top_miner { 1 } else { 0 },
            });
        }
    }
    
    tracing::info!(
        "Round {}: {} deployments to store, top_miner={}",
        round_id,
        all_deployments.len(),
        top_miner_pubkey
    );
    
    // Build round insert
    let round_insert = RoundInsert {
        round_id,
        expires_at: snapshot.end_slot, // Using end_slot as expiry
        start_slot: snapshot.start_slot,
        end_slot: snapshot.end_slot,
        slot_hash: finalized_round.slot_hash,
        winning_square,
        rent_payer: Pubkey::default().to_string(), // Not tracked
        top_miner: top_miner_pubkey.clone(),
        top_miner_reward: finalized_round.top_miner_reward,
        total_deployed: finalized_round.total_deployed,
        total_vaulted: finalized_round.total_vaulted,
        total_winnings,
        motherlode: finalized_round.motherlode,
        motherlode_hit: if finalized_round.motherlode > 0 { 1 } else { 0 },
        total_deployments: all_deployments.len() as u32,
        unique_miners: snapshot.deployments.len() as u32,
        source: "live".to_string(),
    };
    
    // Build treasury snapshot
    let treasury_snapshot = TreasurySnapshot {
        balance: snapshot.treasury.balance,
        motherlode: snapshot.treasury.motherlode,
        total_staked: snapshot.treasury.total_staked,
        total_unclaimed: snapshot.treasury.total_unclaimed,
        total_refined: snapshot.treasury.total_refined,
        round_id,
    };
    
    // Build miner snapshots
    let miner_snapshots: Vec<MinerSnapshot> = snapshot
        .miners
        .values()
        .filter(|m| m.round_id == round_id)
        .map(|m| MinerSnapshot {
            round_id,
            miner_pubkey: m.authority.to_string(),
            unclaimed_ore: m.rewards_ore,
            refined_ore: m.refined_ore,
            lifetime_sol: m.lifetime_rewards_sol,
            lifetime_ore: m.lifetime_rewards_ore,
        })
        .collect();
    
    // Store to ClickHouse
    state.clickhouse.insert_round(round_insert).await?;
    tracing::debug!("Stored round {} to ClickHouse", round_id);
    
    state.clickhouse.insert_deployments(all_deployments.clone()).await?;
    tracing::debug!("Stored {} deployments to ClickHouse", all_deployments.len());
    
    state.clickhouse.insert_treasury_snapshot(treasury_snapshot).await?;
    tracing::debug!("Stored treasury snapshot for round {}", round_id);
    
    state.clickhouse.insert_miner_snapshots(miner_snapshots.clone()).await?;
    tracing::debug!("Stored {} miner snapshots for round {}", miner_snapshots.len(), round_id);
    
    // Broadcast winning square announcement
    let _ = state.round_broadcast.send(LiveBroadcastData::WinningSquare {
        round_id,
        winning_square,
        motherlode_hit: finalized_round.motherlode > 0,
    });
    
    tracing::info!(
        "Round {} finalized: winning_square={}, {} deployments, {} miner snapshots",
        round_id,
        winning_square,
        all_deployments.len(),
        miner_snapshots.len()
    );
    
    Ok(())
}

/// Calculate rewards for a deployment on the winning square
fn calculate_rewards(
    amount: u64,
    round: &Round,
    winning_square: u8,
    total_winnings: u64,
    _is_split: bool,
) -> (u64, u64) {
    let square_total = round.deployed[winning_square as usize];
    
    if square_total == 0 {
        return (0, 0);
    }
    
    // SOL share proportional to deployment amount
    let sol_share = (amount as u128 * total_winnings as u128 / square_total as u128) as u64;
    
    // ORE rewards - simplified for now
    // TODO: Implement proper ORE calculation based on treasury mechanics
    let ore_share = 0u64;
    
    (sol_share, ore_share)
}

/// Wait for round to have both slot_hash and top_miner populated
/// Polls every 2 seconds, times out after 60 seconds
async fn wait_for_round_finalization(
    state: &AppState,
    round_id: u64,
) -> anyhow::Result<Round> {
    let max_attempts = 30; // 60 seconds total
    let poll_interval = Duration::from_secs(2);
    
    for attempt in 1..=max_attempts {
        match state.rpc.get_round(round_id).await {
            Ok(round) => {
                // Check if slot_hash is populated (not all zeros)
                let has_slot_hash = round.slot_hash != [0u8; 32];
                
                // Check if top_miner is populated (not default pubkey)
                let has_top_miner = round.top_miner != Pubkey::default();
                
                if has_slot_hash && has_top_miner {
                    tracing::info!(
                        "Round {} ready for finalization (attempt {}): top_miner={}",
                        round_id, attempt, round.top_miner
                    );
                    return Ok(round);
                }
                
                tracing::debug!(
                    "Round {} not ready (attempt {}): slot_hash={}, top_miner={}",
                    round_id, attempt, has_slot_hash, has_top_miner
                );
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to fetch round {} (attempt {}): {}",
                    round_id, attempt, e
                );
            }
        }
        
        tokio::time::sleep(poll_interval).await;
    }
    
    Err(anyhow::anyhow!(
        "Timeout waiting for round {} to have slot_hash and top_miner after {} seconds",
        round_id, max_attempts * 2
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_calculate_rewards() {
        // Test proportional distribution
        // If miner deployed 1000 out of 10000 total, they get 10% of winnings
        // Mocking round for test would require more setup
    }
}

