//! Bot Task - Runs deployment lifecycle independently in tokio task
//!
//! Architecture:
//! - Spawned as tokio task, doesn't block TUI
//! - Sends TuiUpdate messages via channel
//! - Handles full lifecycle: checkpoint → wait → deploy → claim

use std::sync::Arc;
use std::time::Duration;

use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::client::EvoreClient;
use crate::deploy::{build_checkpoint_tx, build_claim_sol_tx, build_ev_deploy_tx, EvDeployParams};
use crate::slot_tracker::SlotTracker;
use crate::tui::{BotStatus, TuiUpdate, TxAction, TxType, TxStatus};

/// Bot task configuration
#[derive(Clone)]
pub struct BotConfig {
    pub name: String,
    pub bot_index: usize,
    pub auth_id: u64,
    pub manager: Pubkey,
    pub params: EvDeployParams,
}

/// Run the bot deployment loop
/// 
/// This runs in a separate tokio task and sends updates to the TUI via the channel.
pub async fn run_bot_task(
    config: BotConfig,
    client: Arc<EvoreClient>,
    slot_tracker: Arc<SlotTracker>,
    signer: Arc<Keypair>,
    tx: mpsc::UnboundedSender<TuiUpdate>,
) {
    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(config.manager, config.auth_id);
    
    let mut last_round_deployed: Option<u64> = None;
    let mut last_round_checkpointed: Option<u64> = None;
    
    // Session stats tracking (for P&L) - starting values set in TUI on first update
    let mut rounds_participated: u64 = 0;
    let mut rounds_won: u64 = 0;
    let mut rounds_skipped: u64 = 0;
    let mut current_claimable_sol: u64 = 0;
    let mut current_ore: u64 = 0;
    let mut last_rewards_sol_before_checkpoint: u64 = 0;
    let mut last_rewards_ore_before_checkpoint: u64 = 0;
    
    // Get initial signer (fee payer) balance
    if let Ok(balance) = client.get_balance(&signer.pubkey()) {
        let _ = tx.send(TuiUpdate::BotSignerBalanceUpdate {
            bot_index: config.bot_index,
            balance,
        });
    }
    
    // Check initial state - did we already deploy to current round?
    if let Ok(board) = client.get_board() {
        let _ = tx.send(TuiUpdate::BoardUpdate(board.clone()));
        
        // Fetch and send round data
        if let Ok(round) = client.get_round(board.round_id) {
            let _ = tx.send(TuiUpdate::RoundUpdate(round));
        }
        
        if let Ok(Some(miner)) = client.get_miner(&managed_miner_auth) {
            // Initialize current claimable balances for P&L tracking
            // Starting values are set in TUI on first stats update
            current_claimable_sol = miner.rewards_sol;
            current_ore = miner.rewards_ore;
            
                // Send initial stats (TUI will set starting values on first update)
                let _ = tx.send(TuiUpdate::BotStatsUpdate {
                    bot_index: config.bot_index,
                    rounds_participated: 0,
                    rounds_won: 0,
                    rounds_skipped: 0,
                    rounds_missed: 0,
                    current_claimable_sol,
                    current_ore,
                });
            
            let _ = tx.send(TuiUpdate::BotMinerUpdate {
                bot_index: config.bot_index,
                miner: miner.clone(),
            });
            
            if miner.round_id == board.round_id {
                last_round_deployed = Some(board.round_id);
                let deployed: u64 = miner.deployed.iter().sum();
                let _ = tx.send(TuiUpdate::BotDeployedUpdate {
                    bot_index: config.bot_index,
                    amount: deployed,
                    round_id: board.round_id,
                });
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Deployed,
                });
            }
            
            // Check if previous round needs checkpointing
            if miner.round_id > miner.checkpoint_id {
                last_round_deployed = Some(miner.round_id);
            }
        }
    }
    
    // Track last round data fetch for periodic updates
    let mut last_round_fetch: Option<u64> = None;
    
    loop {
        // Get current state
        let board = match client.get_board() {
            Ok(b) => {
                let _ = tx.send(TuiUpdate::BoardUpdate(b.clone()));
                Some(b)
            }
            Err(e) => {
                // Convert error to String and drop e immediately
                let _ = tx.send(TuiUpdate::Error(format!("Board fetch: {}", e)));
                None
            }
        };
        
        // Handle board fetch failure outside the match to drop the error
        let board = match board {
            Some(b) => b,
            None => {
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        
        // Fetch round data periodically or when round changes
        if last_round_fetch != Some(board.round_id) {
            if let Ok(round) = client.get_round(board.round_id) {
                let _ = tx.send(TuiUpdate::RoundUpdate(round));
                last_round_fetch = Some(board.round_id);
            }
        }
        
        // Send slot update (blockhash fetched on demand when needed)
        let current_slot = slot_tracker.get_slot();
        let blockhash = client.get_latest_blockhash().unwrap_or_default();
        let _ = tx.send(TuiUpdate::SlotUpdate { slot: current_slot, blockhash });
        
        let already_deployed = last_round_deployed == Some(board.round_id);
        
        // === PHASE: WAITING FOR ROUND START ===
        if board.end_slot == u64::MAX {
            let _ = tx.send(TuiUpdate::BotStatusUpdate {
                bot_index: config.bot_index,
                status: BotStatus::Idle,
            });
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        
        // === PHASE: ROUND ENDED (INTERMISSION/RESET) ===
        if current_slot >= board.end_slot {
            if already_deployed {
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Deployed,
                });
            } else {
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Idle,
                });
            }
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        
        // === PHASE: ROUND ACTIVE ===
        
        // Already deployed this round - just wait
        if already_deployed {
            let _ = tx.send(TuiUpdate::BotStatusUpdate {
                bot_index: config.bot_index,
                status: BotStatus::Deployed,
            });
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        
        // === NEW ROUND: CHECKPOINT PREVIOUS ROUND FIRST ===
        if let Some(last_round) = last_round_deployed {
            if last_round_checkpointed != Some(last_round) {
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Checkpointing,
                });
                
                // Store pre-checkpoint rewards for delta calculation
                let miner_before = client.get_miner(&managed_miner_auth).ok().flatten();
                if let Some(m) = &miner_before {
                    last_rewards_sol_before_checkpoint = m.rewards_sol;
                    last_rewards_ore_before_checkpoint = m.rewards_ore;
                }
                
                // Get blockhash
                let bh = wait_for_blockhash_from_client(&client).await;
                
                // Send checkpoint
                let checkpoint_tx = build_checkpoint_tx(&signer, &config.manager, config.auth_id, last_round, bh);
                let checkpoint_result = client.send_and_confirm_transaction(&checkpoint_tx)
                    .map_err(|e| e.to_string());
                
                match checkpoint_result {
                    Ok(sig) => {
                        last_round_checkpointed = Some(last_round);
                        let _ = tx.send(TuiUpdate::TxEventTyped {
                            bot_name: config.name.clone(),
                            tx_type: TxType::Checkpoint,
                            status: TxStatus::Confirmed,
                            signature: sig,
                            error: None,
                            slot: None,
                            round_id: Some(last_round),
                            amount: None,
                            attempt: None,
                        });
                    }
                    Err(err_msg) => {
                        let _ = tx.send(TuiUpdate::TxEventTyped {
                            bot_name: config.name.clone(),
                            tx_type: TxType::Checkpoint,
                            status: TxStatus::Failed,
                            signature: Signature::default(),
                            error: Some(err_msg),
                            slot: None,
                            round_id: Some(last_round),
                            amount: None,
                            attempt: None,
                        });
                        // Failed - retry next loop
                        sleep(Duration::from_millis(500)).await;
                        continue;
                    }
                }
                
                // Wait for state to update, then get new miner data
                sleep(Duration::from_millis(500)).await;
                
                // Get updated miner data after checkpoint
                let miner_data = client.get_miner(&managed_miner_auth).ok().flatten();
                
                if let Some(miner) = miner_data {
                    // Calculate rewards delta from this round
                    let sol_delta = miner.rewards_sol.saturating_sub(last_rewards_sol_before_checkpoint);
                    let ore_delta = miner.rewards_ore.saturating_sub(last_rewards_ore_before_checkpoint);
                    
                    if sol_delta > 0 || ore_delta > 0 {
                        rounds_won += 1;
                    }
                    
                    // Update current claimable balances for P&L tracking
                    current_claimable_sol = miner.rewards_sol;
                    current_ore = miner.rewards_ore;
                    
                    // Send stats update with P&L
                    let _ = tx.send(TuiUpdate::BotStatsUpdate {
                        bot_index: config.bot_index,
                        rounds_participated,
                        rounds_won,
                        rounds_skipped,
                        rounds_missed: 0,  // TODO: track missed rounds in legacy bot_task
                        current_claimable_sol,
                        current_ore,
                    });
                    
                    let _ = tx.send(TuiUpdate::BotMinerUpdate {
                        bot_index: config.bot_index,
                        miner: miner.clone(),
                    });
                    
                    let should_claim = miner.rewards_sol > 0;
                    
                    if should_claim {
                        let rewards = miner.rewards_sol;
                        let bh = wait_for_blockhash_from_client(&client).await;
                        let claim_tx = build_claim_sol_tx(&signer, &config.manager, config.auth_id, bh);
                        match client.send_and_confirm_transaction(&claim_tx) {
                            Ok(sig) => {
                                let _ = tx.send(TuiUpdate::TxEventTyped {
                                    bot_name: config.name.clone(),
                                    tx_type: TxType::ClaimSol,
                                    status: TxStatus::Confirmed,
                                    signature: sig,
                                    error: None,
                                    slot: None,
                                    round_id: None,
                                    amount: Some(rewards),
                                    attempt: None,
                                });
                                
                                // Update signer balance after claim (spent fees)
                                if let Ok(balance) = client.get_balance(&signer.pubkey()) {
                                    let _ = tx.send(TuiUpdate::BotSignerBalanceUpdate {
                                        bot_index: config.bot_index,
                                        balance,
                                    });
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(TuiUpdate::TxEventTyped {
                                    bot_name: config.name.clone(),
                                    tx_type: TxType::ClaimSol,
                                    status: TxStatus::Failed,
                                    signature: Signature::default(),
                                    error: Some(format!("{}", e)),
                                    slot: None,
                                    round_id: None,
                                    amount: Some(rewards),
                                    attempt: None,
                                });
                            }
                        }
                    }
                }
                
                // Reset deployed amount for new round
                let _ = tx.send(TuiUpdate::BotDeployedUpdate {
                    bot_index: config.bot_index,
                    amount: 0,
                    round_id: board.round_id,
                });
            }
        }
        
        // === WAIT FOR DEPLOY WINDOW ===
        // Calculate deploy window (same logic as single_deploy)
        // For single send (slots_left > 10), wait 1 extra slot to ensure on-chain check passes
        // let deploy_start_slot = config.params.slots_left > 10 {
        //     board.end_slot.saturating_sub(config.params.slots_left - 1)
        // } else {
        //     board.end_slot.saturating_sub(config.params.slots_left)
        // };

        let deploy_start_slot = config.params.slots_left;
        
        // Wait until one slot BEFORE deploy_start_slot
        let wait_until_slot = deploy_start_slot.saturating_sub(1);
        
        if current_slot < wait_until_slot {
            let _ = tx.send(TuiUpdate::BotStatusUpdate {
                bot_index: config.bot_index,
                status: BotStatus::Waiting,
            });
            // Tight loop waiting for the right slot
            sleep(Duration::from_millis(50)).await;
            continue;
        }
        
        // We're at or past wait_until_slot - wait 50ms to be mid-slot, then deploy
        if current_slot == wait_until_slot {
            sleep(Duration::from_millis(50)).await;
        }
        
        // === DEPLOY WINDOW REACHED ===
        let _ = tx.send(TuiUpdate::BotStatusUpdate {
            bot_index: config.bot_index,
            status: BotStatus::Deploying,
        });
        
        // Determine send strategy based on slots_left
        // slots_left <= 2 → 100ms, slots_left <= 4 → 400ms, else single send
        let send_interval_ms: u64 = if config.params.slots_left <= 2 {
            100 // Fast spam: every 100ms
        } else if config.params.slots_left <= 4 {
            400 // Medium spam: every 400ms
        } else {
            0 // Single send, no spam
        };
        
        let mut signatures: Vec<Signature> = Vec::new();
        
        // Spam loop - keep sending until round ends (same as single_deploy)
        loop {
            let current = slot_tracker.get_slot();
            
            // Stop if we've reached end slot
            if current >= board.end_slot {
                break;
            }
            
            let blockhash_result = client.get_latest_blockhash().map_err(|e| e.to_string());
            let bh = match blockhash_result {
                Ok(bh) => bh,
                Err(_) => {
                    sleep(Duration::from_millis(10)).await;
                    continue;
                }
            };
            
            let deploy_tx = build_ev_deploy_tx(
                &signer,
                &config.manager,
                config.auth_id,
                board.round_id,
                &config.params,
                false,  // allow_multi_deploy - default to false
                bh,
                5000,    // default priority fee
                200_000, // default jito tip (0.0002 SOL)
            );
            
            match client.send_transaction_no_wait(&deploy_tx) {
                Ok(sig) => {
                    signatures.push(sig);
                    let _ = tx.send(TuiUpdate::TxEventTyped {
                        bot_name: config.name.clone(),
                        tx_type: TxType::Deploy,
                        status: TxStatus::Sent,
                        signature: sig,
                        error: None,
                        slot: Some(current_slot),
                        round_id: Some(board.round_id),
                        amount: Some(config.params.bankroll),
                        attempt: None,
                    });
                }
                Err(e) => {
                    let _ = tx.send(TuiUpdate::TxEventTyped {
                        bot_name: config.name.clone(),
                        tx_type: TxType::Deploy,
                        status: TxStatus::Failed,
                        signature: Signature::default(),
                        error: Some(e.to_string()),
                        slot: Some(current_slot),
                        round_id: Some(board.round_id),
                        amount: Some(config.params.bankroll),
                        attempt: None,
                    });
                }
            }
            
            // Single send mode - break after first tx
            if send_interval_ms == 0 {
                break;
            }
            
            // Wait before next tx
            sleep(Duration::from_millis(send_interval_ms)).await;
        }
        
        // Wait and check confirmations
        if !signatures.is_empty() {
            sleep(Duration::from_secs(3)).await;
            
            let mut any_confirmed = false;
            for sig in &signatures {
                match client.get_transaction_status(sig) {
                    Ok(Some(status)) => {
                        if status.err.is_none() {
                            any_confirmed = true;
                            let _ = tx.send(TuiUpdate::TxEventTyped {
                                bot_name: config.name.clone(),
                                tx_type: TxType::Deploy,
                                status: TxStatus::Confirmed,
                                signature: *sig,
                                error: None,
                                slot: Some(status.slot),
                                round_id: Some(board.round_id),
                                amount: Some(config.params.bankroll),
                                attempt: None,
                            });
                        } else {
                            // Transaction landed but failed on-chain
                            let err_msg = format!("{:?}", status.err.unwrap());
                            let _ = tx.send(TuiUpdate::TxEventTyped {
                                bot_name: config.name.clone(),
                                tx_type: TxType::Deploy,
                                status: TxStatus::Failed,
                                signature: *sig,
                                error: Some(err_msg),
                                slot: Some(status.slot),
                                round_id: Some(board.round_id),
                                amount: Some(config.params.bankroll),
                                attempt: None,
                            });
                        }
                    }
                    Ok(None) => {
                        // Transaction not found - expired or dropped
                        let _ = tx.send(TuiUpdate::TxEventTyped {
                            bot_name: config.name.clone(),
                            tx_type: TxType::Deploy,
                            status: TxStatus::Failed,
                            signature: *sig,
                            error: Some("Tx expired/dropped".to_string()),
                            slot: None,
                            round_id: Some(board.round_id),
                            amount: Some(config.params.bankroll),
                            attempt: None,
                        });
                    }
                    Err(e) => {
                        // RPC error checking status
                        let _ = tx.send(TuiUpdate::TxEventTyped {
                            bot_name: config.name.clone(),
                            tx_type: TxType::Deploy,
                            status: TxStatus::Failed,
                            signature: *sig,
                            error: Some(format!("RPC: {}", e)),
                            slot: None,
                            round_id: Some(board.round_id),
                            amount: Some(config.params.bankroll),
                            attempt: None,
                        });
                    }
                }
            }
            
            if any_confirmed {
                last_round_deployed = Some(board.round_id);
                rounds_participated += 1;
                
                let _ = tx.send(TuiUpdate::BotStatsUpdate {
                    bot_index: config.bot_index,
                    rounds_participated,
                    rounds_won,
                    rounds_skipped,
                    rounds_missed: 0,  // TODO: track missed rounds in legacy bot_task
                    current_claimable_sol,
                    current_ore,
                });
                
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Deployed,
                });
                
                // Update deployed amount and round data
                let miner_data = client.get_miner(&managed_miner_auth).ok().flatten();
                if let Some(miner) = miner_data {
                    let deployed: u64 = miner.deployed.iter().sum();
                    let _ = tx.send(TuiUpdate::BotDeployedUpdate {
                        bot_index: config.bot_index,
                        amount: deployed,
                        round_id: board.round_id,
                    });
                    let _ = tx.send(TuiUpdate::BotMinerUpdate {
                        bot_index: config.bot_index,
                        miner,
                    });
                }
                
                // Refresh round data to show updated deployments
                if let Ok(round) = client.get_round(board.round_id) {
                    let _ = tx.send(TuiUpdate::RoundUpdate(round));
                }
                
                // Update signer balance after deploy (spent fees)
                if let Ok(balance) = client.get_balance(&signer.pubkey()) {
                    let _ = tx.send(TuiUpdate::BotSignerBalanceUpdate {
                        bot_index: config.bot_index,
                        balance,
                    });
                }
            } else {
                // All failed - will retry next loop if round still active
                let _ = tx.send(TuiUpdate::BotStatusUpdate {
                    bot_index: config.bot_index,
                    status: BotStatus::Waiting,
                });
            }
        }
    }
}

/// Wait for a valid blockhash from the client
async fn wait_for_blockhash_from_client(client: &EvoreClient) -> Hash {
    loop {
        if let Ok(bh) = client.get_latest_blockhash() {
            return bh;
        }
        sleep(Duration::from_millis(50)).await;
    }
}
