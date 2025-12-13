//! Evore Autodeploy Crank
//!
//! Reference implementation for automated deploying via the Evore program.
//! This crank scans for deployer accounts where the configured wallet is the
//! deploy_authority and can execute autodeploy transactions.
//!
//! Users should customize the deployment strategy in run_strategy() based on
//! their specific requirements.

mod config;
mod crank;
mod db;
mod lut;
mod sender;

use clap::Parser;
use config::Config;
use lut::LutManager;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

// =============================================================================
// DEPLOYMENT STRATEGY - Customize these for your use case
// =============================================================================

/// Amount to deploy per square in lamports (0.0001 SOL = 100_000 lamports)
const DEPLOY_AMOUNT_LAMPORTS: u64 = 10_000;

/// Which auth_id to deploy for (each manager can have multiple managed miners)
const AUTH_ID: u64 = 0;

/// Squares mask - which squares to deploy to (0x1FFFFFF = all 25 squares)
const SQUARES_MASK: u32 = 0x1FFFFFF;

/// How many slots before round end to trigger deployment
const DEPLOY_SLOTS_BEFORE_END: u64 = 150;

/// Minimum slots remaining to attempt deployment (don't deploy too close to end)
const MIN_SLOTS_TO_DEPLOY: u64 = 10;

/// Maximum deployers to batch in one transaction without LUT
const MAX_BATCH_SIZE_NO_LUT: usize = 2;

/// Maximum deployers to batch in one transaction with LUT (checkpoint+recycle+deploy combined)
const MAX_BATCH_SIZE_WITH_LUT: usize = 5;

// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .init();
    
    // Load .env file if present
    dotenvy::dotenv().ok();
    
    // Parse configuration
    let config = Config::parse();
    
    info!("Evore Autodeploy Crank");
    info!("RPC URL: {}", config.rpc_url);
    
    // Initialize database
    let db_pool = db::init_db(&config.db_path).await?;
    
    // Create crank instance
    let crank = crank::Crank::new(config.clone(), db_pool).await?;
    info!("Deploy authority: {}", crank.deploy_authority_pubkey());
    
    // Initialize LUT manager
    let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
    
    // Handle subcommand
    match config.command {
        Some(config::Command::Test) => {
            info!("Running test transaction...");
            match crank.send_test_transaction().await {
                Ok(sig) => {
                    info!("✓ Test transaction successful: {}", sig);
                }
                Err(e) => {
                    error!("✗ Test transaction failed: {}", e);
                    return Err(e.into());
                }
            }
            return Ok(());
        }
        Some(config::Command::List) => {
            info!("Finding deployers...");
            let deployers = crank.find_deployers().await?;
            
            if deployers.is_empty() {
                warn!("No deployers found where we are the deploy_authority");
                warn!("Create a deployer with deploy_authority set to: {}", crank.deploy_authority_pubkey());
            } else {
                info!("Managing {} deployers:", deployers.len());
                for d in &deployers {
                    let balance = crank.get_autodeploy_balance(d).unwrap_or(0);
                    info!("  Manager: {}", d.manager_address);
                    info!("    Deployer: {}", d.deployer_address);
                    info!("    Fee: {} bps", d.fee_bps);
                    info!("    Balance: {} lamports ({:.6} SOL)", balance, balance as f64 / 1_000_000_000.0);
                }
            }
            return Ok(());
        }
        Some(config::Command::CreateLut) => {
            info!("Creating new Address Lookup Table...");
            match crank.create_lut(&mut lut_manager).await {
                Ok(lut_address) => {
                    info!("✓ LUT created: {}", lut_address);
                    info!("Add to .env: LUT_ADDRESS={}", lut_address);
                }
                Err(e) => {
                    error!("✗ Failed to create LUT: {}", e);
                    return Err(e.into());
                }
            }
            return Ok(());
        }
        Some(config::Command::ExtendLut) => {
            let lut_address = config.lut_address.ok_or("LUT_ADDRESS not set in .env")?;
            lut_manager.load_lut(lut_address)?;
            
            info!("Finding deployers to add to LUT...");
            let deployers = crank.find_deployers().await?;
            
            if deployers.is_empty() {
                warn!("No deployers found");
                return Ok(());
            }
            
            // Get current board for round_id
            let (board, _) = crank.get_board()?;
            
            match crank.extend_lut_with_deployers(&mut lut_manager, &deployers, AUTH_ID, board.round_id).await {
                Ok(count) => {
                    if count > 0 {
                        info!("✓ Added {} addresses to LUT", count);
                    } else {
                        info!("LUT already contains all deployer addresses");
                    }
                }
                Err(e) => {
                    error!("✗ Failed to extend LUT: {}", e);
                    return Err(e.into());
                }
            }
            return Ok(());
        }
        Some(config::Command::ShowLut) => {
            let lut_address = config.lut_address.ok_or("LUT_ADDRESS not set in .env")?;
            let lut_account = lut_manager.load_lut(lut_address)?;
            
            info!("LUT Address: {}", lut_address);
            info!("Contains {} addresses:", lut_account.addresses.len());
            for (i, addr) in lut_account.addresses.iter().enumerate() {
                info!("  [{}] {}", i, addr);
            }
            return Ok(());
        }
        Some(config::Command::Run) | None => {
            // Continue to main loop
        }
    }
    
    info!("Database: {}", config.db_path.display());
    info!("Priority fee: {} microlamports/CU", config.priority_fee);
    info!("Jito tip: {} lamports", config.jito_tip);
    
    // Load LUT if configured
    let lut_manager = if let Some(lut_address) = config.lut_address {
        match lut_manager.load_lut(lut_address) {
            Ok(_) => {
                info!("Using LUT: {} (enables batching up to 10 deploys/tx)", lut_address);
                Some(Arc::new(RwLock::new(lut_manager)))
            }
            Err(e) => {
                warn!("Failed to load LUT {}: {}. Running without LUT.", lut_address, e);
                None
            }
        }
    } else {
        info!("No LUT configured. Run 'create-lut' to create one for better batching.");
        None
    };
    
    // Find deployers we manage
    let deployers = crank.find_deployers().await?;
    
    if deployers.is_empty() {
        warn!("No deployers found where we are the deploy_authority");
        warn!("Create a deployer with deploy_authority set to: {}", crank.deploy_authority_pubkey());
        return Ok(());
    }
    
    info!("Managing {} deployers", deployers.len());
    for d in &deployers {
        info!("  - Manager: {} (fee: {} bps)", d.manager_address, d.fee_bps);
    }
    
    // Main loop
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    info!("Starting main loop (poll interval: {}ms)", config.poll_interval_ms);
    info!("Strategy: deploy {} lamports/square, {} squares, {} slots before end",
        DEPLOY_AMOUNT_LAMPORTS, SQUARES_MASK.count_ones(), DEPLOY_SLOTS_BEFORE_END);
    
    let mut last_round_id: Option<u64> = None;
    // Track which (deployer, round) pairs have already been deployed
    let mut deployed_rounds: HashSet<(Pubkey, u64)> = HashSet::new();
    
    loop {
        // Check pending transactions first
        if let Err(e) = crank.check_pending_txs().await {
            error!("Error checking pending txs: {}", e);
        }
        
        // Run the deployment strategy
        if let Err(e) = run_strategy(&crank, &deployers, &mut last_round_id, &mut deployed_rounds, &lut_manager).await {
            error!("Strategy error: {}", e);
        }
        
        tokio::time::sleep(poll_interval).await;
    }
}

/// Deployment strategy - customize this for your use case
async fn run_strategy(
    crank: &crank::Crank,
    deployers: &[config::DeployerInfo],
    last_round_id: &mut Option<u64>,
    deployed_rounds: &mut HashSet<(Pubkey, u64)>,
    lut_manager: &Option<Arc<RwLock<LutManager>>>,
) -> Result<(), crank::CrankError> {
    // Get current board state
    let (board, current_slot) = crank.get_board()?;
    
    // Don't deploy if round hasn't fully started (end_slot is u64::MAX during reset)
    if board.end_slot == u64::MAX {
        return Ok(());
    }
    
    let slots_remaining = board.end_slot.saturating_sub(current_slot);
    
    // Check if this is a new round
    let is_new_round = last_round_id.map_or(true, |id| id != board.round_id);
    if is_new_round {
        info!("New round detected: {} (ends in {} slots)", board.round_id, slots_remaining);
        *last_round_id = Some(board.round_id);
        
        // Clean up old entries from deployed_rounds (keep only current round)
        deployed_rounds.retain(|(_, round_id)| *round_id == board.round_id);
    }
    
    // Don't deploy if too close to round end (transaction won't land in time)
    if slots_remaining < MIN_SLOTS_TO_DEPLOY {
        return Ok(());
    }
    
    // Only deploy when close to round end
    if slots_remaining > DEPLOY_SLOTS_BEFORE_END {
        return Ok(());
    }
    
    // Collect deployers for deployment (with checkpoint info)
    let mut to_deploy: Vec<(&config::DeployerInfo, u64, u64, u64, u32, Option<u64>)> = Vec::new();
    
    for deployer in deployers {
        let deploy_key = (deployer.deployer_address, board.round_id);
        
        // Skip if already deployed this round
        if deployed_rounds.contains(&deploy_key) {
            info!("Skipping {}: already deployed this round", deployer.manager_address);
            continue;
        }
        
        // Check if checkpoint is needed
        let checkpoint_round = crank.needs_checkpoint(deployer, AUTH_ID)?;
        
        // Calculate required balance for this deploy
        let required = crank.calculate_required_balance_with_state(
            deployer,
            AUTH_ID,
            DEPLOY_AMOUNT_LAMPORTS, 
            SQUARES_MASK, 
        )?;
        
        // Check balance
        let balance = crank.get_autodeploy_balance(deployer)?;
        
        if balance >= required {
            info!(
                "Adding {} to deploy batch: balance {} >= required {} lamports{}",
                deployer.manager_address, balance, required,
                if checkpoint_round.is_some() { format!(" (will checkpoint round {})", checkpoint_round.unwrap()) } else { "".to_string() }
            );
            to_deploy.push((deployer, AUTH_ID, board.round_id, DEPLOY_AMOUNT_LAMPORTS, SQUARES_MASK, checkpoint_round));
        } else if checkpoint_round.is_some() {
            // Not enough to deploy but needs checkpoint - do checkpoint only
            info!("Manager {} needs checkpoint but insufficient balance for deploy", deployer.manager_address);
            match crank.execute_checkpoint_recycle(deployer, AUTH_ID, checkpoint_round.unwrap()).await {
                Ok(sig) => info!("✓ Checkpoint+recycle for {}: {}", deployer.manager_address, sig),
                Err(e) => error!("✗ Checkpoint+recycle failed for {}: {}", deployer.manager_address, e),
            }
        } else {
            warn!(
                "Skipping {}: insufficient balance ({} < {} lamports)",
                deployer.manager_address, balance, required
            );
        }
    }
    
    // Execute deploys in batches
    if !to_deploy.is_empty() {
        info!("Deploying for {} managers (round {})", to_deploy.len(), board.round_id);
        
        // Determine batch size based on LUT availability
        let batch_size = if lut_manager.is_some() {
            MAX_BATCH_SIZE_WITH_LUT
        } else {
            MAX_BATCH_SIZE_NO_LUT
        };
        
        for batch in to_deploy.chunks(batch_size) {
            let deployer_keys: Vec<_> = batch.iter().map(|(d, _, _, _, _, _)| d.deployer_address).collect();
            
            // Use versioned transactions with LUT if available (combines checkpoint+recycle+deploy)
            if let Some(lut_mgr) = lut_manager {
                let lut = lut_mgr.read().await;
                // Keep checkpoint info - versioned tx includes checkpoint+recycle+deploy
                let batch_vec: Vec<_> = batch.to_vec();
                let checkpoints_in_batch = batch.iter().filter(|(_, _, _, _, _, cp)| cp.is_some()).count();
                match crank.execute_batched_autodeploys_versioned(&lut, batch_vec).await {
                    Ok(sig) => {
                        info!("✓ Versioned autodeploy ({} deployers, {} checkpoints, with LUT): {}", batch.len(), checkpoints_in_batch, sig);
                        for key in deployer_keys {
                            deployed_rounds.insert((key, board.round_id));
                        }
                    }
                    Err(e) => error!("✗ Versioned autodeploy failed: {}", e),
                }
            } else if batch.len() == 1 {
                let (deployer, auth_id, round_id, amount, mask, _) = batch[0];
                match crank.execute_autodeploy(deployer, auth_id, round_id, amount, mask).await {
                    Ok(sig) => {
                        info!("✓ Autodeploy for {}: {}", deployer.manager_address, sig);
                        deployed_rounds.insert((deployer.deployer_address, board.round_id));
                    }
                    Err(e) => error!("✗ Autodeploy failed for {}: {}", deployer.manager_address, e),
                }
            } else {
                let batch_vec: Vec<_> = batch.to_vec();
                match crank.execute_batched_autodeploys(batch_vec).await {
                    Ok(sig) => {
                        info!("✓ Batched autodeploy ({} deployers): {}", batch.len(), sig);
                        for key in deployer_keys {
                            deployed_rounds.insert((key, board.round_id));
                        }
                    }
                    Err(e) => error!("✗ Batched autodeploy failed: {}", e),
                }
            }
        }
    }
    
    Ok(())
}
