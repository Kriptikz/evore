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
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::blockhash_cache::BlockhashCache;
use crate::board_tracker::BoardTracker;
use crate::bot_state::{BotPhase, BotState};
use crate::client::EvoreClient;
use crate::config::StrategyParams;
use crate::deploy::{build_checkpoint_tx, build_claim_sol_tx, build_ev_deploy_tx, EvDeployParams};
use crate::round_tracker::RoundTracker;
use crate::slot_tracker::SlotTracker;
use crate::tui::{BotStatus, TuiUpdate, TxAction};
use crate::tx_pipeline::{create_tx_pipeline, TxRequest};

/// Shared services for all bots
pub struct SharedServices {
    pub slot_tracker: Arc<SlotTracker>,
    pub board_tracker: Arc<BoardTracker>,
    pub round_tracker: Arc<RoundTracker>,
    pub blockhash_cache: Arc<BlockhashCache>,
    pub tx_channel: mpsc::UnboundedSender<TxRequest>,
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
        let tx_channel = create_tx_pipeline(tx_rpc);
        
        Ok(Self {
            slot_tracker,
            board_tracker,
            round_tracker,
            blockhash_cache,
            tx_channel,
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
    pub strategy_params: StrategyParams,
}

/// Run a single bot using shared services
pub async fn run_bot_with_services(
    config: BotRunConfig,
    services: Arc<SharedServices>,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
) {
    let mut state = BotState::new();
    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(config.manager, config.auth_id);
    
    // Get initial signer balance
    if let Ok(balance) = services.client.rpc.get_balance(&config.signer.pubkey()) {
        let _ = tui_tx.send(TuiUpdate::BotSignerBalanceUpdate {
            bot_index: config.bot_index,
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
            bot_index: config.bot_index,
            rounds_participated: 0,
            rounds_won: 0,
            current_claimable_sol: miner.rewards_sol,
            current_ore: miner.rewards_ore,
        });
        
        let _ = tui_tx.send(TuiUpdate::BotMinerUpdate {
            bot_index: config.bot_index,
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
                    bot_index: config.bot_index,
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

        // State machine logic
        match determine_phase(&board, current_slot, &state, config.slots_left) {
            BotPhase::Idle => {
                state.set_phase(BotPhase::Idle);
                send_status(&tui_tx, config.bot_index, BotStatus::Idle);
                sleep(Duration::from_millis(500)).await;
            }
            
            BotPhase::Checkpointing => {
                state.set_phase(BotPhase::Checkpointing);
                send_status(&tui_tx, config.bot_index, BotStatus::Checkpointing);
                
                if let Some(last_round) = state.last_deployed_round {
                    // Store pre-checkpoint values
                    if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
                        state.store_pre_checkpoint(miner.rewards_sol, miner.rewards_ore);
                    }
                    
                    // Send checkpoint
                    let bh = wait_for_blockhash(&services.blockhash_cache).await;
                    let checkpoint_tx = build_checkpoint_tx(
                        &config.signer,
                        &config.manager,
                        config.auth_id,
                        last_round,
                        bh,
                    );
                    
                    match services.client.rpc.send_and_confirm_transaction(&checkpoint_tx) {
                        Ok(sig) => {
                            send_tx_event(&tui_tx, &config.name, TxAction::Confirmed, sig, None);
                            
                            // Update state after checkpoint
                            sleep(Duration::from_millis(500)).await;
                            
                            // Get miner data and extract values before any await
                            let miner_data = services.client.get_miner(&managed_miner_auth).ok().flatten();
                            
                            if let Some(miner) = miner_data {
                                let rewards_sol = miner.rewards_sol;
                                let rewards_ore = miner.rewards_ore;
                                
                                state.process_checkpoint(last_round, rewards_sol, rewards_ore);
                                
                                let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                                    bot_index: config.bot_index,
                                    rounds_participated: state.rounds_participated,
                                    rounds_won: state.rounds_won,
                                    current_claimable_sol: state.current_claimable_sol,
                                    current_ore: state.current_ore,
                                });
                                
                                let _ = tui_tx.send(TuiUpdate::BotMinerUpdate {
                                    bot_index: config.bot_index,
                                    miner: miner.clone(),
                                });
                                
                                // Claim if rewards available
                                if rewards_sol > 0 {
                                    state.set_phase(BotPhase::Claiming);
                                    let bh = wait_for_blockhash(&services.blockhash_cache).await;
                                    let claim_tx = build_claim_sol_tx(
                                        &config.signer,
                                        &config.manager,
                                        config.auth_id,
                                        bh,
                                    );
                                    
                                    if let Ok(sig) = services.client.rpc.send_and_confirm_transaction(&claim_tx) {
                                        send_tx_event(&tui_tx, &config.name, TxAction::Confirmed, sig, None);
                                        update_signer_balance(&services, &config, &tui_tx).await;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let error_msg = format!("Checkpoint: {}", e);
                            send_tx_event(&tui_tx, &config.name, TxAction::Failed, Signature::default(), Some(error_msg));
                            sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
            }
            
            BotPhase::Waiting => {
                state.set_phase(BotPhase::Waiting);
                send_status(&tui_tx, config.bot_index, BotStatus::Waiting);
                sleep(Duration::from_millis(50)).await;
            }
            
            BotPhase::Deploying => {
                state.set_phase(BotPhase::Deploying);
                send_status(&tui_tx, config.bot_index, BotStatus::Deploying);
                
                // Build deploy params based on strategy
                let params = build_ev_params(&config);
                
                // Spam deploy transactions
                let mut signatures = Vec::new();
                let send_interval_ms = if config.slots_left > 10 { 0 } else if config.slots_left >= 5 { 400 } else { 100 };
                
                loop {
                    let current = services.slot_tracker.get_slot();
                    if current >= board.end_slot {
                        break;
                    }
                    
                    let bh = services.blockhash_cache.get_blockhash();
                    if bh == Hash::default() {
                        sleep(Duration::from_millis(10)).await;
                        continue;
                    }
                    
                    let deploy_tx = build_ev_deploy_tx(
                        &config.signer,
                        &config.manager,
                        config.auth_id,
                        board.round_id,
                        &params,
                        bh,
                    );
                    
                    match services.client.send_transaction_no_wait(&deploy_tx) {
                        Ok(sig) => {
                            signatures.push(sig);
                            send_tx_event(&tui_tx, &config.name, TxAction::Sent, sig, None);
                        }
                        Err(e) => {
                            send_tx_event(&tui_tx, &config.name, TxAction::Failed, Signature::default(), Some(e.to_string()));
                        }
                    }
                    
                    if send_interval_ms == 0 {
                        break;
                    }
                    sleep(Duration::from_millis(send_interval_ms)).await;
                }
                
                // Check confirmations
                if !signatures.is_empty() {
                    sleep(Duration::from_secs(3)).await;
                    
                    let mut any_confirmed = false;
                    for sig in &signatures {
                        if services.client.confirm_transaction(sig).unwrap_or(false) {
                            any_confirmed = true;
                            send_tx_event(&tui_tx, &config.name, TxAction::Confirmed, *sig, None);
                        }
                    }
                    
                    if any_confirmed {
                        // Get deployed amount from miner
                        if let Ok(Some(miner)) = services.client.get_miner(&managed_miner_auth) {
                            let deployed: u64 = miner.deployed.iter().sum();
                            state.record_deployment(board.round_id, deployed);
                            
                            let _ = tui_tx.send(TuiUpdate::BotDeployedUpdate {
                                bot_index: config.bot_index,
                                amount: deployed,
                                round_id: board.round_id,
                            });
                            
                            let _ = tui_tx.send(TuiUpdate::BotStatsUpdate {
                                bot_index: config.bot_index,
                                rounds_participated: state.rounds_participated,
                                rounds_won: state.rounds_won,
                                current_claimable_sol: state.current_claimable_sol,
                                current_ore: state.current_ore,
                            });
                        }
                        
                        update_signer_balance(&services, &config, &tui_tx).await;
                    }
                }
            }
            
            BotPhase::Deployed => {
                state.set_phase(BotPhase::Deployed);
                send_status(&tui_tx, config.bot_index, BotStatus::Deployed);
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

/// Send transaction event
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

/// Update signer balance after transaction
async fn update_signer_balance(
    services: &SharedServices,
    config: &BotRunConfig,
    tx: &mpsc::UnboundedSender<TuiUpdate>,
) {
    if let Ok(balance) = services.client.rpc.get_balance(&config.signer.pubkey()) {
        let _ = tx.send(TuiUpdate::BotSignerBalanceUpdate {
            bot_index: config.bot_index,
            balance,
        });
    }
}

/// Build EV deploy params from config
fn build_ev_params(config: &BotRunConfig) -> EvDeployParams {
    match &config.strategy_params {
        StrategyParams::EV { max_per_square, min_bet, ore_value } => {
            EvDeployParams {
                bankroll: 0, // Will be calculated from miner balance
                max_per_square: *max_per_square,
                min_bet: *min_bet,
                ore_value: *ore_value,
                slots_left: config.slots_left,
            }
        }
        // TODO: Handle other strategies
        _ => EvDeployParams {
            bankroll: 0,
            max_per_square: 100_000_000,
            min_bet: 10_000,
            ore_value: 800_000_000,
            slots_left: config.slots_left,
        }
    }
}
