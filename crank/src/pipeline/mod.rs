//! Pipeline architecture for the crank
//!
//! This module implements a modular pipeline where miners flow through
//! multiple stages via tokio channels. Each stage is a separate async task.
//!
//! Pipeline flow:
//! ```text
//! MinerCacheUpdate
//!     → FeeCheck (single worker)
//!         → [flat_fee wrong] → Log & Skip
//!         → [expected_fee wrong] → ExpectedFeeUpdater → TxProcessor → ...
//!         → [fees OK] → LUTCheck
//!             → [cached] → DeploymentCheck (3 workers)
//!             → [not cached] → LUTCreation → DeploymentCheck
//!                 → [pass] → DeployerBatcher
//!                 → [needs checkpoint] → CheckpointBatcher
//!                 → [fail] → Log & Skip
//! ```

pub mod board_state_monitor;
pub mod channels;
pub mod checkpoint_batcher;
pub mod confirmation;
pub mod deployer_batcher;
pub mod deployment_check;
pub mod expected_fee_updater;
pub mod failure_handler;
pub mod fee_check;
pub mod lut_check;
pub mod lut_creation;
pub mod shared_state;
pub mod tx_processor;
pub mod tx_sender;
pub mod types;

use std::sync::Arc;

use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::{Keypair, Signer};
use tokio::sync::mpsc;
use tracing::{error, info};

use crate::config::{Config, DeployerInfo};
use crate::crank::CrankError;

pub use channels::{ChannelSenders, PipelineChannels};
pub use shared_state::{BoardState, PipelineStats, RoundPhase, SharedState};
pub use types::{BatchedTx, MinerTask, PendingConfirmation, SignedTx, TxType};

/// Required flat fee in lamports that users must agree to
pub const REQUIRED_FLAT_FEE: u64 = 715;

/// Auth ID used for managed miners (always 0 for now)
pub const AUTH_ID: u64 = 0;

/// Run the pipeline
pub async fn run_pipeline(
    config: Config,
    rpc_client: Arc<RpcClient>,
    deploy_authority: Arc<Keypair>,
) -> Result<(), CrankError> {
    info!("Starting pipeline architecture...");

    // Initialize shared state
    let shared = Arc::new(SharedState::new(
        &config.rpc_url,
        deploy_authority.pubkey(),
    ));

    // Create channels
    let mut channels = PipelineChannels::new();
    let senders = ChannelSenders::from_channels(&channels);

    // Create sender for sending work from main loop
    let main_sender = senders.clone();

    // Take receivers out of channels struct for the systems
    let fee_check_rx = std::mem::replace(
        &mut channels.from_fee_check,
        mpsc::channel(1).1,
    );
    let expected_fee_updater_rx = std::mem::replace(
        &mut channels.from_expected_fee_updater,
        mpsc::channel(1).1,
    );
    let lut_check_rx = std::mem::replace(
        &mut channels.from_lut_check,
        mpsc::channel(1).1,
    );
    let lut_creation_rx = std::mem::replace(
        &mut channels.from_lut_creation,
        mpsc::channel(1).1,
    );
    let deployment_check_rx = std::mem::replace(
        &mut channels.from_deployment_check,
        mpsc::channel(1).1,
    );
    let checkpoint_batcher_rx = std::mem::replace(
        &mut channels.from_checkpoint_batcher,
        mpsc::channel(1).1,
    );
    let deployer_batcher_rx = std::mem::replace(
        &mut channels.from_deployer_batcher,
        mpsc::channel(1).1,
    );
    let tx_processor_rx = std::mem::replace(
        &mut channels.from_tx_processor,
        mpsc::channel(1).1,
    );
    let tx_sender_rx = std::mem::replace(
        &mut channels.from_tx_sender,
        mpsc::channel(1).1,
    );
    let confirmation_rx = std::mem::replace(
        &mut channels.from_confirmation,
        mpsc::channel(1).1,
    );
    let failure_handler_rx = std::mem::replace(
        &mut channels.from_failure_handler,
        mpsc::channel(1).1,
    );

    // Spawn all systems as tokio tasks
    let handles = vec![
        // Board state monitor (background)
        tokio::spawn(board_state_monitor::run(
            shared.clone(),
            senders.clone(),
            rpc_client.clone(),
            config.poll_interval_ms,
        )),
        // Fee Check (pipeline entry point, single worker)
        tokio::spawn(fee_check::run(
            shared.clone(),
            senders.clone(),
            fee_check_rx,
        )),
        // Expected Fee Updater (batches fee updates)
        tokio::spawn(expected_fee_updater::run(
            shared.clone(),
            senders.clone(),
            expected_fee_updater_rx,
            rpc_client.clone(),
            deploy_authority.clone(),
            config.priority_fee,
        )),
        // LUT Check
        tokio::spawn(lut_check::run(
            shared.clone(),
            senders.clone(),
            lut_check_rx,
        )),
        // LUT Creation
        tokio::spawn(lut_creation::run(
            shared.clone(),
            senders.clone(),
            lut_creation_rx,
            rpc_client.clone(),
            deploy_authority.clone(),
        )),
        // Deployment Check - 3 parallel workers
        tokio::spawn(deployment_check::run(
            shared.clone(),
            senders.clone(),
            deployment_check_rx,
            1, // number of workers
        )),
        // Checkpoint Batcher
        tokio::spawn(checkpoint_batcher::run(
            shared.clone(),
            senders.clone(),
            checkpoint_batcher_rx,
            rpc_client.clone(),
            deploy_authority.clone(),
            config.priority_fee,
        )),
        // Deployer Batcher
        tokio::spawn(deployer_batcher::run(
            shared.clone(),
            senders.clone(),
            deployer_batcher_rx,
            rpc_client.clone(),
            deploy_authority.clone(),
            config.priority_fee,
        )),
        // Transaction Processor
        tokio::spawn(tx_processor::run(
            shared.clone(),
            senders.clone(),
            tx_processor_rx,
            deploy_authority.clone(),
        )),
        // Transaction Sender
        tokio::spawn(tx_sender::run(
            shared.clone(),
            senders.clone(),
            tx_sender_rx,
            config.rpc_url.clone(),
        )),
        // Confirmation System
        tokio::spawn(confirmation::run(
            shared.clone(),
            senders.clone(),
            confirmation_rx,
            config.rpc_url.clone(),
        )),
        // Failure Handler (processes failed batches)
        tokio::spawn(failure_handler::run(
            shared.clone(),
            senders.clone(),
            failure_handler_rx,
            rpc_client.clone(),
        )),
    ];

    info!("All pipeline systems spawned");

    // Main loop: detect rounds, trigger discovery + cache update
    let mut round_changed_rx = senders.round_changed.subscribe();
    let mut last_round_id: Option<u64> = None;

    loop {
        // Wait for round change notification from board state monitor
        match round_changed_rx.recv().await {
            Ok(new_round_id) => {
                // Skip if same round
                if last_round_id == Some(new_round_id) {
                    continue;
                }

                info!("New round detected: {}", new_round_id);
                last_round_id = Some(new_round_id);

                // Reset stats for new round
                shared.stats.reset();

                // Discover deployers
                let deployers = match discover_deployers(&rpc_client, &deploy_authority).await {
                    Ok(d) => d,
                    Err(e) => {
                        error!("Failed to discover deployers: {}", e);
                        continue;
                    }
                };

                if deployers.is_empty() {
                    info!("No deployers found");
                    continue;
                }

                info!("Found {} deployers", deployers.len());

                // Update miner cache
                {
                    let mut cache = shared.miner_cache.write().await;
                    if let Err(e) = cache.refresh(&rpc_client, &deployers, AUTH_ID, new_round_id) {
                        error!("Failed to refresh miner cache: {}", e);
                        continue;
                    }
                }

                // Load LUTs
                {
                    let mut lut_cache = shared.lut_cache.write().await;
                    if let Err(e) = lut_cache.load_all_luts() {
                        error!("Failed to load LUTs: {}", e);
                        // Continue anyway - we can create LUTs as needed
                    }
                }

                // Send all miners into pipeline (entry point: FeeCheck)
                // Record pipeline start time before sending first miner
                shared.stats.record_pipeline_start();

                let cache = shared.miner_cache.read().await;
                let mut sent_count = 0u64;
                for cached_miner in cache.all_miners() {
                    // Find the deployer info for this miner
                    let deployer = match deployers
                        .iter()
                        .find(|d| d.deployer_address == cached_miner.deployer_address)
                    {
                        Some(d) => d.clone(),
                        None => continue,
                    };

                    let task = MinerTask::new(
                        deployer,
                        cached_miner.miner_address,
                        cached_miner.authority,
                        new_round_id,
                    );

                    if let Err(e) = main_sender.to_fee_check.send(task).await {
                        error!("Failed to send miner to fee check: {}", e);
                    } else {
                        sent_count += 1;
                    }
                }

                // Record how many miners were sent
                shared.stats.add(&shared.stats.miners_sent_to_pipeline, sent_count);
                info!("Sent {} miners to pipeline", sent_count);
            }
            Err(e) => {
                error!("Round change receiver error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        }
    }
}

/// Discover all deployers we have authority over
async fn discover_deployers(
    rpc_client: &RpcClient,
    deploy_authority: &Keypair,
) -> Result<Vec<DeployerInfo>, CrankError> {
    use evore::state::Deployer;
    use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
    use solana_client::rpc_filter::{Memcmp, RpcFilterType};
    use solana_account_decoder::UiAccountEncoding;
    use steel::AccountDeserialize;

    let deploy_authority_pubkey = deploy_authority.pubkey();

    // Deployer size: 8 discriminator + 32 manager_key + 32 deploy_authority + 8 bps_fee + 8 flat_fee + 8 expected_bps_fee + 8 expected_flat_fee + 8 max_per_round = 112
    const DEPLOYER_SIZE: u64 = 112;

    info!(
        "Scanning for deployers with deploy_authority: {} (data_size={})",
        deploy_authority_pubkey, DEPLOYER_SIZE
    );

    // Use getProgramAccounts with optimized filters
    let accounts = rpc_client
        .get_program_accounts_with_config(
            &evore::id(),
            RpcProgramAccountsConfig {
                filters: Some(vec![
                    // Filter by data size first (most efficient filter)
                    RpcFilterType::DataSize(DEPLOYER_SIZE),
                    // Filter by account discriminator (Deployer = 101)
                    RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                        0,
                        &[101, 0, 0, 0, 0, 0, 0, 0], // EvoreAccount::Deployer discriminator
                    )),
                    // Filter by deploy_authority (offset: 8 discriminator + 32 manager_key = 40)
                    RpcFilterType::Memcmp(Memcmp::new_base58_encoded(
                        40,
                        deploy_authority_pubkey.as_ref(),
                    )),
                ]),
                account_config: RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        )
        .map_err(|e| CrankError::Rpc(e.to_string()))?;

    info!("GPA returned {} deployer accounts", accounts.len());

    let mut deployers = Vec::new();

    for (deployer_address, account) in accounts {
        match Deployer::try_from_bytes(&account.data) {
            Ok(deployer) => {
                deployers.push(DeployerInfo {
                    deployer_address,
                    manager_address: deployer.manager_key,
                    bps_fee: deployer.bps_fee,
                    flat_fee: deployer.flat_fee,
                    expected_bps_fee: deployer.expected_bps_fee,
                    expected_flat_fee: deployer.expected_flat_fee,
                    max_per_round: deployer.max_per_round,
                });
            }
            Err(e) => {
                error!("Failed to parse deployer {}: {:?}", deployer_address, e);
            }
        }
    }

    Ok(deployers)
}

