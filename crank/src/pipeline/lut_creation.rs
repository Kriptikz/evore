//! LUT Creation System
//!
//! Creates LUTs for miners that don't have one in the cache.
//! - First checks if LUT exists on-chain (add to cache if yes)
//! - Creates LUT if doesn't exist
//! - Sends to DeploymentCheck after LUT is ready

use std::sync::Arc;
use std::time::Duration;

use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    signature::{Keypair, Signer},
};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::lut::{get_miner_accounts, get_miner_auth_pda, LutRegistry};
use crate::sender::TxSender;

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::MinerTask;
use super::AUTH_ID;

/// Run the LUT creation system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<MinerTask>,
    rpc_client: Arc<RpcClient>,
    deploy_authority: Arc<Keypair>,
) {
    info!("[LUTCreation] Starting...");

    let sender = TxSender::new(rpc_client.url());
    let mut created_count = 0u64;
    let mut found_count = 0u64;
    let mut failed_count = 0u64;

    while let Some(task) = rx.recv().await {
        let miner_auth = get_miner_auth_pda(task.manager(), AUTH_ID);

        info!(
            "[LUTCreation] Processing {} (miner_auth: {})",
            task.manager(),
            miner_auth
        );

        // Try to ensure LUT exists
        match ensure_miner_lut(
            &shared,
            &rpc_client,
            &deploy_authority,
            &sender,
            &task,
            miner_auth,
        )
        .await
        {
            Ok(was_created) => {
                if was_created {
                    created_count += 1;
                    info!(
                        "[LUTCreation] Created new LUT for {} (total created: {})",
                        task.manager(),
                        created_count
                    );
                } else {
                    found_count += 1;
                    debug!(
                        "[LUTCreation] Found existing LUT for {}",
                        task.manager()
                    );
                }

                // Send to deployment check
                if let Err(e) = senders.to_deployment_check.send(task).await {
                    warn!(
                        "[LUTCreation] Failed to send to deployment check: {}",
                        e
                    );
                }
            }
            Err(e) => {
                error!(
                    "[LUTCreation] Failed to create LUT for {}: {}",
                    task.manager(),
                    e
                );
                failed_count += 1;

                // Retry if possible
                if task.can_retry() {
                    warn!(
                        "[LUTCreation] Retrying {} (attempt {})",
                        task.manager(),
                        task.retry_count + 1
                    );
                    if let Err(e) = senders.to_lut_creation.send(task.with_retry()).await {
                        error!("[LUTCreation] Failed to retry: {}", e);
                    }
                } else {
                    error!(
                        "[LUTCreation] Max retries exceeded for {}",
                        task.manager()
                    );
                }
            }
        }
    }

    info!(
        "[LUTCreation] Shutting down. Created: {}, Found: {}, Failed: {}",
        created_count, found_count, failed_count
    );
}

/// Ensure a miner has a LUT
/// Returns Ok(true) if a new LUT was created, Ok(false) if it already existed
async fn ensure_miner_lut(
    shared: &Arc<SharedState>,
    rpc_client: &RpcClient,
    deploy_authority: &Keypair,
    sender: &TxSender,
    task: &MinerTask,
    miner_auth: solana_sdk::pubkey::Pubkey,
) -> Result<bool, String> {
    // First, reload LUTs to check if one was created since we last checked
    {
        let mut lut_cache = shared.lut_cache.write().await;

        // Check if it exists now
        if lut_cache.has_miner_lut(&miner_auth) {
            return Ok(false);
        }

        // Try to reload from chain
        if let Err(e) = lut_cache.load_all_luts() {
            warn!("[LUTCreation] Failed to reload LUTs: {}", e);
        }

        // Check again after reload
        if lut_cache.has_miner_lut(&miner_auth) {
            return Ok(false);
        }
    }

    // Need to create a new LUT
    let lut_address = create_lut(rpc_client, deploy_authority, sender).await?;

    // Wait for LUT to be active
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Get miner accounts and extend LUT
    let miner_accounts = get_miner_accounts(task.manager(), AUTH_ID);
    extend_lut(rpc_client, deploy_authority, sender, lut_address, miner_accounts.clone()).await?;

    // Register in cache
    {
        let mut lut_cache = shared.lut_cache.write().await;
        lut_cache.register_miner_lut(miner_auth, lut_address, miner_accounts);
    }

    Ok(true)
}

/// Create a new LUT
async fn create_lut(
    rpc_client: &RpcClient,
    deploy_authority: &Keypair,
    sender: &TxSender,
) -> Result<solana_sdk::pubkey::Pubkey, String> {
    let recent_slot = rpc_client
        .get_slot()
        .map_err(|e| format!("Failed to get slot: {}", e))?;

    let (create_ix, lut_address) =
        solana_sdk::address_lookup_table::instruction::create_lookup_table(
            deploy_authority.pubkey(),
            deploy_authority.pubkey(),
            recent_slot,
        );

    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .map_err(|e| format!("Failed to get blockhash: {}", e))?;

    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(50_000),
        ComputeBudgetInstruction::set_compute_unit_price(100_000),
        create_ix,
    ];

    let tx = LutRegistry::build_versioned_tx_no_lut(deploy_authority, instructions, recent_blockhash)
        .map_err(|e| format!("Failed to build tx: {}", e))?;

    sender
        .send_and_confirm_versioned_rpc(&tx, 60)
        .await
        .map_err(|e| format!("Failed to send tx: {}", e))?;

    debug!("[LUTCreation] Created LUT: {}", lut_address);
    Ok(lut_address)
}

/// Extend a LUT with addresses
async fn extend_lut(
    rpc_client: &RpcClient,
    deploy_authority: &Keypair,
    sender: &TxSender,
    lut_address: solana_sdk::pubkey::Pubkey,
    addresses: Vec<solana_sdk::pubkey::Pubkey>,
) -> Result<(), String> {
    if addresses.is_empty() {
        return Ok(());
    }

    let extend_ix = solana_sdk::address_lookup_table::instruction::extend_lookup_table(
        lut_address,
        deploy_authority.pubkey(),
        Some(deploy_authority.pubkey()),
        addresses,
    );

    let recent_blockhash = rpc_client
        .get_latest_blockhash()
        .map_err(|e| format!("Failed to get blockhash: {}", e))?;

    let instructions = vec![
        ComputeBudgetInstruction::set_compute_unit_limit(100_000),
        ComputeBudgetInstruction::set_compute_unit_price(100_000),
        extend_ix,
    ];

    let tx = LutRegistry::build_versioned_tx_no_lut(deploy_authority, instructions, recent_blockhash)
        .map_err(|e| format!("Failed to build tx: {}", e))?;

    sender
        .send_and_confirm_versioned_rpc(&tx, 60)
        .await
        .map_err(|e| format!("Failed to send tx: {}", e))?;

    debug!("[LUTCreation] Extended LUT {} with {} addresses", lut_address, 5);
    Ok(())
}

