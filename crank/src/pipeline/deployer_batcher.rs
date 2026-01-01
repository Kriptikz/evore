//! Deployer Batcher System
//!
//! Batches deploy transactions (up to 7 miners per transaction or 5 second timeout).
//! Uses mm_full_autodeploy with LUTs for efficient transaction packing.

use std::sync::Arc;
use std::time::Duration;

use evore::instruction::mm_full_autodeploy;
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    signature::{Keypair, Signer},
};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::{debug, error, info, warn};

use crate::lut::get_miner_auth_pda;

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::{BatchedTx, MinerTask, TxType};
use super::AUTH_ID;

/// Maximum miners per deploy transaction
const MAX_BATCH_SIZE: usize = 7;

/// Timeout for batching (wait for more miners before sending)
const BATCH_TIMEOUT: Duration = Duration::from_secs(5);

/// Deploy amount per square in lamports (2,800 × 25 squares = 70,000 total)
const DEPLOY_AMOUNT: u64 = 2_800;

/// Deploy to all squares (bitmask with all 25 bits set)
const SQUARES_MASK: u32 = 0x1FFFFFF;

/// Run the deployer batcher system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
    rpc_client: Arc<RpcClient>,
    deploy_authority: Arc<Keypair>,
    priority_fee: u64,
) {
    info!("[DeployerBatcher] Starting...");

    let mut batch: Vec<MinerTask> = Vec::with_capacity(MAX_BATCH_SIZE);
    let mut total_batched = 0u64;
    let mut total_miners = 0u64;

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
                        let batch_size = batch.len();
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
                        total_miners += batch_size as u64;
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
                    let batch_size = batch.len();
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
                    total_miners += batch_size as u64;
                }
            }
            Err(_) => {
                // Channel closed, process remaining batch
                if !batch.is_empty() {
                    let batch_size = batch.len();
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
                    total_miners += batch_size as u64;
                }
                break;
            }
        }
    }

    info!(
        "[DeployerBatcher] Shutting down. Total batches: {}, Total miners: {}",
        total_batched, total_miners
    );
}

/// Process a batch of deploy miners
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
    let round_id = batch.first().map(|t| t.round_id).unwrap_or(0);

    info!(
        "[DeployerBatcher] Processing batch of {} deploys for round {}",
        batch_size, round_id
    );

    // Get checkpoint rounds for miners that need it
    let checkpoint_rounds: Vec<Option<u64>> = {
        let cache = shared.miner_cache.read().await;
        batch
            .iter()
            .map(|task| {
                let miner = cache.get(&task.miner_address);
                miner.and_then(|m| {
                    if m.checkpoint_id < m.round_id {
                        Some(m.round_id)
                    } else {
                        None
                    }
                })
            })
            .collect()
    };

    // Collect miner_auths for LUT lookup
    let miner_auths: Vec<_> = batch
        .iter()
        .map(|t| get_miner_auth_pda(t.manager(), AUTH_ID))
        .collect();

    // Get LUTs
    let lut_accounts = {
        let lut_cache = shared.lut_cache.read().await;
        lut_cache.get_luts_for_miners(&miner_auths)
    };

    if lut_accounts.is_empty() {
        error!(
            "[DeployerBatcher] No LUTs found for batch, cannot proceed"
        );
        return;
    }

    // Get recent blockhash
    let (recent_blockhash, _) = match rpc_client
        .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
    {
        Ok(bh) => bh,
        Err(e) => {
            error!(
                "[DeployerBatcher] Failed to get blockhash: {}. Retrying miners.",
                e
            );
            // Send miners back for retry
            for task in batch {
                if task.can_retry() {
                    let _ = senders.to_deployment_check.send(task.with_retry()).await;
                }
            }
            return;
        }
    };

    // Build instructions
    let mut instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(1_400_000),
        ComputeBudgetInstruction::set_compute_unit_price(priority_fee),
    ];

    // Add mm_full_autodeploy instruction for each miner
    for (task, checkpoint_round) in batch.iter().zip(checkpoint_rounds.iter()) {
        // checkpoint_round_id: if checkpoint needed, use that round; otherwise use current round
        let checkpoint_round_id = checkpoint_round.unwrap_or(round_id);

        instructions.push(mm_full_autodeploy(
            deploy_authority.pubkey(),
            task.manager(),
            AUTH_ID,
            round_id,
            checkpoint_round_id,
            DEPLOY_AMOUNT,
            SQUARES_MASK,
        ));
    }

    // Build versioned transaction with LUTs
    let tx = {
        let lut_cache = shared.lut_cache.read().await;
        match lut_cache.build_versioned_tx(
            deploy_authority,
            instructions,
            lut_accounts,
            recent_blockhash,
        ) {
            Ok(tx) => tx,
            Err(e) => {
                error!(
                    "[DeployerBatcher] Failed to build transaction: {}",
                    e
                );
                return;
            }
        }
    };

    // Log transaction size
    let tx_bytes = bincode::serialize(&tx).unwrap_or_default();
    debug!(
        "[DeployerBatcher] Built versioned tx: {} bytes (limit 1232)",
        tx_bytes.len()
    );

    // Create batched transaction
    let batched_tx = BatchedTx::new(tx, batch, TxType::Deploy, round_id);

    // Send to transaction processor
    if let Err(e) = senders.to_tx_processor.send(batched_tx).await {
        error!(
            "[DeployerBatcher] Failed to send to tx processor: {}",
            e
        );
    }

    shared.stats.increment(&shared.stats.deploys_sent);
    info!(
        "[DeployerBatcher] Sent batch of {} deploys to tx processor",
        batch_size
    );
}

