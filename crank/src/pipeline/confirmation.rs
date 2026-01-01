//! Confirmation System
//!
//! Tracks pending transactions and confirms them in batches.
//! - Polls get_signature_statuses() with batches of up to 200 signatures
//! - Routes confirmed/failed transactions appropriately
//! - Handles timeouts and retries

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use solana_sdk::signature::Signature;
use tokio::sync::mpsc;
use tokio::time::interval;
use tracing::{debug, error, info, warn};

use crate::sender::TxSender;

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::{FailedBatch, MinerTask, PendingConfirmation, TxType};

/// Maximum signatures per batch check
const MAX_BATCH_SIZE: usize = 200;

/// How often to check signatures (rate limiting)
const CHECK_INTERVAL: Duration = Duration::from_millis(400);

/// Timeout for confirmations (in seconds)
const CONFIRMATION_TIMEOUT_SECS: u64 = 60;

/// Run the confirmation system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<PendingConfirmation>,
    rpc_url: String,
) {
    info!("[Confirmation] Starting...");

    let sender = TxSender::new(rpc_url);
    let mut pending: HashMap<Signature, PendingConfirmation> = HashMap::new();
    let mut check_interval = interval(CHECK_INTERVAL);

    // Stats
    let mut confirmed_deploy = 0u64;
    let mut confirmed_checkpoint = 0u64;
    let mut confirmed_fee_update = 0u64;
    let mut failed_count = 0u64;
    let mut timeout_count = 0u64;

    loop {
        tokio::select! {
            // Receive new pending confirmations
            Some(confirmation) = rx.recv() => {
                debug!(
                    "[Confirmation] Tracking {} txn: {}",
                    confirmation.tx_type, confirmation.signature
                );
                pending.insert(confirmation.signature, confirmation);
            }

            // Periodic batch check
            _ = check_interval.tick() => {
                if pending.is_empty() {
                    continue;
                }

                // Check for timeouts first
                let now = Instant::now();
                let timed_out: Vec<Signature> = pending
                    .iter()
                    .filter(|(_, p)| {
                        now.duration_since(p.sent_at).as_secs() > CONFIRMATION_TIMEOUT_SECS
                    })
                    .map(|(sig, _)| *sig)
                    .collect();

                for sig in timed_out {
                    if let Some(confirmation) = pending.remove(&sig) {
                        let miner_count = confirmation.miners.len() as u64;
                        warn!(
                            "[Confirmation] {} txn timed out: {} ({} miners)",
                            confirmation.tx_type, sig, miner_count
                        );
                        timeout_count += 1;

                        // Update stats
                        match confirmation.tx_type {
                            TxType::Deploy => {
                                shared.stats.increment(&shared.stats.deploys_failed);
                                shared.stats.add(&shared.stats.miners_deploy_failed, miner_count);
                            }
                            TxType::Checkpoint => {
                                shared.stats.increment(&shared.stats.checkpoints_failed);
                                shared.stats.add(&shared.stats.miners_checkpoint_failed, miner_count);
                            }
                            TxType::FeeUpdate => {
                                shared.stats.increment(&shared.stats.fee_updates_failed);
                            }
                        }

                        // Send to failure handler for intelligent retry
                        let failed_batch = FailedBatch {
                            miners: confirmation.miners,
                            signature: sig,
                            tx_type: confirmation.tx_type,
                            round_id: confirmation.round_id,
                            error: Some("Timeout".to_string()),
                        };
                        if let Err(e) = senders.to_failure_handler.send(failed_batch).await {
                            error!("[Confirmation] Failed to send timeout to failure handler: {}", e);
                        }
                    }
                }

                // Batch check remaining signatures
                let signatures: Vec<Signature> = pending.keys().cloned().collect();

                for chunk in signatures.chunks(MAX_BATCH_SIZE) {
                    let start = Instant::now();

                    match sender.get_signature_statuses(chunk).await {
                        Ok(statuses) => {
                            for (sig, status) in chunk.iter().zip(statuses.iter()) {
                                match status {
                                    Some(true) => {
                                        // Confirmed!
                                        if let Some(confirmation) = pending.remove(sig) {
                                            let elapsed = confirmation.sent_at.elapsed().as_millis() as u64;

                                            info!(
                                                "[Confirmation] {} txn confirmed: {} ({}ms)",
                                                confirmation.tx_type, sig, elapsed
                                            );

                                            // Update stats
                                            let miner_count = confirmation.miners.len() as u64;
                                            match confirmation.tx_type {
                                                TxType::Deploy => {
                                                    shared.stats.increment(&shared.stats.deploys_confirmed);
                                                    shared.stats.add(&shared.stats.deploy_total_time_ms, elapsed);
                                                    shared.stats.increment(&shared.stats.deploy_count_for_avg);
                                                    shared.stats.add(&shared.stats.miners_deployed, miner_count);
                                                    confirmed_deploy += 1;

                                                    // Record deploy confirmed time for round total timing
                                                    shared.stats.record_deploy_confirmed();

                                                    // Mark miners as deployed in cache
                                                    let miner_addresses: Vec<_> = confirmation
                                                        .miners
                                                        .iter()
                                                        .map(|t| t.miner_address)
                                                        .collect();
                                                    let mut cache = shared.miner_cache.write().await;
                                                    cache.mark_deployed(&miner_addresses, confirmation.round_id);
                                                }
                                                TxType::Checkpoint => {
                                                    shared.stats.increment(&shared.stats.checkpoints_confirmed);
                                                    shared.stats.add(&shared.stats.checkpoint_total_time_ms, elapsed);
                                                    shared.stats.increment(&shared.stats.checkpoint_count_for_avg);
                                                    shared.stats.add(&shared.stats.miners_checkpointed, miner_count);
                                                    confirmed_checkpoint += 1;
                                                }
                                                TxType::FeeUpdate => {
                                                    shared.stats.increment(&shared.stats.fee_updates_confirmed);
                                                    shared.stats.add(&shared.stats.fee_update_total_time_ms, elapsed);
                                                    shared.stats.increment(&shared.stats.fee_update_count_for_avg);
                                                    confirmed_fee_update += 1;

                                                    // Send miners to deployment check to continue pipeline
                                                    for miner in confirmation.miners {
                                                        if let Err(e) = senders.to_deployment_check.send(miner).await {
                                                            warn!("[Confirmation] Failed to send miner to deployment check: {}", e);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Some(false) => {
                                        // Failed!
                                        if let Some(confirmation) = pending.remove(sig) {
                                            let miner_count = confirmation.miners.len() as u64;
                                            error!(
                                                "[Confirmation] {} txn failed: {} ({} miners)",
                                                confirmation.tx_type, sig, miner_count
                                            );
                                            failed_count += 1;

                                            // Update stats
                                            match confirmation.tx_type {
                                                TxType::Deploy => {
                                                    shared.stats.increment(&shared.stats.deploys_failed);
                                                    shared.stats.add(&shared.stats.miners_deploy_failed, miner_count);
                                                }
                                                TxType::Checkpoint => {
                                                    shared.stats.increment(&shared.stats.checkpoints_failed);
                                                    shared.stats.add(&shared.stats.miners_checkpoint_failed, miner_count);
                                                }
                                                TxType::FeeUpdate => {
                                                    shared.stats.increment(&shared.stats.fee_updates_failed);
                                                }
                                            }

                                            // Send to failure handler for intelligent retry
                                            let failed_batch = FailedBatch {
                                                miners: confirmation.miners,
                                                signature: *sig,
                                                tx_type: confirmation.tx_type,
                                                round_id: confirmation.round_id,
                                                error: None, // We don't have error details from signature status
                                            };
                                            if let Err(e) = senders.to_failure_handler.send(failed_batch).await {
                                                error!("[Confirmation] Failed to send to failure handler: {}", e);
                                            }
                                        }
                                    }
                                    None => {
                                        // Still pending
                                        if let Some(confirmation) = pending.get_mut(sig) {
                                            confirmation.check_count += 1;
                                        }
                                    }
                                }
                            }

                            // Update timing stats
                            let elapsed = start.elapsed().as_millis() as u64;
                            shared.stats.add(&shared.stats.confirmation_batch_total_time_ms, elapsed);
                            shared.stats.increment(&shared.stats.confirmation_batch_count);
                        }
                        Err(e) => {
                            warn!(
                                "[Confirmation] Failed to check signatures: {}",
                                e
                            );
                        }
                    }
                }

                // Log summary periodically
                let total_confirmed = confirmed_deploy + confirmed_checkpoint + confirmed_fee_update;
                if total_confirmed > 0 && total_confirmed % 10 == 0 {
                    info!(
                        "[Confirmation] Confirmed: Deploy({}) Checkpoint({}) FeeUpdate({}) | Failed: {} | Pending: {}",
                        confirmed_deploy, confirmed_checkpoint, confirmed_fee_update, failed_count, pending.len()
                    );
                }
            }
        }
    }
}

