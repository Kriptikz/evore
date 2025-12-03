//! Bot Runner - Refactored bot using shared services
//!
//! Uses:
//! - BoardTracker for real-time board updates
//! - RoundTracker for deployment data
//! - BlockhashCache for transaction blockhash
//! - tx_pipeline for sending transactions
//! - BotState for state machine

use std::sync::Arc;
use std::time::Duration;

use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};
use tokio::sync::{mpsc, RwLock};
use tokio::time::sleep;

use crate::blockhash_cache::BlockhashCache;
use crate::board_tracker::BoardTracker;
use crate::bot_state::{BotPhase, BotState};
use crate::client::EvoreClient;
use crate::config::StrategyParams;
use crate::config::DeployStrategy;
use crate::deploy::{build_checkpoint_tx, build_claim_sol_tx, build_ev_deploy_tx, build_percentage_deploy_tx, EvDeployParams, PercentageDeployParams};
use crate::round_tracker::RoundTracker;
use crate::sender::PingStats;
use crate::slot_tracker::SlotTracker;
use crate::tui::{BotStatus, TuiUpdate, TxAction, TxType, TxStatus};
use crate::tx_pipeline::{create_tx_pipeline, TxRequest};

/// Shared services for all bots
pub struct SharedServices {
    pub slot_tracker: Arc<SlotTracker>,
    pub board_tracker: Arc<BoardTracker>,
    pub round_tracker: Arc<RoundTracker>,
    pub blockhash_cache: Arc<BlockhashCache>,
    pub tx_channel: mpsc::UnboundedSender<TxRequest>,
    pub fast_sender: Arc<crate::sender::FastSender>,
    pub ping_stats: Arc<PingStats>,
    pub client: Arc<EvoreClient>,
    rpc_url: String,
}

impl SharedServices {
    /// Create and start all shared services
    pub fn new(rpc_url: &str, ws_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let client = Arc::new(EvoreClient::new(rpc_url));
        
        // Create trackers
        let slot_tracker = Arc::new(SlotTracker::new(ws_url));
        let board_tracker = Arc::new(BoardTracker::new(ws_url));
        let round_tracker = Arc::new(RoundTracker::new(ws_url));
        let blockhash_cache = Arc::new(BlockhashCache::new(rpc_url));
        
        // Create tx pipeline using a new RPC client (tx_pipeline needs Arc<RpcClient>)
        let tx_rpc = Arc::new(solana_client::rpc_client::RpcClient::new_with_commitment(
            rpc_url.to_string(),
            solana_sdk::commitment_config::CommitmentConfig::confirmed(),
        ));
        let (tx_channel, fast_sender, ping_stats) = create_tx_pipeline(tx_rpc);
        
        Ok(Self {
            slot_tracker,
            board_tracker,
            round_tracker,
            blockhash_cache,
            tx_channel,
            fast_sender,
            ping_stats,
            client,
            rpc_url: rpc_url.to_string(),
        })
    }

    /// Start all background subscriptions
    pub fn start(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.slot_tracker
            .start_slot_subscription()
            .map_err(|e| format!("Slot subscription: {}", e))?;
        self.slot_tracker
            .start_blockhash_subscription(&self.rpc_url)
            .map_err(|e| format!("Blockhash subscription: {}", e))?;
        self.board_tracker.start_subscription()?;
        self.blockhash_cache.start_polling()?;
        // Note: RoundTracker needs initial round_id, started when board is available
        Ok(())
    }
}

/// Runtime configuration for a bot instance
pub struct BotRunConfig {
    pub name: String,
    pub bot_index: usize,
    pub auth_id: u64,
    pub manager: Pubkey,
    pub signer: Arc<Keypair>,
    pub slots_left: u64,
    pub strategy: DeployStrategy,
    pub strategy_params: StrategyParams,
    pub bankroll: u64,
    pub attempts: u64,   // Number of deploy txs to send (default 4)
    pub priority_fee: u64,  // Priority fee in micro-lamports per CU
    pub jito_tip: u64,   // Jito tip in lamports (default 200_000 = 0.0002 SOL)
}

/// Run a single bot using shared services
pub async fn run_bot_with_services(
    config: Arc<RwLock<BotRunConfig>>,
    services: Arc<SharedServices>,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
) {
    let mut state = BotState::new();
    
    // Extract static values that don't change at runtime
    let (signer, manager, auth_id, bot_index, bot_name) = {
        let cfg = config.read().await;
        (
            Arc::clone(&cfg.signer),
            cfg.manager,
            cfg.auth_id,
            cfg.bot_index,
            cfg.name.clone(),
        )
    };
    
    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager, auth_id);
    
    // Get initial signer balance
    if let Ok(balance) = services.client.rpc.get_balance(&signer.pubkey()) {
        let _ = tui_tx.send(TuiUpdate::BotSignerBalanceUpdate {
            bot_index,
            balance,
        });
    }

    // Wait for board tracker to have data
    loop {
        if services.board_tracker.get_board().is_some() {
            break;
        }
        sleep(Duration::from_millis(100)).await;
    }

    // Start round tracker with initial round
    let initial_round_id = services.board_tracker.get_round_id();
    if initial_round_id > 0 {
        services.round_tracker.switch_round(initial_round_id);
    }

    // Initialize miner state
    if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
        state.init_starting_values(miner.rewards_sol, miner.rewards_ore);
        
        let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
            bot_index,
            rounds_participated: 0,
            rounds_won: 0,
            rounds_skipped: 0,
            rounds_missed: 0,
            current_claimable_sol: miner.rewards_sol,
            current_ore: miner.rewards_ore,
        });
        
        let _ = tui_tx.send(TuiUpdate::BotMinerUpdate {
            bot_index,
            miner: miner.clone(),
        });

        // Check if already deployed to current round
        if let Some(board) = services.board_tracker.get_board() {
            if miner.round_id == board.round_id {
                state.last_deployed_round = Some(board.round_id);
                let deployed: u64 = miner.deployed.iter().sum();
                state.deployed_amount = deployed;
                state.set_phase(BotPhase::Deployed);
                
                let _ = tui_tx.send(TuiUpdate::BotDeployedUpdate {
                    bot_index,
                    amount: deployed,
                    round_id: board.round_id,
                });
            }
            
            // Check if previous round needs checkpointing
            if miner.round_id > miner.checkpoint_id {
                state.last_deployed_round = Some(miner.round_id);
            }
        }
    }

    // Main loop
    loop {
        // Get current state from trackers
        let board = match services.board_tracker.get_board() {
            Some(b) => {
                let _ = tui_tx.send(TuiUpdate::BoardUpdate(b));
                b
            }
            None => {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
        };

        // Check for new round and switch tracker
        if let Some(new_round_id) = services.board_tracker.check_new_round() {
            services.round_tracker.switch_round(new_round_id);
            state.reset_for_round(new_round_id);
            
            // At start of new round, check if previous round needs checkpointing
            // This handles cases where deploy failed but checkpoint is still needed
            if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
                if miner.round_id > miner.checkpoint_id {
                    // Miner deployed to a round that wasn't checkpointed yet
                    state.last_deployed_round = Some(miner.round_id);
                    state.last_checkpointed_round = Some(miner.checkpoint_id);
                    
                    let _ = tui_tx.send(TuiUpdate::BotMinerUpdate {
                        bot_index,
                        miner: miner.clone(),
                    });
                }
            }
        }

        // Send round data if available
        if let Some(round) = services.round_tracker.get_round() {
            let _ = tui_tx.send(TuiUpdate::RoundUpdate(round));
        }

        // Update slot/blockhash from caches
        let current_slot = services.slot_tracker.get_slot();
        let blockhash = services.blockhash_cache.get_blockhash();
        services.blockhash_cache.set_current_slot(current_slot);
        services.blockhash_cache.set_end_slot(board.end_slot);
        
        let _ = tui_tx.send(TuiUpdate::SlotUpdate { slot: current_slot, blockhash });

        // Read dynamic config values (can be updated via config reload)
        let (slots_left, strategy, strategy_params, bankroll, attempts, priority_fee, jito_tip) = {
            let cfg = config.read().await;
            (cfg.slots_left, cfg.strategy.clone(), cfg.strategy_params.clone(), cfg.bankroll, cfg.attempts, cfg.priority_fee, cfg.jito_tip)
        };

        // State machine logic
        match determine_phase(&board, current_slot, &state, slots_left) {
            BotPhase::Idle => {
                state.set_phase(BotPhase::Idle);
                send_status(&tui_tx, bot_index, BotStatus::Idle);
                sleep(Duration::from_millis(500)).await;
            }
            
            BotPhase::Checkpointing => {
                state.set_phase(BotPhase::Checkpointing);
                send_status(&tui_tx, bot_index, BotStatus::Checkpointing);
                
                if let Some(last_round) = state.last_deployed_round {
                    // Store pre-checkpoint values
                    if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
                        state.store_pre_checkpoint(miner.rewards_sol, miner.rewards_ore);
                    }
                    
                    // Send checkpoint
                    let bh = wait_for_blockhash(&services.blockhash_cache).await;
                    let checkpoint_tx = build_checkpoint_tx(
                        &signer,
                        &manager,
                        auth_id,
                        last_round,
                        bh,
                    );
                    
                    match services.client.rpc.send_and_confirm_transaction(&checkpoint_tx) {
                        Ok(sig) => {
                            send_tx_event_typed(&tui_tx, &bot_name, TxType::Checkpoint, TxStatus::Confirmed, sig, None, 
                                Some(current_slot), Some(last_round), None, None);
                            
                            // Update state after checkpoint
                            sleep(Duration::from_millis(500)).await;
                            
                            // Get miner data and extract values before any await
                            let miner_data = services.client.get_miner(&managed_miner_auth).ok().flatten();
                            
                            if let Some(miner) = miner_data {
                                let rewards_sol = miner.rewards_sol;
                                let rewards_ore = miner.rewards_ore;
                                
                                state.process_checkpoint(last_round, rewards_sol, rewards_ore);
                                
                                let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                                    bot_index,
                                    rounds_participated: state.rounds_participated,
                                    rounds_won: state.rounds_won,
                                    rounds_skipped: state.rounds_skipped,
                                    rounds_missed: state.rounds_missed,
                                    current_claimable_sol: state.current_claimable_sol,
                                    current_ore: state.current_ore,
                                });

                                let _ = tui_tx.send(TuiUpdate::BotMinerUpdate {
                                    bot_index,
                                    miner: miner.clone(),
                                });
                                
                                // Claim if rewards available
                                if rewards_sol > 0 {
                                    state.set_phase(BotPhase::Claiming);
                                    let bh = wait_for_blockhash(&services.blockhash_cache).await;
                                    let claim_tx = build_claim_sol_tx(
                                        &signer,
                                        &manager,
                                        auth_id,
                                        bh,
                                    );
                                    
                                    match services.client.rpc.send_and_confirm_transaction(&claim_tx) {
                                        Ok(sig) => {
                                            send_tx_event_typed(&tui_tx, &bot_name, TxType::ClaimSol, TxStatus::Confirmed, sig, None,
                                                Some(current_slot), None, Some(rewards_sol), None);
                                            // Track claimed amount for accurate P&L
                                            let _ = tui_tx.send(TuiUpdate::BotClaimedSol {
                                                bot_index,
                                                amount: rewards_sol,
                                            });
                                            update_signer_balance(&services, bot_index, &signer, &tui_tx).await;
                                        }
                                        Err(e) => {
                                            send_tx_event_typed(&tui_tx, &bot_name, TxType::ClaimSol, TxStatus::Failed, Signature::default(), Some(format!("{}", e)),
                                                Some(current_slot), None, Some(rewards_sol), None);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            send_tx_event_typed(&tui_tx, &bot_name, TxType::Checkpoint, TxStatus::Failed, Signature::default(), Some(format!("{}", e)),
                                Some(current_slot), Some(last_round), None, None);
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            }
            
            BotPhase::Waiting => {
                state.set_phase(BotPhase::Waiting);
                send_status(&tui_tx, bot_index, BotStatus::Waiting);
                sleep(Duration::from_millis(50)).await;
            }
            
            BotPhase::Deploying => {
                state.set_phase(BotPhase::Deploying);
                send_status(&tui_tx, bot_index, BotStatus::Deploying);
                
                // Send deploy transactions at 100ms intervals
                // Each tx has a unique attempts value to generate different signatures
                let mut signatures = Vec::new();
                
                // Get blockhash once and reuse for all attempts
                let bh = services.blockhash_cache.get_blockhash();
                if bh == Hash::default() {
                    // No blockhash available, skip this round
                    state.reset_for_round(board.round_id);
                    continue;
                }
                
                // Only EV strategy supports multiple attempts (via attempts field in instruction)
                // Percentage and Manual would create identical txs with same signature
                let num_attempts = match strategy {
                    DeployStrategy::EV => attempts,
                    DeployStrategy::Percentage | DeployStrategy::Manual => 1,
                };
                
                for attempt in 0..num_attempts {
                    let current = services.slot_tracker.get_slot();
                    if current >= board.end_slot {
                        break;
                    }
                    
                    // Build deploy transaction based on strategy
                    let deploy_tx = match strategy {
                        DeployStrategy::EV => {
                            let mut params = build_ev_params_from_values(&strategy_params, bankroll, slots_left);
                            params.attempts = attempt;  // Each tx has unique attempts value
                            build_ev_deploy_tx(
                                &signer,
                                &manager,
                                auth_id,
                                board.round_id,
                                &params,
                                bh,
                                priority_fee,
                                jito_tip,
                            )
                        }
                        DeployStrategy::Percentage => {
                            let params = build_percentage_params_from_values(&strategy_params, bankroll);
                            build_percentage_deploy_tx(
                                &signer,
                                &manager,
                                auth_id,
                                board.round_id,
                                &params,
                                bh,
                                priority_fee,
                                jito_tip,
                            )
                        }
                        DeployStrategy::Manual => {
                            // TODO: Implement manual strategy - falls back to EV for now
                            let mut params = build_ev_params_from_values(&strategy_params, bankroll, slots_left);
                            params.attempts = attempt;
                            build_ev_deploy_tx(
                                &signer,
                                &manager,
                                auth_id,
                                board.round_id,
                                &params,
                                bh,
                                priority_fee,
                                jito_tip,
                            )
                        }
                    };
                    
                    // Use FastSender for deploy transactions (automatic 4x retry via Helius)
                    match services.fast_sender.send_transaction(&deploy_tx) {
                        Ok(sig) => {
                            signatures.push(sig);
                            send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Sent, sig, None,
                                Some(current), Some(board.round_id), Some(bankroll), Some(attempt));
                        }
                        Err(e) => {
                            send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Failed, Signature::default(), Some(e.to_string()),
                                Some(current), Some(board.round_id), Some(bankroll), Some(attempt));
                        }
                    }
                    
                    // Sleep between attempts (except after last one)
                    if attempt < num_attempts - 1 {
                        sleep(Duration::from_millis(100)).await;
                    }
                }
                
                // Check confirmations
                if !signatures.is_empty() {
                    sleep(Duration::from_secs(3)).await;
                    
                    let mut any_confirmed = false;
                    let mut ev_skip = false;
                    let mut had_other_error = false;
                    
                    for sig in &signatures {
                        match services.client.get_transaction_status(sig) {
                            Ok(Some(status)) => {
                                if status.err.is_none() {
                                    any_confirmed = true;
                                    send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Confirmed, *sig, None,
                                        Some(status.slot), Some(board.round_id), Some(bankroll), None);
                                } else {
                                    // Transaction landed but failed on-chain
                                    let err = status.err.unwrap();
                                    let err_msg = format!("{:?}", err);
                                    
                                    // Map Evore program error codes to human-readable names
                                    let friendly_err = parse_evore_error(&err_msg);
                                    
                                    // Check error type:
                                    // - Custom(7): NoDeployments (EV skip) - count as skip
                                    // - Custom(9): AlreadyDeployed - means one of our txs landed, treat as success
                                    if err_msg.contains("Custom(7)") {
                                        ev_skip = true;
                                    } else if err_msg.contains("Custom(9)") {
                                        // Already deployed this round - one of our earlier txs landed
                                        // This is expected when sending multiple attempts
                                        any_confirmed = true;
                                    } else {
                                        had_other_error = true;
                                    }
                                    
                                    send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Failed, *sig, Some(friendly_err),
                                        Some(status.slot), Some(board.round_id), Some(bankroll), None);
                                }
                            }
                            Ok(None) => {
                                // Transaction not found - expired or dropped
                                had_other_error = true;
                                send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Failed, *sig, Some("Tx expired/dropped".to_string()),
                                    None, Some(board.round_id), Some(bankroll), None);
                            }
                            Err(e) => {
                                had_other_error = true;
                                send_tx_event_typed(&tui_tx, &bot_name, TxType::Deploy, TxStatus::Failed, *sig, Some(format!("RPC: {}", e)),
                                    None, Some(board.round_id), Some(bankroll), None);
                            }
                        }
                    }
                    
                    // Count EV skips (only once per round even if multiple txs failed with Custom(7))
                    if ev_skip && !any_confirmed {
                        state.rounds_skipped += 1;
                        send_status(&tui_tx, bot_index, BotStatus::Skipped);
                        let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                            bot_index,
                            rounds_participated: state.rounds_participated,
                            rounds_won: state.rounds_won,
                            rounds_skipped: state.rounds_skipped,
                            rounds_missed: state.rounds_missed,
                            current_claimable_sol: state.current_claimable_sol,
                            current_ore: state.current_ore,
                        });
                        // Mark round as handled (both deployed and checkpointed) so we don't retry
                        // and don't try to checkpoint a round we skipped
                        state.last_deployed_round = Some(board.round_id);
                        state.last_checkpointed_round = Some(board.round_id);
                    }
                    
                    // Count missed rounds (tx failed for reasons other than EV skip)
                    if had_other_error && !any_confirmed && !ev_skip {
                        state.rounds_missed += 1;
                        send_status(&tui_tx, bot_index, BotStatus::Missed);
                        let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                            bot_index,
                            rounds_participated: state.rounds_participated,
                            rounds_won: state.rounds_won,
                            rounds_skipped: state.rounds_skipped,
                            rounds_missed: state.rounds_missed,
                            current_claimable_sol: state.current_claimable_sol,
                            current_ore: state.current_ore,
                        });
                        // Mark round as handled so we don't retry
                        state.last_deployed_round = Some(board.round_id);
                        state.last_checkpointed_round = Some(board.round_id);
                    }
                    
                    if any_confirmed {
                        // Deployment succeeded - set status to Deployed
                        send_status(&tui_tx, bot_index, BotStatus::Deployed);
                        
                        // Get deployed amount from miner
                        if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
                            let deployed: u64 = miner.deployed.iter().sum();
                            state.record_deployment(board.round_id, deployed);
                            
                            let _ = tui_tx.send(TuiUpdate::BotDeployedUpdate {
                                bot_index,
                                amount: deployed,
                                round_id: board.round_id,
                            });
                            
                            let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                                bot_index,
                                rounds_participated: state.rounds_participated,
                                rounds_won: state.rounds_won,
                                rounds_skipped: state.rounds_skipped,
                                rounds_missed: state.rounds_missed,
                                current_claimable_sol: state.current_claimable_sol,
                                current_ore: state.current_ore,
                            });
                        }
                        
                        update_signer_balance(&services, bot_index, &signer, &tui_tx).await;
                    }
                }
            }
            
            BotPhase::Deployed => {
                // Don't change status here - preserve Skipped/Missed/Deployed status
                // Status is set when deployment actually succeeds or when skip/miss is detected
                state.set_phase(BotPhase::Deployed);
                sleep(Duration::from_millis(100)).await;
            }
            
            BotPhase::Claiming => {
                // Handled within Checkpointing
                sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Determine what phase the bot should be in
fn determine_phase(
    board: &evore::ore_api::Board,
    current_slot: u64,
    state: &BotState,
    slots_left_threshold: u64,
) -> BotPhase {
    // Round not active
    if board.end_slot == u64::MAX {
        return BotPhase::Idle;
    }
    
    // Round ended
    if current_slot >= board.end_slot {
        if state.already_deployed(board.round_id) {
            return BotPhase::Deployed;
        }
        return BotPhase::Idle;
    }
    
    // Already deployed this round
    if state.already_deployed(board.round_id) {
        return BotPhase::Deployed;
    }
    
    // Needs checkpoint from previous round
    if state.needs_checkpoint() {
        return BotPhase::Checkpointing;
    }
    
    // Calculate deploy window
    let deploy_start_slot = if slots_left_threshold > 10 {
        board.end_slot.saturating_sub(slots_left_threshold - 1)
    } else {
        board.end_slot.saturating_sub(slots_left_threshold)
    };
    
    if current_slot >= deploy_start_slot.saturating_sub(1) {
        return BotPhase::Deploying;
    }
    
    BotPhase::Waiting
}

/// Wait for valid blockhash from cache
async fn wait_for_blockhash(cache: &BlockhashCache) -> Hash {
    loop {
        let bh = cache.get_blockhash();
        if bh != Hash::default() {
            return bh;
        }
        sleep(Duration::from_millis(50)).await;
    }
}

/// Send bot status update
fn send_status(tx: &mpsc::UnboundedSender<TuiUpdate>, bot_index: usize, status: BotStatus) {
    let _ = tx.send(TuiUpdate::BotStatusUpdate { bot_index, status });
}

/// Send transaction event (legacy)
fn send_tx_event(
    tx: &mpsc::UnboundedSender<TuiUpdate>,
    bot_name: &str,
    action: TxAction,
    signature: Signature,
    error: Option<String>,
) {
    let _ = tx.send(TuiUpdate::TxEvent {
        bot_name: bot_name.to_string(),
        action,
        signature,
        error,
    });
}

/// Send transaction event with type info and details
fn send_tx_event_typed(
    tx: &mpsc::UnboundedSender<TuiUpdate>,
    bot_name: &str,
    tx_type: TxType,
    status: TxStatus,
    signature: Signature,
    error: Option<String>,
    slot: Option<u64>,
    round_id: Option<u64>,
    amount: Option<u64>,
    attempt: Option<u64>,
) {
    let _ = tx.send(TuiUpdate::TxEventTyped {
        bot_name: bot_name.to_string(),
        tx_type,
        status,
        signature,
        error,
        slot,
        round_id,
        amount,
        attempt,
    });
}

/// Update signer balance after transaction
async fn update_signer_balance(
    services: &SharedServices,
    bot_index: usize,
    signer: &Keypair,
    tx: &mpsc::UnboundedSender<TuiUpdate>,
) {
    if let Ok(balance) = services.client.rpc.get_balance(&signer.pubkey()) {
        let _ = tx.send(TuiUpdate::BotSignerBalanceUpdate {
            bot_index,
            balance,
        });
    }
}

/// Build EV deploy params from values (for runtime config updates)
fn build_ev_params_from_values(strategy_params: &StrategyParams, bankroll: u64, slots_left: u64) -> EvDeployParams {
    match strategy_params {
        StrategyParams::EV { max_per_square, min_bet, ore_value } => {
            EvDeployParams {
                bankroll,
                max_per_square: *max_per_square,
                min_bet: *min_bet,
                ore_value: *ore_value,
                slots_left,
                attempts: 0,  // Will be set per-tx in deploy loop
                allow_multi_deploy: false,  // Don't allow multiple deploys per round
            }
        }
        _ => EvDeployParams {
            bankroll,
            max_per_square: 100_000_000,
            min_bet: 10_000,
            ore_value: 800_000_000,
            slots_left,
            attempts: 0,
            allow_multi_deploy: false,
        }
    }
}

/// Build Percentage deploy params from values (for runtime config updates)
fn build_percentage_params_from_values(strategy_params: &StrategyParams, bankroll: u64) -> PercentageDeployParams {
    match strategy_params {
        StrategyParams::Percentage { percentage, squares_count } => {
            PercentageDeployParams {
                bankroll,
                percentage: *percentage,
                squares_count: *squares_count,
                slots_left: 0, // Not used for percentage strategy
            }
        }
        _ => PercentageDeployParams {
            bankroll,
            percentage: 100,  // 1%
            squares_count: 20,
            slots_left: 0,
        }
    }
}

/// Parse Evore program error codes into human-readable messages
/// Error codes from program/src/error.rs
fn parse_evore_error(err_str: &str) -> String {
    // Extract custom error code if present
    if let Some(code) = extract_custom_error(err_str) {
        match code {
            1 => "NotAuthorized".to_string(),
            2 => "TooManySlotsLeft".to_string(),
            3 => "EndSlotExceeded".to_string(),
            4 => "InvalidPDA".to_string(),
            5 => "ManagerNotInitialized".to_string(),
            6 => "InvalidFeeCollector".to_string(),
            7 => "NoDeployments (EV skip)".to_string(),
            8 => "ArithmeticOverflow".to_string(),
            9 => "AlreadyDeployed".to_string(),
            _ => format!("Custom({})", code),
        }
    } else {
        // Return truncated original error if not a custom error
        if err_str.len() > 50 {
            format!("{}...", &err_str[..50])
        } else {
            err_str.to_string()
        }
    }
}

/// Extract custom error code from error string like "InstructionError(0, Custom(7))"
fn extract_custom_error(err_str: &str) -> Option<u32> {
    // Look for "Custom(N)" pattern
    if let Some(start) = err_str.find("Custom(") {
        let after_custom = &err_str[start + 7..];
        if let Some(end) = after_custom.find(')') {
            if let Ok(code) = after_custom[..end].parse::<u32>() {
                return Some(code);
            }
        }
    }
    None
}
