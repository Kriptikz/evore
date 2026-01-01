//! Deployment Check System
//!
//! 3 parallel workers that check deployment eligibility:
//! - Sufficient SOL balance
//! - Enough slots remaining (>= 20)
//! - Not already deployed this round
//!
//! Routes miners to:
//! - DeployerBatcher (pass all checks) - checkpoint is bundled with deploy via mm_full_autodeploy
//! - CheckpointBatcher (can't deploy this round but has unchecked rounds from previous deploys)
//! - Skip/log (other failures, or no action needed)

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::{RoundPhase, SharedState};
use super::types::MinerTask;

/// Minimum balance required for deployment (in lamports)
/// This should cover rent + fees for the deploy transaction
const MIN_DEPLOY_BALANCE: u64 = 10_000_000; // 0.01 SOL

/// Run the deployment check system with multiple workers
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    rx: mpsc::Receiver<MinerTask>,
    num_workers: usize,
) {
    info!(
        "[DeploymentCheck] Starting with {} workers...",
        num_workers
    );

    // Create a shared receiver using Arc<Mutex>
    let rx = Arc::new(tokio::sync::Mutex::new(rx));

    // Spawn workers
    let mut handles = Vec::new();
    for worker_id in 1..=num_workers {
        let shared = shared.clone();
        let senders = senders.clone();
        let rx = rx.clone();

        let handle = tokio::spawn(async move {
            run_worker(shared, senders, rx, worker_id).await;
        });
        handles.push(handle);
    }

    // Wait for all workers to complete
    for handle in handles {
        let _ = handle.await;
    }

    info!("[DeploymentCheck] All workers shut down");
}

/// Run a single deployment check worker
async fn run_worker(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    rx: Arc<tokio::sync::Mutex<mpsc::Receiver<MinerTask>>>,
    worker_id: usize,
) {
    let prefix = format!("[DeploymentCheck:{}]", worker_id);
    info!("{} Starting worker", prefix);

    let mut deploy_count = 0u64;
    let mut checkpoint_count = 0u64;
    let mut skipped_count = 0u64;

    loop {
        // Try to receive a task
        let task = {
            let mut rx = rx.lock().await;
            rx.recv().await
        };

        let task = match task {
            Some(t) => t,
            None => break, // Channel closed
        };

        let start = Instant::now();

        // Get cached miner data
        let miner_data = {
            let cache = shared.miner_cache.read().await;
            cache.get(&task.miner_address).cloned()
        };

        let miner = match miner_data {
            Some(m) => m,
            None => {
                warn!(
                    "{} No cached data for miner {}, skipping",
                    prefix, task.miner_address
                );
                skipped_count += 1;
                continue;
            }
        };

        // Get current board state
        let (can_deploy, phase, current_round_id) = {
            let state = shared.board_state.read().await;
            (state.can_deploy(), state.phase, state.round_id)
        };

        // Check 1: Is the round still open for deployments?
        if !can_deploy {
            warn!(
                "{} SKIPPED no_slots | manager: {} | miner: {} | auth: {} | phase: {}",
                prefix, task.manager(), task.miner_address, task.miner_auth, phase
            );
            shared.stats.increment(&shared.stats.miners_skipped_no_slots);
            skipped_count += 1;
            continue;
        }

        // Check 2: Has miner already deployed this round?
        if miner.round_id == current_round_id && miner.has_deployed {
            debug!(
                "{} {} - already deployed this round",
                prefix, task.manager()
            );
            shared
                .stats
                .increment(&shared.stats.miners_skipped_already_deployed);
            skipped_count += 1;
            continue;
        }

        // Check 3: Does miner have retry limit exceeded?
        if !task.can_retry() && task.retry_count > 0 {
            warn!(
                "{} SKIPPED max_retries | manager: {} | miner: {} | auth: {} | retries: {}",
                prefix, task.manager(), task.miner_address, task.miner_auth, task.retry_count
            );
            skipped_count += 1;
            continue;
        }

        // Check 4: Sufficient balance?
        let balance = miner.auth_balance;
        let has_sufficient_balance = balance >= MIN_DEPLOY_BALANCE;

        // Check 5: Needs checkpoint from previous rounds?
        // (checkpoint_id tracks last checkpointed round, round_id is last deployed round)
        let needs_checkpoint = miner.checkpoint_id < miner.round_id;

        // Route based on checks
        if has_sufficient_balance {
            // Can deploy - checkpoint (if needed) will be bundled with deploy via mm_full_autodeploy
            debug!(
                "{} {} - ready to deploy (balance: {} lamports, needs_checkpoint: {})",
                prefix, task.manager(), balance, needs_checkpoint
            );
            if let Err(e) = senders.to_deployer_batcher.send(task).await {
                warn!("{} Failed to send to deployer batcher: {}", prefix, e);
            }
            deploy_count += 1;
        } else if needs_checkpoint {
            // Can't deploy this round (insufficient balance) but has unchecked rounds
            // Do checkpoint-only to collect any pending rewards from previous deploys
            info!(
                "{} CHECKPOINT_ONLY | manager: {} | miner: {} | auth: {} | balance: {} < {} | checkpoint_id: {} < round_id: {}",
                prefix, task.manager(), task.miner_address, task.miner_auth, balance, MIN_DEPLOY_BALANCE, miner.checkpoint_id, miner.round_id
            );
            if let Err(e) = senders.to_checkpoint_batcher.send(task).await {
                warn!("{} Failed to send to checkpoint batcher: {}", prefix, e);
            }
            checkpoint_count += 1;
        } else {
            // Can't deploy and no checkpoint needed - nothing to do
            warn!(
                "{} SKIPPED low_balance | manager: {} | miner: {} | auth: {} | balance: {} < {}",
                prefix, task.manager(), task.miner_address, task.miner_auth, balance, MIN_DEPLOY_BALANCE
            );
            shared
                .stats
                .increment(&shared.stats.miners_skipped_low_balance);
            skipped_count += 1;
        }

        // Update stats
        let elapsed = start.elapsed().as_millis() as u64;
        shared
            .stats
            .add(&shared.stats.deployment_check_total_time_ms, elapsed);
        shared.stats.increment(&shared.stats.deployment_check_count);

        // Log progress periodically
        let total = deploy_count + checkpoint_count + skipped_count;
        if total % 20 == 0 && total > 0 {
            info!(
                "{} {} deploy, {} checkpoint, {} skipped",
                prefix, deploy_count, checkpoint_count, skipped_count
            );
        }
    }

    info!(
        "{} Shutting down. Final: {} deploy, {} checkpoint, {} skipped",
        prefix, deploy_count, checkpoint_count, skipped_count
    );
}

