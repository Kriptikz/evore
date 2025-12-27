//! Round finalization logic
//!
//! Captures round snapshots when rounds end and finalizes them
//! after the round resets (when slot_hash and top_miner become available).
//!
//! Uses a multi-source approach for data accuracy:
//! 1. GPA miners snapshot (getProgramAccounts) - source of truth for miner counts & amounts
//! 2. Miners cache (v2 endpoint) - provides slot timing data from polling
//! 3. WebSocket pending_deployments - provides real-time slot data
//!
//! We combine all three to get the most accurate deployment data with slots.

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
/// 
/// Process:
/// 1. IMMEDIATELY take a GPA miners snapshot (source of truth - this worked 100% in old system)
/// 2. Log WebSocket pending_deployments count for debugging
/// 3. Wait 5 seconds for v2 endpoint to settle
/// 4. Refresh miners_cache (v2 endpoint) for slot timing data
/// 5. Compare GPA vs miners_cache and log differences
/// 6. Use GPA as source of truth, merge slot data from cache/websocket
pub async fn capture_round_snapshot(state: &AppState) -> Option<RoundSnapshot> {
    // Get current round info
    let round_cache = state.round_cache.read().await;
    let live_round = round_cache.as_ref()?;
    let round_id = live_round.round_id;
    let start_slot = live_round.start_slot;
    let end_slot = live_round.end_slot;
    drop(round_cache);
    
    // === STEP 1: IMMEDIATELY take GPA miners snapshot (source of truth) ===
    tracing::info!("Round {} ending - taking GPA miners snapshot IMMEDIATELY...", round_id);
    
    let gpa_miners = match state.rpc.get_all_miners_gpa().await {
        Ok(miners) => {
            let total_count = miners.len();
            let round_miners: HashMap<String, Miner> = miners
                .into_iter()
                .filter(|(_, m)| m.round_id == round_id)
                .collect();
            tracing::info!(
                "GPA snapshot: {} total miners, {} with round_id={}",
                total_count,
                round_miners.len(),
                round_id
            );
            round_miners
        }
        Err(e) => {
            tracing::error!("Failed to get GPA miners snapshot: {}. Will fall back to cache.", e);
            HashMap::new()
        }
    };
    
    // === STEP 2: Log WebSocket pending_deployments count ===
    let ws_deployments = state.pending_deployments.read().await.clone();
    let ws_unique_miners = ws_deployments.len();
    let ws_total_squares: usize = ws_deployments.values().map(|s| s.len()).sum();
    
    tracing::info!(
        "WebSocket pending_deployments for round {}: {} unique miners, {} square entries",
        round_id, ws_unique_miners, ws_total_squares
    );
    
    // === STEP 3: Wait 5 seconds for v2 endpoint to settle ===
    tracing::info!("Waiting 5 seconds before refreshing miners cache (v2 endpoint)...");
    tokio::time::sleep(Duration::from_secs(5)).await;
    
    // === STEP 4: Refresh miners_cache (v2 endpoint) for slot timing ===
    tracing::info!("Refreshing miners cache (v2 endpoint) for slot timing data...");
    match fetch_all_miners(state).await {
        Ok(count) => {
            tracing::info!("V2 miners cache refreshed: {} miners", count);
        }
        Err(e) => {
            tracing::error!("Failed to refresh miners cache: {}", e);
        }
    }
    
    // Get miners from v2 cache (for slot timing from deployments_cache)
    let all_miners = state.miners_cache.read().await;
    let cache_round_miners: HashMap<String, Miner> = all_miners
        .iter()
        .filter(|(_, m)| m.round_id == round_id)
        .map(|(k, v)| (k.clone(), *v))
        .collect();
    drop(all_miners);
    
    // Get deployments cache (has slot timing from polling)
    let deployments_cache = state.deployments_cache.read().await.clone();
    
    // === STEP 5: Compare GPA vs miners_cache and log differences ===
    let gpa_count = gpa_miners.len();
    let cache_count = cache_round_miners.len();
    
    if gpa_count != cache_count {
        tracing::warn!(
            "DATA MISMATCH - Round {}: GPA has {} miners, v2 cache has {} miners (diff={})",
            round_id, gpa_count, cache_count, 
            (gpa_count as i64 - cache_count as i64).abs()
        );
        
        // Find miners in GPA but not in cache
        let gpa_only: Vec<&String> = gpa_miners.keys()
            .filter(|k| !cache_round_miners.contains_key(*k))
            .collect();
        if !gpa_only.is_empty() {
            tracing::warn!(
                "  Miners in GPA but NOT in v2 cache ({}): {:?}",
                gpa_only.len(),
                gpa_only.iter().take(5).collect::<Vec<_>>()
            );
        }
        
        // Find miners in cache but not in GPA
        let cache_only: Vec<&String> = cache_round_miners.keys()
            .filter(|k| !gpa_miners.contains_key(*k))
            .collect();
        if !cache_only.is_empty() {
            tracing::warn!(
                "  Miners in v2 cache but NOT in GPA ({}): {:?}",
                cache_only.len(),
                cache_only.iter().take(5).collect::<Vec<_>>()
            );
        }
    } else {
        tracing::info!(
            "Round {}: GPA and v2 cache agree on {} miners",
            round_id, gpa_count
        );
    }
    
    // === STEP 6: Build combined snapshot ===
    // Use GPA as source of truth for miners/amounts, merge slot data from cache/websocket
    
    // Source of truth: GPA miners (if available), fallback to cache
    let source_miners = if !gpa_miners.is_empty() {
        tracing::info!("Using GPA miners as source of truth ({} miners)", gpa_miners.len());
        gpa_miners
    } else {
        tracing::warn!("GPA empty, falling back to v2 cache ({} miners)", cache_round_miners.len());
        cache_round_miners.clone()
    };
    
    // Build combined deployments: amounts from source_miners, slots from cache/websocket
    let mut combined_deployments: HashMap<String, HashMap<u8, (u64, u64)>> = HashMap::new();
    
    for (miner_pubkey, miner) in &source_miners {
        let mut miner_squares: HashMap<u8, (u64, u64)> = HashMap::new();
        
        for (square_id, &amount) in miner.deployed.iter().enumerate() {
            if amount == 0 {
                continue;
            }
            
            let square_id_u8 = square_id as u8;
            
            // Try to find slot from:
            // 1. WebSocket pending_deployments (most accurate if available)
            // 2. Deployments cache (from polling)
            // 3. Default to start_slot if no slot data found
            
            let slot = ws_deployments
                .get(miner_pubkey)
                .and_then(|squares| squares.get(&square_id_u8))
                .map(|(_, slot)| *slot)
                .or_else(|| {
                    deployments_cache
                        .get(miner_pubkey)
                        .and_then(|squares| squares.get(&square_id_u8))
                        .map(|(_, slot)| *slot)
                })
                .unwrap_or(start_slot); // Default to start_slot if no slot data
            
            miner_squares.insert(square_id_u8, (amount, slot));
        }
        
        if !miner_squares.is_empty() {
            combined_deployments.insert(miner_pubkey.clone(), miner_squares);
        }
    }
    
    // Count deployments with known slots vs defaulted
    let mut known_slots = 0u32;
    let mut defaulted_slots = 0u32;
    for (_, squares) in &combined_deployments {
        for (_, (_, slot)) in squares {
            if *slot == start_slot {
                defaulted_slots += 1;
            } else {
                known_slots += 1;
            }
        }
    }
    
    tracing::info!(
        "Combined deployments: {} miners, {} squares with known slots, {} defaulted to start_slot",
        combined_deployments.len(), known_slots, defaulted_slots
    );
    
    // Get treasury state
    let treasury = state.treasury_cache.read().await.clone()?;
    
    // Get round state (may not have slot_hash yet)
    let round = state.rpc.get_round(round_id).await.ok()?;
    
    let snapshot = RoundSnapshot {
        round_id,
        start_slot,
        end_slot,
        deployments: combined_deployments,
        miners: source_miners,
        treasury,
        round,
        captured_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    };
    
    tracing::info!(
        "Captured snapshot for round {}: {} miners (GPA source), {} deployment entries",
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
    let finalized_round = wait_for_round_finalization(state, round_id, &snapshot).await?;
    
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
    // Use miner snapshot as authoritative source for deployed amounts
    // Match with websocket slot data where available, fallback to slot 0
    let mut all_deployments = Vec::new();
    let mut miners_with_deployments = 0u32;
    
    for (miner_pubkey, miner) in &snapshot.miners {
        // Only include miners who deployed this round
        if miner.round_id != round_id {
            continue;
        }
        
        // Check if this miner is the top_miner (from on-chain data)
        let is_this_top_miner = *miner_pubkey == top_miner_pubkey;
        
        // Get websocket slot data for this miner (if available)
        let ws_squares = snapshot.deployments.get(miner_pubkey);
        
        let mut miner_has_deployment = false;
        
        // Use miner.deployed[25] as authoritative amounts
        for (square_id, &amount) in miner.deployed.iter().enumerate() {
            if amount == 0 {
                continue;
            }
            
            miner_has_deployment = true;
            let square_id_u8 = square_id as u8;
            
            // Try to get slot from websocket data, fallback to 0 if not found
            let deployed_slot = ws_squares
                .and_then(|squares| squares.get(&square_id_u8))
                .map(|(_, slot)| *slot)
                .unwrap_or(0);
            
            let is_winner = square_id_u8 == winning_square;
            
            // Calculate rewards for winning square
            let (sol_earned, ore_earned) = if is_winner {
                calculate_rewards(
                    amount,
                    &finalized_round,
                    winning_square,
                    total_winnings,
                    is_split_reward,
                    is_this_top_miner,
                )
            } else {
                (0, 0)
            };
            
            all_deployments.push(DeploymentInsert {
                round_id,
                miner_pubkey: miner_pubkey.clone(),
                square_id: square_id_u8,
                amount,
                deployed_slot,
                sol_earned,
                ore_earned,
                is_winner: if is_winner { 1 } else { 0 },
                is_top_miner: if is_winner && is_this_top_miner { 1 } else { 0 },
            });
        }
        
        if miner_has_deployment {
            miners_with_deployments += 1;
        }
    }
    
    tracing::info!(
        "Round {}: {} deployments from {} miners to store, top_miner={}",
        round_id,
        all_deployments.len(),
        miners_with_deployments,
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
        unique_miners: miners_with_deployments,
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
/// 
/// SOL rewards: Pro-rata share of total_winnings based on deployed amount
/// 
/// ORE rewards:
/// - If is_split: All winners share top_miner_reward proportionally
/// - If NOT split: Only top_miner gets the full top_miner_reward (1 ORE)
/// - Motherlode (if > 0): Always split proportionally among all winners
fn calculate_rewards(
    amount: u64,
    round: &Round,
    winning_square: u8,
    total_winnings: u64,
    is_split: bool,
    is_this_top_miner: bool,
) -> (u64, u64) {
    let square_total = round.deployed[winning_square as usize];
    
    if square_total == 0 {
        return (0, 0);
    }
    
    // SOL share: pro-rata based on deployment amount on winning square
    let sol_share = if total_winnings > 0 {
        (amount as u128 * total_winnings as u128 / square_total as u128) as u64
    } else {
        0
    };
    
    // ORE rewards calculation
    let mut ore_share: u64 = 0;
    
    // 1. Top miner reward (1 ORE = top_miner_reward)
    if is_split {
        // Split reward: all winners share proportionally
        ore_share += (amount as u128 * round.top_miner_reward as u128 / square_total as u128) as u64;
    } else if is_this_top_miner {
        // Not split: only top_miner gets the full 1 ORE
        ore_share += round.top_miner_reward;
    }
    // If not split and not top_miner, they get 0 from top_miner_reward
    
    // 2. Motherlode (if hit): always split proportionally among all winners
    if round.motherlode > 0 {
        ore_share += (amount as u128 * round.motherlode as u128 / square_total as u128) as u64;
    }
    
    (sol_share, ore_share)
}

/// Wait for round to have both slot_hash and top_miner populated
/// Polls every 2 seconds, times out after 60 seconds
/// Also logs optimistic top_miner calculation for verification
async fn wait_for_round_finalization(
    state: &AppState,
    round_id: u64,
    snapshot: &RoundSnapshot,
) -> anyhow::Result<Round> {
    let max_attempts = 30; // 60 seconds total
    let poll_interval = Duration::from_secs(2);
    let mut logged_optimistic = false;
    
    for attempt in 1..=max_attempts {
        match state.rpc.get_round(round_id).await {
            Ok(round) => {
                // Check if slot_hash is populated (not all zeros)
                let has_slot_hash = round.slot_hash != [0u8; 32];
                
                // Check if top_miner is populated (not default pubkey)
                let has_top_miner = round.top_miner != Pubkey::default();
                
                // Log optimistic calculation as soon as slot_hash is available
                if has_slot_hash && !logged_optimistic {
                    logged_optimistic = true;
                    log_optimistic_calculation(&round, snapshot);
                }
                
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

/// Log the optimistic winning square and top miner calculation
/// This runs as soon as slot_hash is available (before on-chain top_miner is set)
/// Used to verify our calculation matches the on-chain result
///
/// Top miner is NOT the highest deployer - it's a WEIGHTED RANDOM selection:
/// - top_miner_sample = rng.reverse_bits() % total_deployed[winning_square]
/// - The miner whose cumulative range contains this sample wins
/// - If is_split_reward is true, the 1 ORE is split among all winners proportionally
fn log_optimistic_calculation(round: &Round, snapshot: &RoundSnapshot) {
    // Calculate winning square from slot_hash
    let rng = match round.rng() {
        Some(r) => r,
        None => {
            tracing::warn!("OPTIMISTIC: Cannot calculate - no rng available");
            return;
        }
    };
    
    let winning_square = round.winning_square(rng);
    let is_split = round.is_split_reward(rng);
    let total_on_winning = round.deployed[winning_square];
    
    tracing::info!(
        "OPTIMISTIC CALC - Round {}: winning_square={}, total_on_square={}, is_split_reward={}",
        round.id, winning_square, total_on_winning, is_split
    );
    
    if total_on_winning == 0 {
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: no deployments on winning square, no top_miner",
            round.id
        );
        return;
    }
    
    // If split reward, the 1 ORE is split among all winners - no single top_miner selection
    if is_split {
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: SPLIT REWARD - 1 ORE split among all winners on square {}",
            round.id, winning_square
        );
        // Log participants
        let miners_on_square: Vec<(&String, u64)> = snapshot.miners
            .iter()
            .filter(|(_, m)| m.deployed[winning_square] > 0)
            .map(|(pubkey, m)| (pubkey, m.deployed[winning_square]))
            .collect();
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: {} miners will split the reward",
            round.id, miners_on_square.len()
        );
        return;
    }
    
    // Weighted random selection using top_miner_sample
    // Sample point in [0, total_deployed[winning_square])
    let sample = round.top_miner_sample(rng, winning_square);
    
    tracing::info!(
        "OPTIMISTIC CALC - Round {}: sample={} (will select miner whose cumulative range contains this)",
        round.id, sample
    );
    
    // Find the miner whose cumulative range contains the sample
    // Range for each miner: [cumulative, cumulative + deployed)
    let mut predicted_top_miner: Option<(String, u64, u64, u64)> = None; // (pubkey, cumulative, deployed, upper_bound)
    
    for (pubkey, miner) in &snapshot.miners {
        let deployed = miner.deployed[winning_square];
        if deployed == 0 {
            continue;
        }
        
        let cumulative = miner.cumulative[winning_square];
        let upper_bound = cumulative + deployed;
        
        // Check if sample falls in this miner's range [cumulative, upper_bound)
        if sample >= cumulative && sample < upper_bound {
            predicted_top_miner = Some((pubkey.clone(), cumulative, deployed, upper_bound));
            break;
        }
    }
    
    if let Some((pubkey, cumulative, deployed, upper)) = predicted_top_miner {
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: PREDICTED top_miner={}",
            round.id, pubkey
        );
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: range=[{}, {}) contains sample={}, deployed={} lamports",
            round.id, cumulative, upper, sample, deployed
        );
    } else {
        tracing::warn!(
            "OPTIMISTIC CALC - Round {}: Could not find miner for sample={}! Data mismatch?",
            round.id, sample
        );
        
        // Debug: log all miners on the winning square
        let mut miners_on_square: Vec<(&String, u64, u64)> = snapshot.miners
            .iter()
            .filter(|(_, m)| m.deployed[winning_square] > 0)
            .map(|(pubkey, m)| (pubkey, m.cumulative[winning_square], m.deployed[winning_square]))
            .collect();
        miners_on_square.sort_by(|a, b| a.1.cmp(&b.1)); // Sort by cumulative
        
        tracing::info!(
            "OPTIMISTIC CALC - Round {}: {} miners on winning square (sorted by cumulative):",
            round.id, miners_on_square.len()
        );
        for (pubkey, cumulative, deployed) in miners_on_square.iter().take(10) {
            tracing::info!(
                "  {} - cumulative={}, deployed={}, range=[{}, {})",
                pubkey, cumulative, deployed, cumulative, cumulative + deployed
            );
        }
    }
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

