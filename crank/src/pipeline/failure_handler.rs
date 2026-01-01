//! Failure Handler System
//!
//! Handles failed transaction batches by:
//! 1. Attempting to identify which miner caused the failure
//! 2. Refreshing the problematic miner's cache data
//! 3. Sending the problematic miner back to fee_check (fresh start)
//! 4. Sending other miners in the batch directly to deployment_check (fast retry)

use std::sync::Arc;

use solana_client::rpc_client::RpcClient;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::{FailedBatch, TxType};

/// Known error patterns and which instruction index they typically affect
/// These are based on common Solana program errors
const ERROR_PATTERNS: &[(&str, &str)] = &[
    ("insufficient funds", "balance"),
    ("InsufficientAutodeployBalance", "balance"),
    ("0x1", "insufficient funds"),
    ("already deployed", "deployed"),
    ("EndSlotReached", "round ended"),
    ("custom program error: 0x", "program error"),
];

/// Run the failure handler system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<FailedBatch>,
    rpc_client: Arc<RpcClient>,
) {
    info!("[FailureHandler] Starting...");

    let mut handled_count = 0u64;
    let mut refreshed_count = 0u64;
    let mut fast_retry_count = 0u64;

    while let Some(failed_batch) = rx.recv().await {
        handled_count += 1;
        let batch_size = failed_batch.miners.len();
        
        info!(
            "[FailureHandler] Handling failed {} batch: {} ({} miners) | error: {:?}",
            failed_batch.tx_type, failed_batch.signature, batch_size, failed_batch.error
        );

        // Try to identify which miner caused the failure
        let problematic_index = identify_problematic_miner(&failed_batch);
        
        match problematic_index {
            Some(idx) if batch_size > 1 => {
                // We identified a specific miner as problematic
                let problematic_miner = &failed_batch.miners[idx];
                warn!(
                    "[FailureHandler] Identified problematic miner at index {}: {} (manager: {})",
                    idx, problematic_miner.miner_address, problematic_miner.manager()
                );

                // Refresh the problematic miner's cache
                {
                    let mut cache = shared.miner_cache.write().await;
                    match cache.refresh_single(&rpc_client, &problematic_miner.miner_address) {
                        Ok(Some(updated)) => {
                            info!(
                                "[FailureHandler] Refreshed problematic miner {} | balance: {} | deployed: {}",
                                problematic_miner.miner_address, updated.auth_balance, updated.has_deployed
                            );
                            refreshed_count += 1;
                        }
                        Ok(None) => {
                            warn!("[FailureHandler] Miner not in cache: {}", problematic_miner.miner_address);
                        }
                        Err(e) => {
                            error!("[FailureHandler] Failed to refresh miner: {}", e);
                        }
                    }
                }

                // Send problematic miner back to fee_check (fresh start)
                if problematic_miner.can_retry() {
                    let retry_task = problematic_miner.clone().with_retry();
                    debug!(
                        "[FailureHandler] Sending problematic miner {} to fee_check (retry #{})",
                        retry_task.miner_address, retry_task.retry_count
                    );
                    if let Err(e) = senders.to_fee_check.send(retry_task).await {
                        error!("[FailureHandler] Failed to send to fee_check: {}", e);
                    }
                } else {
                    warn!(
                        "[FailureHandler] Problematic miner {} exceeded max retries",
                        problematic_miner.miner_address
                    );
                }

                // Send other miners directly to deployment_check (fast retry)
                for (i, miner) in failed_batch.miners.into_iter().enumerate() {
                    if i == idx {
                        continue; // Skip the problematic one
                    }
                    debug!(
                        "[FailureHandler] Fast-retry miner {} to deployment_check",
                        miner.miner_address
                    );
                    if let Err(e) = senders.to_deployment_check.send(miner).await {
                        error!("[FailureHandler] Failed to send to deployment_check: {}", e);
                    }
                    fast_retry_count += 1;
                }
            }
            _ => {
                // Cannot identify specific problematic miner, or batch size is 1
                // Refresh all miners and send them all to fee_check
                info!(
                    "[FailureHandler] Cannot identify specific problematic miner, refreshing all {} miners",
                    batch_size
                );

                for miner in failed_batch.miners {
                    // Refresh each miner's cache
                    {
                        let mut cache = shared.miner_cache.write().await;
                        if let Err(e) = cache.refresh_single(&rpc_client, &miner.miner_address) {
                            error!("[FailureHandler] Failed to refresh miner {}: {}", miner.miner_address, e);
                        } else {
                            refreshed_count += 1;
                        }
                    }

                    // Send to fee_check with retry increment
                    if miner.can_retry() {
                        let retry_task = miner.with_retry();
                        if let Err(e) = senders.to_fee_check.send(retry_task).await {
                            error!("[FailureHandler] Failed to send to fee_check: {}", e);
                        }
                    } else {
                        warn!(
                            "[FailureHandler] Miner {} exceeded max retries",
                            miner.miner_address
                        );
                    }
                }
            }
        }

        // Log summary periodically
        if handled_count % 5 == 0 {
            info!(
                "[FailureHandler] Handled: {} batches | Refreshed: {} miners | Fast retries: {}",
                handled_count, refreshed_count, fast_retry_count
            );
        }
    }

    info!(
        "[FailureHandler] Shutting down. Total: {} batches handled, {} miners refreshed, {} fast retries",
        handled_count, refreshed_count, fast_retry_count
    );
}

/// Try to identify which miner in the batch caused the failure
/// Returns the index of the problematic miner if we can determine it
fn identify_problematic_miner(failed_batch: &FailedBatch) -> Option<usize> {
    let error_msg = failed_batch.error.as_ref()?;
    let error_lower = error_msg.to_lowercase();

    // Look for instruction index in error message
    // Solana errors often include "instruction X" or "InstructionError(X, ...)"
    if let Some(idx) = extract_instruction_index(&error_lower) {
        // For deploy batches, each miner typically gets multiple instructions
        // Try to map instruction index to miner index
        let miners_count = failed_batch.miners.len();
        
        match failed_batch.tx_type {
            TxType::Deploy => {
                // Deploy transactions have compute budget (2 ixs) + deploy ix per miner
                // So instruction 2 = miner 0, instruction 3 = miner 1, etc.
                if idx >= 2 {
                    let miner_idx = (idx - 2) as usize;
                    if miner_idx < miners_count {
                        return Some(miner_idx);
                    }
                }
            }
            TxType::Checkpoint => {
                // Similar structure for checkpoints
                if idx >= 2 {
                    let miner_idx = (idx - 2) as usize;
                    if miner_idx < miners_count {
                        return Some(miner_idx);
                    }
                }
            }
            TxType::FeeUpdate => {
                // Fee updates batch differently
                if idx >= 2 {
                    let miner_idx = (idx - 2) as usize;
                    if miner_idx < miners_count {
                        return Some(miner_idx);
                    }
                }
            }
        }
    }

    // Check for error patterns that might indicate a specific type of failure
    for (pattern, _category) in ERROR_PATTERNS {
        if error_lower.contains(pattern) {
            // For balance errors, the first miner in the batch is often the culprit
            // (since they're processed in order)
            return Some(0);
        }
    }

    // Default: cannot identify specific miner
    None
}

/// Extract instruction index from error message
fn extract_instruction_index(error_msg: &str) -> Option<u32> {
    // Pattern 1: "InstructionError(X, ...)"
    if let Some(start) = error_msg.find("instructionerror(") {
        let rest = &error_msg[start + 17..];
        if let Some(end) = rest.find(',') {
            if let Ok(idx) = rest[..end].trim().parse::<u32>() {
                return Some(idx);
            }
        }
    }

    // Pattern 2: "instruction X failed"
    if let Some(start) = error_msg.find("instruction ") {
        let rest = &error_msg[start + 12..];
        let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if num_end > 0 {
            if let Ok(idx) = rest[..num_end].parse::<u32>() {
                return Some(idx);
            }
        }
    }

    // Pattern 3: "Error processing Instruction X:"
    if let Some(start) = error_msg.find("error processing instruction ") {
        let rest = &error_msg[start + 29..];
        let num_end = rest.find(':').unwrap_or(rest.len());
        if let Ok(idx) = rest[..num_end].trim().parse::<u32>() {
            return Some(idx);
        }
    }

    None
}

