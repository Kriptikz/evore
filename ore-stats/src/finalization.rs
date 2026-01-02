//! Round finalization logic
//!
//! Captures round snapshots when rounds end and finalizes them
//! after the round resets (when slot_hash and top_miner become available).
//!
//! Uses a multi-source approach for data accuracy:
//! 1. GPA miners snapshot (getProgramAccounts) - source of truth for miner counts & amounts
//! 2. WebSocket pending_deployments - provides real-time slot data for deployments
//!
//! The GPA snapshot is taken 10 seconds after round ending is detected to allow
//! all transactions to settle before capturing the final state.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use evore::ore_api::{Miner, Round, Treasury};
use steel::Pubkey;
use tracing;

use crate::app_state::{AppState, LiveBroadcastData, RoundSnapshot};
use crate::clickhouse::{
    DeploymentInsert, MinerSnapshot, MintSnapshot, PartialRoundInsert, RawTransactionV2, 
    RoundInsert, SignatureRow, TreasurySnapshot,
};
use crate::txn_backfill::parse_transaction_accounts;

/// Capture a snapshot of the current round state
/// 
/// Process:
/// 1. Wait 10 seconds for all transactions to settle
/// 2. Take a GPA miners snapshot (source of truth)
/// 3. Update miners_cache with GPA data
/// 4. Log WebSocket pending_deployments count for debugging
/// 5. Use GPA as source of truth, merge slot data from WebSocket
pub async fn capture_round_snapshot(state: &AppState) -> Option<RoundSnapshot> {
    // Get current round info
    let round_cache = state.round_cache.read().await;
    let live_round = round_cache.as_ref()?;
    let round_id = live_round.round_id;
    let start_slot = live_round.start_slot;
    let end_slot = live_round.end_slot;
    drop(round_cache);
    
    // Store round address mapping (for transaction lookups by round)
    if let Err(e) = crate::round_addresses::insert_round_address(state, round_id).await {
        tracing::warn!("Failed to insert round address for round {}: {}", round_id, e);
    }
    
    // === STEP 1: Wait 10 seconds for transactions to settle ===
    tracing::info!("Round {} ending - waiting 10 seconds for transactions to settle...", round_id);
    tokio::time::sleep(Duration::from_secs(10)).await;
    
    // === STEP 2: Take GPA miners snapshot (source of truth) ===
    tracing::info!("Round {} - taking GPA miners snapshot...", round_id);
    
    // Get treasury for refined_ore calculation
    let treasury_cache = state.treasury_cache.read().await;
    let treasury_for_gpa = treasury_cache.clone();
    drop(treasury_cache);
    
    // Fetch all miners via GPA with built-in retry/fallback logic:
    // - 10 retries with 500ms delays
    // - Round-robin across providers (Flux first, then Helius, etc.)
    // - Validates at least 10,000 miners for a complete snapshot
    let gpa_result = match state.rpc.get_all_miners_gpa(treasury_for_gpa.as_ref()).await {
            Ok(miners) => {
            tracing::info!("GPA miners snapshot: {} miners fetched", miners.len());
            Some(miners)
            }
            Err(e) => {
            tracing::error!("GPA miners snapshot failed after all retries: {}", e);
            None
        }
    };
    
    let (all_gpa_miners, gpa_miners, gpa_failed) = match gpa_result {
        Some(miners) => {
            let total_count = miners.len();
            let all_miners = miners.clone();
            let round_miners: HashMap<String, Miner> = miners
                .into_iter()
                .filter(|(_, m)| m.round_id == round_id)
                .collect();
            tracing::info!(
                "GPA snapshot SUCCESS: {} total miners, {} with round_id={}",
                total_count,
                round_miners.len(),
                round_id
            );
            (all_miners, round_miners, false)
        }
        None => {
            tracing::error!(
                "CRITICAL: GPA snapshot failed for round {}! \
                 Will still store round/treasury snapshots. Deployments need backfill via admin.",
                round_id
            );
            // Continue with empty miners - we'll still store round and treasury
            (HashMap::new(), HashMap::new(), true)
        }
    };
    
    // === STEP 3: Update miners_cache with GPA data ===
    if !all_gpa_miners.is_empty() {
        let mut cache = state.miners_cache.write().await;
        *cache = all_gpa_miners.clone().into_iter().collect();
        tracing::info!("Updated miners_cache with {} miners from GPA", cache.len());
    }
    
    // === STEP 4: Log WebSocket pending_deployments count ===
    let ws_deployments = state.pending_deployments.read().await.clone();
    let ws_unique_miners = ws_deployments.len();
    let ws_total_squares: usize = ws_deployments.values().map(|s| s.len()).sum();
    
    tracing::info!(
        "WebSocket pending_deployments for round {}: {} unique miners, {} square entries",
        round_id, ws_unique_miners, ws_total_squares
    );
    
    // === STEP 5: Build combined snapshot ===
    // Use GPA as source of truth for miners/amounts, merge slot data from WebSocket
    
    // Source of truth: GPA miners
    let source_miners = if !gpa_miners.is_empty() {
        tracing::info!("Using GPA miners as source of truth ({} miners)", gpa_miners.len());
        gpa_miners
    } else {
        tracing::warn!("GPA empty - no miners to process");
        HashMap::new()
    };
    
    // Build combined deployments: amounts from GPA, slots from WebSocket
    let mut combined_deployments: HashMap<String, HashMap<u8, (u64, u64)>> = HashMap::new();
    
    for (miner_pubkey, miner) in &source_miners {
        let mut miner_squares: HashMap<u8, (u64, u64)> = HashMap::new();
        
        for (square_id, &amount) in miner.deployed.iter().enumerate() {
            if amount == 0 {
                continue;
            }
            
            let square_id_u8 = square_id as u8;
            
            // Try to find slot from WebSocket pending_deployments
            // Default to start_slot if no slot data found
            let slot = ws_deployments
                .get(miner_pubkey)
                .and_then(|squares| squares.get(&square_id_u8))
                .map(|(_, slot)| *slot)
                .unwrap_or(start_slot);
            
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
    
    // Get ORE mint supply
    let mint_supply = match state.rpc.get_mint_supply().await {
        Ok(supply) => {
            tracing::debug!("Fetched mint supply for round {}: {} atomic units", round_id, supply);
            Some(supply)
        }
        Err(e) => {
            tracing::warn!("Failed to fetch mint supply for round {}: {}", round_id, e);
            None
        }
    };
    
    let snapshot = RoundSnapshot {
        round_id,
        start_slot,
        end_slot,
        deployments: combined_deployments,
        miners: source_miners,
        all_miners: all_gpa_miners, // Store ALL miners for historical tracking
        treasury,
        round,
        mint_supply,
        captured_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        gpa_failed,
    };
    
    tracing::info!(
        "Captured snapshot for round {}: {} round miners, {} total miners (full GPA), {} deployment entries",
        round_id,
        snapshot.miners.len(),
        snapshot.all_miners.len(),
        snapshot.deployments.len()
    );
    
    Some(snapshot)
}

/// Finalize a round after reset
/// Called after detecting board.round_id has incremented
/// 
/// Key principle: Snapshots are stored BEFORE waiting for top_miner.
/// This ensures miner and treasury data is preserved even if finalization times out.
/// 
/// Process:
/// 1. ALWAYS store miner snapshots (from GPA data)
/// 2. ALWAYS store treasury snapshot
/// 3. ALWAYS store mint supply snapshot
/// 4. TRY to wait for top_miner and finalize:
///    - SUCCESS: Store round + deployments to `rounds` table
///    - TIMEOUT: Store to `partial_rounds` table for later backfill
pub async fn finalize_round(
    state: &AppState,
    snapshot: RoundSnapshot,
) -> anyhow::Result<()> {
    let round_id = snapshot.round_id;
    
    tracing::info!("Finalizing round {}...", round_id);
    
    // ========== STEP 1: Store miner snapshots FIRST (always) ==========
    // This ensures we have historical miner data even if finalization fails
    if !snapshot.gpa_failed {
        let miner_snapshots: Vec<MinerSnapshot> = snapshot
            .all_miners
            .values()
            .map(|m| MinerSnapshot {
                round_id,
                miner_pubkey: m.authority.to_string(),
                unclaimed_ore: m.rewards_ore,
                refined_ore: m.refined_ore,
                lifetime_sol: m.lifetime_rewards_sol,
                lifetime_ore: m.lifetime_rewards_ore,
            })
            .collect();
        
        state.clickhouse.insert_miner_snapshots(miner_snapshots.clone()).await?;
        tracing::info!("Stored {} miner snapshots for round {} (BEFORE finalization)", miner_snapshots.len(), round_id);
    } else {
        tracing::warn!("Skipping miner snapshots for round {} - GPA failed", round_id);
    }
    
    // ========== STEP 2: Store treasury snapshot FIRST (always) ==========
    let treasury_snapshot = TreasurySnapshot {
        balance: snapshot.treasury.balance,
        motherlode: snapshot.treasury.motherlode,
        total_staked: snapshot.treasury.total_staked,
        total_unclaimed: snapshot.treasury.total_unclaimed,
        total_refined: snapshot.treasury.total_refined,
        round_id,
    };
    state.clickhouse.insert_treasury_snapshot(treasury_snapshot).await?;
    tracing::info!("Stored treasury snapshot for round {} (BEFORE finalization)", round_id);
    
    // ========== STEP 3: Store mint supply snapshot (always) ==========
    if let Some(supply) = snapshot.mint_supply {
        let previous_supply = state.clickhouse.get_latest_mint_supply().await
            .ok()
            .flatten()
            .unwrap_or(0);
        
        let supply_change = if previous_supply > 0 {
            supply as i64 - previous_supply as i64
        } else {
            0
        };
        
        let mint_snapshot = MintSnapshot {
            round_id,
            supply,
            decimals: 11,
            supply_change,
        };
        
        state.clickhouse.insert_mint_snapshot(mint_snapshot).await?;
        tracing::info!("Stored mint snapshot for round {} (BEFORE finalization)", round_id);
    }
    
    // ========== STEP 4: Try to wait for top_miner and finalize ==========
    match wait_for_round_finalization(state, round_id, &snapshot).await {
        Ok(finalized_round) => {
            // SUCCESS: Full finalization with top_miner
    let rng = finalized_round.rng().ok_or_else(|| {
        anyhow::anyhow!("Round {} still has no slot_hash after waiting", round_id)
    })?;
            
            store_finalized_round(state, &snapshot, &finalized_round, rng).await?;
            tracing::info!("Round {} finalized successfully", round_id);
        }
        Err(e) => {
            // TIMEOUT: Store partial round for later backfill
            tracing::warn!(
                "Round {} finalization timeout: {}. Storing to partial_rounds.",
                round_id, e
            );
            store_partial_round(state, &snapshot, &e.to_string()).await?;
            tracing::info!("Round {} stored as partial round (needs backfill)", round_id);
        }
    }
    
    Ok(())
}

/// Store a fully finalized round (has top_miner)
async fn store_finalized_round(
    state: &AppState,
    snapshot: &RoundSnapshot,
    finalized_round: &Round,
    rng: u64,
) -> anyhow::Result<()> {
    let round_id = snapshot.round_id;
    
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
        expires_at: snapshot.end_slot,
        start_slot: snapshot.start_slot,
        end_slot: snapshot.end_slot,
        slot_hash: finalized_round.slot_hash,
        winning_square,
        rent_payer: Pubkey::default().to_string(),
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
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
    };
    
    // Store round to ClickHouse
    state.clickhouse.insert_round(round_insert).await?;
    tracing::debug!("Stored round {} to ClickHouse", round_id);
    
    // Store deployments (only if GPA succeeded)
    if !snapshot.gpa_failed {
        state.clickhouse.insert_deployments(all_deployments.clone()).await?;
        tracing::info!("Stored {} deployments for round {}", all_deployments.len(), round_id);
    } else {
        tracing::warn!(
            "Round {} GPA snapshot failed - skipping deployments. Use admin backfill.",
            round_id
        );
    }
    
    // Broadcast winning square announcement
    let _ = state.round_broadcast.send(LiveBroadcastData::WinningSquare {
        round_id,
        winning_square,
        motherlode_hit: finalized_round.motherlode > 0,
    });
    
    Ok(())
}

/// Fetch and store transactions for a finalized round
/// Runs in background after round finalization
/// Takes Arc<AppState> to be called from spawned tasks
pub async fn fetch_and_store_round_transactions(state: &std::sync::Arc<crate::app_state::AppState>, round_id: u64) -> anyhow::Result<()> {
    use evore::ore_api::round_pda;
    
    let round_pda_pubkey = round_pda(round_id).0;
    let round_pda_str = round_pda_pubkey.to_string();
    
    tracing::info!("Fetching transactions for round {} (PDA: {})", round_id, round_pda_str);
    
    // Get all signatures for this round
    let all_signatures = state.rpc.get_all_signatures_for_address(&round_pda_pubkey).await?;
    let total_count = all_signatures.len();
    
    // Filter out failed transactions (ones with errors)
    let signatures: Vec<_> = all_signatures.into_iter()
        .filter(|s| s.err.is_none())
        .collect();
    
    let failed_count = total_count - signatures.len();
    if failed_count > 0 {
        tracing::info!("Round {}: filtered out {} failed transactions", round_id, failed_count);
    }
    
    tracing::info!("Round {}: fetched {} successful signatures (of {} total)", round_id, signatures.len(), total_count);
    
    if signatures.is_empty() {
        tracing::warn!("Round {}: no successful transactions found", round_id);
        return Ok(());
    }
    
    // Store signatures with round PDA as initial account
    let sig_rows: Vec<SignatureRow> = signatures.iter().map(|s| SignatureRow {
        signature: s.signature.clone(),
        slot: s.slot,
        block_time: s.block_time.unwrap_or(0),
        accounts: vec![round_pda_str.clone()], // Will be updated when full tx fetched
    }).collect();
    state.clickhouse.insert_signatures(sig_rows).await?;
    
    // Filter to signatures not already stored (avoid redundant fetches)
    let mut sigs_to_fetch = Vec::new();
    for sig_info in &signatures {
        if !state.clickhouse.transaction_exists_v2(&sig_info.signature).await? {
            sigs_to_fetch.push(sig_info.clone());
        }
    }
    
    tracing::info!("Round {}: {} transactions need fetching ({} already stored)", 
        round_id, sigs_to_fetch.len(), signatures.len() - sigs_to_fetch.len());
    
    // Fetch transactions concurrently in batches of 100
    // RPC providers have built-in rate limiting, so we can fire many requests at once
    const BATCH_SIZE: usize = 100;
    const MAX_RETRIES: usize = 3;
    let mut stored_count = 0;
    let mut pending_sigs = sigs_to_fetch;
    
    for retry_attempt in 0..MAX_RETRIES {
        if pending_sigs.is_empty() {
            break;
        }
        
        if retry_attempt > 0 {
            tracing::info!("Round {}: retry attempt {} for {} failed transactions", 
                round_id, retry_attempt, pending_sigs.len());
            // Small delay before retry
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        
        let mut failed_sigs = Vec::new();
        
        for batch in pending_sigs.chunks(BATCH_SIZE) {
            // Spawn all fetch requests concurrently (don't await individually)
            let fetch_futures: Vec<_> = batch.iter().map(|sig_info| {
                let sig = sig_info.signature.clone();
                let slot = sig_info.slot;
                let rpc = state.rpc.clone();
                async move {
                    let result = rpc.get_transaction(&sig).await;
                    (sig, slot, result)
                }
            }).collect();
            
            // Wait for all requests in this batch to complete
            let results = futures::future::join_all(fetch_futures).await;
            
            // Process results
            let mut tx_rows = Vec::new();
            for (sig, slot, result) in results {
                match result {
                    Ok(Some(tx)) => {
                        // Parse accounts from raw JSON
                        let accounts = match parse_transaction_accounts(&tx.raw_json) {
                            Ok(acc) => acc,
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to parse accounts for tx {}: {}. Using round PDA only.",
                                    sig, e
                                );
                                vec![round_pda_str.clone()]
                            }
                        };
                        
                        tx_rows.push(RawTransactionV2 {
                            signature: sig,
                            slot,
                            block_time: tx.block_time.unwrap_or(0),
                            accounts,
                            raw_json: tx.raw_json,
                        });
                        
                        stored_count += 1;
                    }
                    Ok(None) => {
                        // Transaction not found - might be too old, don't retry
                        tracing::debug!("Transaction {} not found (may be too old)", sig);
                    }
                    Err(e) => {
                        // RPC error - add to retry list
                        tracing::warn!("Failed to fetch transaction {}: {}", sig, e);
                        // Find the original sig_info to retry
                        if let Some(sig_info) = batch.iter().find(|s| s.signature == sig) {
                            failed_sigs.push(sig_info.clone());
                        }
                    }
                }
            }
            
            // Insert this batch to ClickHouse
            if !tx_rows.is_empty() {
                state.clickhouse.insert_raw_transactions_v2(tx_rows).await?;
            }
            
            tracing::debug!("Round {}: processed batch of {} transactions", round_id, batch.len());
        }
        
        // Set up for next retry attempt
        pending_sigs = failed_sigs;
    }
    
    if !pending_sigs.is_empty() {
        tracing::warn!("Round {}: {} transactions failed after {} retries", 
            round_id, pending_sigs.len(), MAX_RETRIES);
    }
    
        tracing::info!(
        "Round {}: stored {} transactions ({} signatures total)",
        round_id, stored_count, signatures.len()
    );
    
    Ok(())
}

/// Store a partial round when finalization times out (top_miner not populated)
/// The partial round can be backfilled later via admin panel
async fn store_partial_round(
    state: &AppState,
    snapshot: &RoundSnapshot,
    failure_reason: &str,
) -> anyhow::Result<()> {
    let round_id = snapshot.round_id;
    
    // Fetch the round to get slot_hash and other data (even without top_miner)
    let round = match state.rpc.get_round(round_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to fetch round {} for partial storage: {}", round_id, e);
            // Create placeholder partial round
            let partial = PartialRoundInsert::from_snapshot(
            round_id,
                snapshot.start_slot,
                snapshot.end_slot,
                [0u8; 32], // No slot_hash available
                255, // Invalid winning square
                0, 0, 0, 0, 0, // No round data
                snapshot.miners.len() as u32,
                snapshot.deployments.len() as u32,
                format!("Round fetch failed: {}; Original: {}", e, failure_reason),
            );
            state.clickhouse.insert_partial_round(partial).await?;
            return Ok(());
        }
    };
    
    // Calculate stats from snapshot
    let unique_miners = snapshot.miners.len() as u32;
    let total_deployments = snapshot.deployments.values()
        .map(|squares| squares.len())
        .sum::<usize>() as u32;
    
    // Get winning_square if slot_hash is available
    let winning_square = if let Some(rng) = round.rng() {
        round.winning_square(rng) as u8
    } else {
        255 // Invalid - slot_hash not available
    };
    
    let partial = PartialRoundInsert::from_snapshot(
            round_id,
        snapshot.start_slot,
        snapshot.end_slot,
        round.slot_hash,
            winning_square,
        round.total_deployed,
        round.total_vaulted,
        round.total_winnings,
        round.top_miner_reward,
        round.motherlode,
        unique_miners,
        total_deployments,
        failure_reason.to_string(),
    );
    
    state.clickhouse.insert_partial_round(partial).await?;
    tracing::info!(
        "Stored partial round {}: winning_square={}, {} miners, {} deployments, reason={}",
        round_id, winning_square, unique_miners, total_deployments, failure_reason
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
    let mut last_round_with_slot_hash: Option<Round> = None;
    
    for attempt in 1..=max_attempts {
        match state.rpc.get_round(round_id).await {
            Ok(round) => {
                // Check if slot_hash is populated (not all zeros)
                let has_slot_hash = round.slot_hash != [0u8; 32];
                
                // Check if top_miner is populated (not default pubkey)
                let has_top_miner = round.top_miner != Pubkey::default();
                
                // Save the round if it has slot_hash (for fallback)
                if has_slot_hash {
                    last_round_with_slot_hash = Some(round.clone());
                }
                
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
    
    // Timeout! But if we have slot_hash, we can calculate optimistic top_miner
    if let Some(mut round) = last_round_with_slot_hash {
        tracing::warn!(
            "Round {} timed out waiting for on-chain top_miner - using optimistic calculation",
            round_id
        );
        
        // Calculate optimistic top_miner
        if let Some(optimistic_top_miner) = calculate_optimistic_top_miner(&round, snapshot) {
            tracing::info!(
                "Round {} using optimistic top_miner={}",
                round_id, optimistic_top_miner
            );
            round.top_miner = optimistic_top_miner;
            return Ok(round);
        } else {
            tracing::warn!(
                "Round {} could not calculate optimistic top_miner, using default",
                round_id
            );
            // Leave as default pubkey - the finalization will handle this
            return Ok(round);
        }
    }
    
    Err(anyhow::anyhow!(
        "Timeout waiting for round {} - no slot_hash available after {} seconds",
        round_id, max_attempts * 2
    ))
}

/// Calculate the optimistic top_miner from snapshot data
/// Returns None if calculation fails (no deployments on winning square, etc.)
fn calculate_optimistic_top_miner(round: &Round, snapshot: &RoundSnapshot) -> Option<Pubkey> {
    let rng = round.rng()?;
    let winning_square = round.winning_square(rng);
    let total_on_winning = round.deployed[winning_square];
    
    if total_on_winning == 0 {
        return None;
    }
    
    // If split reward, there's no single top_miner
    // The on-chain uses a split address, but we can just return the first miner
    // The finalization code checks is_split_reward for reward calculation
    if round.is_split_reward(rng) {
        // Find any miner on the winning square
        for (pubkey, miner) in &snapshot.miners {
            if miner.deployed[winning_square] > 0 {
                if let Ok(pk) = pubkey.parse::<Pubkey>() {
                    return Some(pk);
                }
            }
        }
        return None;
    }
    
    // Weighted random selection using top_miner_sample
    let sample = round.top_miner_sample(rng, winning_square);
    
    // Find the miner whose cumulative range contains the sample
    for (pubkey, miner) in &snapshot.miners {
        let deployed = miner.deployed[winning_square];
        if deployed == 0 {
            continue;
        }
        
        let cumulative = miner.cumulative[winning_square];
        let upper_bound = cumulative + deployed;
        
        // Check if sample falls in this miner's range [cumulative, upper_bound)
        if sample >= cumulative && sample < upper_bound {
            if let Ok(pk) = pubkey.parse::<Pubkey>() {
                return Some(pk);
            }
        }
    }
    
    None
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

