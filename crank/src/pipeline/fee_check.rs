//! Fee Check System - PIPELINE ENTRY POINT
//!
//! First system in the pipeline. All miners enter here.
//! Validates flat_fee and expected_flat_fee on deployer accounts.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::MinerTask;
use super::REQUIRED_FLAT_FEE;

/// Run the fee check system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
) {
    info!("[FeeCheck] Starting...");

    let mut ok_count = 0u64;
    let mut need_update_count = 0u64;
    let mut skipped_count = 0u64;

    while let Some(task) = rx.recv().await {
        let deployer = &task.deployer;

        // Check 1: flat_fee must be REQUIRED_FLAT_FEE (set by user)
        if deployer.flat_fee != REQUIRED_FLAT_FEE {
            warn!(
                "[FeeCheck] SKIPPED - wrong flat_fee | manager: {} | miner: {} | auth: {} | flat_fee: {} (required: {})",
                deployer.manager_address,
                task.miner_address,
                task.miner_auth,
                deployer.flat_fee,
                REQUIRED_FLAT_FEE
            );
            shared.stats.increment(&shared.stats.miners_skipped_wrong_fee);
            skipped_count += 1;
            continue;
        }

        // Check 2: expected_flat_fee must be REQUIRED_FLAT_FEE (set by deployer authority)
        if deployer.expected_flat_fee != REQUIRED_FLAT_FEE {
            debug!(
                "[FeeCheck] {} needs expected fee update: {} -> {}",
                deployer.manager_address, deployer.expected_flat_fee, REQUIRED_FLAT_FEE
            );
            // Send to expected fee updater
            if let Err(e) = senders.to_expected_fee_updater.send(task).await {
                warn!("[FeeCheck] Failed to send to expected fee updater: {}", e);
            }
            need_update_count += 1;
            continue;
        }

        // Both fees are correct, send to LUT check
        if let Err(e) = senders.to_lut_check.send(task).await {
            warn!("[FeeCheck] Failed to send to LUT check: {}", e);
        }
        ok_count += 1;

        // Log progress periodically
        let total = ok_count + need_update_count + skipped_count;
        if total % 50 == 0 {
            info!(
                "[FeeCheck] {} OK, {} need fee update, {} skipped (wrong flat_fee)",
                ok_count, need_update_count, skipped_count
            );
        }
    }

    info!(
        "[FeeCheck] Shutting down. Final: {} OK, {} need fee update, {} skipped",
        ok_count, need_update_count, skipped_count
    );
}
