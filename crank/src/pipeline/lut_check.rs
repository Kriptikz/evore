//! LUT Check System
//!
//! Checks if a miner has a LUT in the cache.
//! - Fast path: LUT in cache → send to DeploymentCheck
//! - Slow path: LUT not in cache → send to LUTCreation

use std::sync::Arc;
use std::time::Instant;

use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use crate::lut::get_miner_auth_pda;

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::MinerTask;
use super::AUTH_ID;

/// Run the LUT check system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
) {
    info!("[LUTCheck] Starting...");

    let mut cached_count = 0u64;
    let mut creation_count = 0u64;
    let mut total_time_ms = 0u64;

    while let Some(task) = rx.recv().await {
        let start = Instant::now();

        // Get the miner_auth PDA for this manager
        let miner_auth = get_miner_auth_pda(task.manager(), AUTH_ID);

        // Check if LUT exists in cache
        let has_lut = {
            let lut_cache = shared.lut_cache.read().await;
            lut_cache.has_miner_lut(&miner_auth)
        };

        let elapsed = start.elapsed().as_millis() as u64;
        total_time_ms += elapsed;

        if has_lut {
            // Fast path: LUT in cache, send to deployment check
            debug!(
                "[LUTCheck] {} has LUT in cache, sending to deployment check",
                task.manager()
            );
            if let Err(e) = senders.to_deployment_check.send(task).await {
                warn!("[LUTCheck] Failed to send to deployment check: {}", e);
            }
            cached_count += 1;
        } else {
            // Slow path: need to create LUT
            debug!(
                "[LUTCheck] {} needs LUT creation",
                task.manager()
            );
            if let Err(e) = senders.to_lut_creation.send(task).await {
                warn!("[LUTCheck] Failed to send to LUT creation: {}", e);
            }
            creation_count += 1;
        }

        // Update stats
        shared
            .stats
            .add(&shared.stats.lut_check_total_time_ms, elapsed);
        shared.stats.increment(&shared.stats.lut_check_count);

        // Log progress periodically
        let total = cached_count + creation_count;
        if total % 50 == 0 && total > 0 {
            let avg_time = total_time_ms as f64 / total as f64;
            info!(
                "[LUTCheck] Processed {} miners in {}ms ({} cached, {} to creation, avg {:.2}ms)",
                total, total_time_ms, cached_count, creation_count, avg_time
            );
        }
    }

    let total = cached_count + creation_count;
    let avg_time = if total > 0 {
        total_time_ms as f64 / total as f64
    } else {
        0.0
    };
    info!(
        "[LUTCheck] Shutting down. Final: {} cached, {} to creation (avg {:.2}ms)",
        cached_count, creation_count, avg_time
    );
}

