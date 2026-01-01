//! Board State Monitor System
//!
//! Runs continuously in background, polling the board account and current slot.
//! Updates shared BoardState and signals round changes.

use std::sync::Arc;

use evore::ore_api::{board_pda, round_pda, Board};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use steel::AccountDeserialize;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::{RoundPhase, SharedState};

/// Run the board state monitor
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    rpc_client: Arc<RpcClient>,
    poll_interval_ms: u64,
) {
    info!("[BoardStateMonitor] Starting...");

    let mut interval = interval(Duration::from_millis(poll_interval_ms));
    let mut last_round_id: Option<u64> = None;
    let mut last_phase: Option<RoundPhase> = None;

    loop {
        interval.tick().await;

        // Fetch board state
        match fetch_board_state(&rpc_client).await {
            Ok((board, current_slot)) => {
                let round_id = board.round_id;
                let (round_address, _) = round_pda(round_id);

                // Update shared state
                {
                    let mut state = shared.board_state.write().await;
                    state.round_id = round_id;
                    state.round_address = round_address;
                    state.start_slot = board.start_slot;
                    state.end_slot = board.end_slot;
                    state.current_slot = current_slot;
                    state.update_phase();

                    let new_phase = state.phase;

                    // Log phase transitions
                    if let Some(old_phase) = last_phase {
                        if std::mem::discriminant(&old_phase) != std::mem::discriminant(&new_phase)
                        {
                            info!(
                                "[BoardStateMonitor] Phase transition: {} -> {}",
                                old_phase, new_phase
                            );

                            // Log round stats when entering intermission (round ended)
                            let was_in_round = matches!(
                                old_phase,
                                RoundPhase::DeploymentWindow { .. } | RoundPhase::LateDeploymentWindow { .. }
                            );
                            let now_in_intermission = matches!(new_phase, RoundPhase::Intermission { .. });

                            if was_in_round && now_in_intermission {
                                info!("[BoardStateMonitor] ========== ROUND {} ENDED ==========", round_id);
                                shared.stats.log_summary(round_id, &new_phase);
                                info!("[BoardStateMonitor] =====================================");
                            }
                        }
                    }

                    last_phase = Some(new_phase);

                    // Log periodic status
                    match new_phase {
                        RoundPhase::DeploymentWindow { slots_remaining } => {
                            debug!(
                                "[BoardStateMonitor] Round {} phase: DeploymentWindow ({} slots remaining)",
                                round_id, slots_remaining
                            );
                        }
                        RoundPhase::WaitingForFirstDeploy => {
                            debug!(
                                "[BoardStateMonitor] Round {} phase: WaitingForFirstDeploy (ready to deploy)",
                                round_id
                            );
                        }
                        _ => {}
                    }
                }

                // Signal round change when round_id changes (reset occurred)
                // At this point end_slot is u64::MAX, but we start updates immediately
                // so our miners can be the first deployers
                if last_round_id != Some(round_id) {
                    info!(
                        "[BoardStateMonitor] New round detected: {} (triggering updates + deployments)",
                        round_id
                    );
                    last_round_id = Some(round_id);

                    // Broadcast round change - this triggers deployer discovery and miner cache update
                    if let Err(e) = senders.round_changed.send(round_id) {
                        warn!("[BoardStateMonitor] Failed to broadcast round change: {}", e);
                    }
                }
            }
            Err(e) => {
                error!("[BoardStateMonitor] Failed to fetch board state: {}", e);
            }
        }
    }
}

/// Fetch current board state and slot from the chain
async fn fetch_board_state(rpc_client: &RpcClient) -> Result<(Board, u64), String> {
    // Get board account
    let (board_address, _) = board_pda();
    let board_account = rpc_client
        .get_account(&board_address)
        .map_err(|e| format!("Failed to get board account: {}", e))?;

    let board = Board::try_from_bytes(&board_account.data)
        .map_err(|e| format!("Failed to parse board: {:?}", e))?;

    // Get current slot
    let current_slot = rpc_client
        .get_slot()
        .map_err(|e| format!("Failed to get slot: {}", e))?;

    Ok((*board, current_slot))
}

