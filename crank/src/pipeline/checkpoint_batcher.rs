//! Checkpoint Batcher System
//!
//! Batches checkpoint transactions (up to 5 miners per transaction or 5 second timeout).
//! Includes recycle_sol for miners that have SOL to recycle.

use std::sync::Arc;
use std::time::Duration;

use evore::instruction::{mm_autocheckpoint, recycle_sol};
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
use super::AUTH_ID;

/// Maximum miners per checkpoint transaction
const MAX_BATCH_SIZE: usize = 5;

/// Timeout for batching (wait for more miners before sending)
const BATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Run the checkpoint batcher system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
    rpc_client: Arc<RpcClient>,
    deploy_authority: Arc<Keypair>,
    priority_fee: u64,
) {
    info!("[CheckpointBatcher] Starting...");

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
        "[CheckpointBatcher] Shutting down. Total batches: {}",
        total_batched
    );
}

/// Process a batch of checkpoint miners
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
        "[CheckpointBatcher] Processing batch of {} checkpoints",
        batch_size
    );

    // Get checkpoint rounds and check if we need to recycle
    let checkpoint_data: Vec<(u64, bool)> = {
        let cache = shared.miner_cache.read().await;
        batch
            .iter()
            .map(|task| {
                let miner = cache.get(&task.miner_address);
                let checkpoint_round = miner.map(|m| m.round_id).unwrap_or(0);
                let has_sol_to_recycle = miner.map(|m| m.rewards_sol > 0).unwrap_or(false);
                (checkpoint_round, has_sol_to_recycle)
            })
            .collect()
    };

    // Build instructions
    // ~150k CU per checkpoint + recycle
    let cu_per_checkpoint = 150_000u32;
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(cu_per_checkpoint * batch_size as u32),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
    ];

    for (task, (checkpoint_round, has_sol_to_recycle)) in batch.iter().zip(checkpoint_data.iter()) {
        // Checkpoint instruction
        instructions.push(mm_autocheckpoint(
            deploy_authority.pubkey(),
            task.manager(),
            *checkpoint_round,
            AUTH_ID,
        ));

        // Only include recycle if there's SOL to recycle
        if *has_sol_to_recycle {
            instructions.push(recycle_sol(
                deploy_authority.pubkey(),
                task.manager(),
                AUTH_ID,
            ));
        }
    }

    // Get recent blockhash
    let recent_blockhash = match rpc_client.get_latest_blockhash() {
        Ok(bh) => bh,
        Err(e) => {
            error!(
                "[CheckpointBatcher] Failed to get blockhash: {}. Dropping batch.",
                e
            );
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
                "[CheckpointBatcher] Failed to convert transaction: {}",
                e
            );
            return;
        }
    };

    // Get round_id from first task
    let round_id = batch.first().map(|t| t.round_id).unwrap_or(0);

    // Create batched transaction
    let batched_tx = BatchedTx::new(versioned_tx, batch, TxType::Checkpoint, round_id);

    // Send to transaction processor
    if let Err(e) = senders.to_tx_processor.send(batched_tx).await {
        error!(
            "[CheckpointBatcher] Failed to send to tx processor: {}",
            e
        );
    }

    shared.stats.increment(&shared.stats.checkpoints_sent);
    info!(
        "[CheckpointBatcher] Sent batch of {} checkpoints to tx processor",
        batch_size
    );
}

