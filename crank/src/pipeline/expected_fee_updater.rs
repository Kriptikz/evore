//! Fee Updater System
//!
//! Updates the actual fees (bps_fee, flat_fee) on deployer accounts.
//! As deploy_authority, we can set the actual fees charged.
//! Batches updates (up to 10 per transaction or 5 second timeout).
//! After confirmation, miners are sent to DeploymentCheck to continue the pipeline.

use std::sync::Arc;
use std::time::Duration;

use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::{BatchedTx, MinerTask, TxType};
use super::REQUIRED_FLAT_FEE;

/// Maximum miners per fee update transaction
const MAX_BATCH_SIZE: usize = 10;

/// Timeout for batching (wait for more miners before sending)
const BATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Run the expected fee updater system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
    rpc_client: Arc<RpcClient>,
    deploy_authority: Arc<Keypair>,
    priority_fee: u64,
) {
    info!("[FeeUpdater] Starting...");

    let mut batch: Vec<MinerTask> = Vec::with_capacity(MAX_BATCH_SIZE);
    let mut total_batched = 0u64;

    loop {
        // Try to receive with timeout
        let recv_result = if batch.is_empty() {
            // No batch started, wait indefinitely for first item
            rx.recv().await.ok_or(())
        } else {
            // Batch started, wait with timeout
            match timeout(BATCH_TIMEOUT, rx.recv()).await {
                Ok(Some(task)) => Ok(task),
                Ok(None) => Err(()), // Channel closed
                Err(_) => {
                    // Timeout - process current batch
                    if !batch.is_empty() {
                        process_batch(
                            &shared,
                            &senders,
                            &rpc_client,
                            &deploy_authority,
                            priority_fee,
                            std::mem::take(&mut batch),
                        )
                        .await;
                        total_batched += 1;
                    }
                    continue;
                }
            }
        };

        match recv_result {
            Ok(task) => {
                batch.push(task);

                // Process batch if full
                if batch.len() >= MAX_BATCH_SIZE {
                    process_batch(
                        &shared,
                        &senders,
                        &rpc_client,
                        &deploy_authority,
                        priority_fee,
                        std::mem::take(&mut batch),
                    )
                    .await;
                    total_batched += 1;
                }
            }
            Err(_) => {
                // Channel closed, process remaining batch
                if !batch.is_empty() {
                    process_batch(
                        &shared,
                        &senders,
                        &rpc_client,
                        &deploy_authority,
                        priority_fee,
                        std::mem::take(&mut batch),
                    )
                    .await;
                    total_batched += 1;
                }
                break;
            }
        }
    }

    info!(
        "[FeeUpdater] Shutting down. Total batches: {}",
        total_batched
    );
}

/// Process a batch of fee updates
async fn process_batch(
    shared: &Arc<SharedState>,
    senders: &ChannelSenders,
    rpc_client: &RpcClient,
    deploy_authority: &Keypair,
    priority_fee: u64,
    batch: Vec<MinerTask>,
) {
    if batch.is_empty() {
        return;
    }

    let batch_size = batch.len();
    info!(
        "[FeeUpdater] Processing batch of {} fee updates",
        batch_size
    );

    // Build instructions for each fee update
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(100_000 * batch_size as u32),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
    ];

    for task in &batch {
        let deployer = &task.deployer;

        // Build update_deployer instruction
        // As deploy_authority, we set the actual fees (bps_fee, flat_fee)
        // We set flat_fee to REQUIRED_FLAT_FEE, keep bps_fee at 0
        let ix = evore::instruction::update_deployer(
            deploy_authority.pubkey(),
            deployer.manager_address,
            deploy_authority.pubkey(), // Keep ourselves as deploy_authority
            0,                         // bps_fee (we set actual bps fee to 0)
            REQUIRED_FLAT_FEE,         // flat_fee (we set actual flat fee)
            deployer.expected_bps_fee, // Keep user's expected_bps_fee
            deployer.expected_flat_fee,// Keep user's expected_flat_fee
            deployer.max_per_round,    // Keep current max_per_round
        );
        instructions.push(ix);
    }

    // Get recent blockhash
    let recent_blockhash = match rpc_client.get_latest_blockhash() {
        Ok(bh) => bh,
        Err(e) => {
            error!(
                "[FeeUpdater] Failed to get blockhash: {}. Retrying miners.",
                e
            );
            // Send miners back to fee check for retry
            for task in batch {
                if task.can_retry() {
                    let _ = senders.to_fee_check.send(task.with_retry()).await;
                }
            }
            return;
        }
    };

    // Build transaction
    let mut tx = Transaction::new_with_payer(&instructions, Some(&deploy_authority.pubkey()));
    tx.sign(&[deploy_authority], recent_blockhash);

    // Convert to versioned transaction for the pipeline
    let versioned_tx = match solana_sdk::transaction::VersionedTransaction::try_from(tx) {
        Ok(vtx) => vtx,
        Err(e) => {
            error!(
                "[FeeUpdater] Failed to convert transaction: {}",
                e
            );
            return;
        }
    };

    // Get round_id from first task
    let round_id = batch.first().map(|t| t.round_id).unwrap_or(0);

    // Create batched transaction
    let batched_tx = BatchedTx::new(versioned_tx, batch, TxType::FeeUpdate, round_id);

    // Send to transaction processor
    if let Err(e) = senders.to_tx_processor.send(batched_tx).await {
        error!(
            "[FeeUpdater] Failed to send to tx processor: {}",
            e
        );
    }

    shared.stats.increment(&shared.stats.fee_updates_sent);
    info!(
        "[FeeUpdater] Sent batch of {} fee updates to tx processor",
        batch_size
    );
}

