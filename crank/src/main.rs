//! Evore Autodeploy Crank
//!
//! Reference implementation for automated deploying via the Evore program.
//! This crank scans for deployer accounts where the configured wallet is the
//! deploy_authority and can execute autodeploy transactions.
//!
//! LUT Architecture:
//! - One shared LUT for static accounts (9 accounts including deploy authority)
//! - One LUT per miner for their 6 specific accounts
//! - Round address is NOT in any LUT (changes each round)
//!
//! Transaction batching is limited by Solana's 64 instruction trace limit,
//! not transaction size. With checkpoint+recycle+deploy per miner, max ~5 deploys/tx.

mod config;
mod crank;
mod db;
mod lut;
mod miner_cache;
mod pipeline;
mod sender;

use clap::Parser;
use config::Config;
use lut::{LutManager, LutRegistry, get_miner_auth_pda};
use solana_sdk::signature::Signer;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

// =============================================================================
// DEPLOYMENT STRATEGY - Customize these for your use case
// =============================================================================

/// Amount to deploy per square in lamports (0.0001 SOL = 100_000 lamports)
const DEPLOY_AMOUNT_LAMPORTS: u64 = 2_800;

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

/// Maximum deployers to batch in one transaction with LUT
/// With consolidated LUTs (multiple miners per LUT):
/// - Account limit: 64 max, each deploy adds 7 accounts
/// - Base: ~10 shared + 2 rounds + compute budget = ~14 accounts
/// - (64 - 14) / 7 = 7.1, safe max is 7 deploys
const MAX_BATCH_SIZE: usize = 7;

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
            
            // Also load LUT registry to show LUT status
            let mut registry = LutRegistry::new(&config.rpc_url, crank.deploy_authority_pubkey());
            let _ = registry.load_all_luts();
            
            if deployers.is_empty() {
                warn!("No deployers found where we are the deploy_authority");
                warn!("Create a deployer with deploy_authority set to: {}", crank.deploy_authority_pubkey());
            } else {
                info!("Managing {} deployers:", deployers.len());
                for d in &deployers {
                    let balance = crank.get_miner_balance(d, AUTH_ID).unwrap_or(0);
                    let fee_str = if d.bps_fee == 0 {
                        format!("{} bps", d.bps_fee)
                    } else {
                        format!("{} lamports (flat)", d.flat_fee)
                    };
                    let miner_auth = get_miner_auth_pda(d.manager_address, AUTH_ID);
                    let has_lut = registry.has_miner_lut(&miner_auth);
                    
                    info!("  Manager: {}", d.manager_address);
                    info!("    Deployer: {}", d.deployer_address);
                    info!("    Fee: {}", fee_str);
                    info!("    Balance: {} lamports ({:.6} SOL)", balance, balance as f64 / 1_000_000_000.0);
                    info!("    Miner LUT: {}", if has_lut { "✓" } else { "✗ (will create on run)" });
                }
            }
            
            // Show shared LUT status
            if let Some(shared) = registry.shared_lut() {
                info!("Shared LUT: {}", shared);
            } else {
                info!("Shared LUT: Not found (will create on run)");
            }
            info!("Miner LUTs: {} found", registry.miner_luts().len());
            
            return Ok(());
        }
        Some(config::Command::SetExpectedFees { expected_bps_fee, expected_flat_fee }) => {
            info!("Setting expected fees for all deployers...");
            info!("Expected BPS fee: {} (0 = accept any)", expected_bps_fee);
            info!("Expected flat fee: {} lamports", expected_flat_fee);

            let deployers = crank.find_deployers().await?;
            if deployers.is_empty() {
                warn!("No deployers found where we are the deploy_authority");
                return Ok(());
            }

            info!("Checking {} deployers...", deployers.len());
            let mut updated = 0;
            let mut skipped = 0;
            for d in &deployers {
                match crank.update_expected_fees(&d, expected_bps_fee, expected_flat_fee).await {
                    Ok(Some(sig)) => {
                        info!("  ✓ Updated {}: {}", d.manager_address, sig);
                        updated += 1;
                    }
                    Ok(None) => {
                        info!("  - Skipped {} (already set)", d.manager_address);
                        skipped += 1;
                    }
                    Err(e) => {
                        error!("  ✗ Failed to update {}: {}", d.manager_address, e);
                    }
                }
            }
            
            info!("Done: {} updated, {} already set", updated, skipped);
            return Ok(());
        }
        Some(config::Command::CreateLut) => {
            info!("[LEGACY] Creating new Address Lookup Table...");
            info!("Note: 'run' command auto-creates LUTs. This is for manual management.");
            let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
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
            let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
            lut_manager.load_lut(lut_address)?;
            
            info!("Adding static shared accounts to LUT...");
            info!("Note: Round addresses are NOT added to LUT (they change each round and can't be removed)");
            
            // Add static shared accounts
            match crank.extend_lut_with_static_accounts(&mut lut_manager).await {
                Ok(count) => {
                    if count > 0 {
                        info!("✓ Added {} static shared addresses to LUT", count);
                    } else {
                        info!("LUT already contains all static shared addresses");
                    }
                }
                Err(e) => {
                    error!("✗ Failed to extend LUT with static accounts: {}", e);
                    return Err(e.into());
                }
            }
            
            // Show LUT info
            let lut_account = lut_manager.get_lut_account()?;
            info!("LUT now contains {} addresses (max 256)", lut_account.addresses.len());
            info!("Static shared accounts (8): system_program, ore_program, entropy_program,");
            info!("  fee_collector, board, config, treasury, entropy_var");
            
            return Ok(());
        }
        Some(config::Command::ShowLut) => {
            let lut_address = config.lut_address.ok_or("LUT_ADDRESS not set in .env")?;
            let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
            let lut_account = lut_manager.load_lut(lut_address)?;
            
            info!("LUT Address: {}", lut_address);
            info!("Contains {} addresses:", lut_account.addresses.len());
            for (i, addr) in lut_account.addresses.iter().enumerate() {
                info!("  [{}] {}", i, addr);
            }
            
            // Check deactivation status
            match lut_manager.get_deactivation_status()? {
                Some(slot) => info!("Status: DEACTIVATED at slot {} (can close after ~512 slots)", slot),
                None => info!("Status: ACTIVE"),
            }
            
            return Ok(());
        }
        Some(config::Command::DeactivateLut) => {
            let lut_address = config.lut_address.ok_or("LUT_ADDRESS not set in .env")?;
            let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
            lut_manager.load_lut(lut_address)?;
            
            // Check if already deactivated
            if let Some(slot) = lut_manager.get_deactivation_status()? {
                info!("LUT already deactivated at slot {}", slot);
                info!("Run 'close-lut' after ~512 slots to reclaim rent");
                return Ok(());
            }
            
            info!("Deactivating LUT {}...", lut_address);
            
            match crank.deactivate_lut(&lut_manager).await {
                Ok(_) => {
                    info!("✓ LUT deactivated successfully");
                    info!("Wait ~512 slots (~3-4 minutes) then run 'close-lut' to reclaim rent");
                }
                Err(e) => {
                    error!("✗ Failed to deactivate LUT: {}", e);
                    return Err(e.into());
                }
            }
            return Ok(());
        }
        Some(config::Command::CloseLut) => {
            let lut_address = config.lut_address.ok_or("LUT_ADDRESS not set in .env")?;
            let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
            lut_manager.load_lut(lut_address)?;
            
            // Check deactivation status
            match lut_manager.get_deactivation_status()? {
                None => {
                    error!("LUT is still active. Run 'deactivate-lut' first.");
                    return Ok(());
                }
                Some(deactivation_slot) => {
                    let current_slot = crank.get_current_slot()?;
                    let slots_since_deactivation = current_slot.saturating_sub(deactivation_slot);
                    
                    if slots_since_deactivation < 512 {
                        let slots_remaining = 512 - slots_since_deactivation;
                        error!("LUT still in cooldown. {} slots remaining (~{} seconds)", 
                            slots_remaining, slots_remaining * 400 / 1000);
                        return Ok(());
                    }
                    
                    info!("LUT deactivated at slot {}, current slot {}", deactivation_slot, current_slot);
                }
            }
            
            info!("Closing LUT {} and reclaiming rent...", lut_address);
            
            match crank.close_lut(&lut_manager).await {
                Ok(lamports) => {
                    info!("✓ LUT closed successfully");
                    info!("Reclaimed {} lamports ({:.6} SOL)", lamports, lamports as f64 / 1_000_000_000.0);
                    info!("Remove LUT_ADDRESS from .env and run 'create-lut' for a new LUT");
                }
                Err(e) => {
                    error!("✗ Failed to close LUT: {}", e);
                    return Err(e.into());
                }
            }
            return Ok(());
        }
        Some(config::Command::DeactivateUnused) => {
            info!("Scanning for unused/invalid LUTs...");
            
            let registry = LutRegistry::new(&config.rpc_url, crank.deploy_authority_pubkey());
            
            let unused_luts = registry.get_unused_luts()?;
            
            if unused_luts.is_empty() {
                info!("No unused LUTs found. All LUTs are valid.");
                return Ok(());
            }
            
            info!("Found {} unused/invalid LUTs:", unused_luts.len());
            for lut in &unused_luts {
                let lut_type = if lut.is_shared { "Shared" } else { "Miner" };
                let error_msg = lut.validation_error.as_deref().unwrap_or("Unknown");
                info!("  {} {} ({} accounts) - {}", 
                    lut_type, lut.address, lut.account_count, error_msg);
            }
            
            info!("\nDeactivating {} LUTs...", unused_luts.len());
            
            let mut deactivated = 0;
            for lut in &unused_luts {
                let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
                lut_manager.load_lut(lut.address)?;
                
                match crank.deactivate_lut(&lut_manager).await {
                    Ok(_) => {
                        info!("  ✓ Deactivated {}", lut.address);
                        deactivated += 1;
                    }
                    Err(e) => {
                        error!("  ✗ Failed to deactivate {}: {}", lut.address, e);
                    }
                }
            }
            
            info!("\nDeactivated {}/{} LUTs", deactivated, unused_luts.len());
            info!("Run 'cleanup-deactivated' after ~512 slots (~3.5 minutes) to close and reclaim rent");
            return Ok(());
        }
        Some(config::Command::CleanupDeactivated) => {
            info!("Scanning for deactivating LUTs...");
            
            let registry = LutRegistry::new(&config.rpc_url, crank.deploy_authority_pubkey());
            
            let deactivating_luts = registry.get_deactivating_luts()?;
            
            if deactivating_luts.is_empty() {
                info!("No deactivating LUTs found.");
                return Ok(());
            }
            
            let ready_to_close: Vec<_> = deactivating_luts.iter()
                .filter(|(_, slots_remaining)| *slots_remaining == 0)
                .collect();
            let still_cooling: Vec<_> = deactivating_luts.iter()
                .filter(|(_, slots_remaining)| *slots_remaining > 0)
                .collect();
            
            if !still_cooling.is_empty() {
                info!("LUTs still in cooldown ({}):", still_cooling.len());
                for (lut, slots_remaining) in &still_cooling {
                    let seconds_remaining = slots_remaining * 400 / 1000;
                    info!("  {} - {} slots remaining (~{} seconds)", 
                        lut.address, slots_remaining, seconds_remaining);
                }
            }
            
            if !ready_to_close.is_empty() {
                info!("\nClosing {} LUTs ready for cleanup...", ready_to_close.len());
                
                let mut closed = 0;
                let mut total_reclaimed = 0u64;
                
                for (lut, _) in &ready_to_close {
                    let mut lut_manager = LutManager::new(&config.rpc_url, crank.deploy_authority_pubkey());
                    lut_manager.load_lut(lut.address)?;
                    
                    match crank.close_lut(&lut_manager).await {
                        Ok(lamports) => {
                            info!("  ✓ Closed {} - reclaimed {} lamports", lut.address, lamports);
                            closed += 1;
                            total_reclaimed += lamports;
                        }
                        Err(e) => {
                            error!("  ✗ Failed to close {}: {}", lut.address, e);
                        }
                    }
                }
                
                info!("\nClosed {}/{} LUTs", closed, ready_to_close.len());
                info!("Total reclaimed: {} lamports ({:.6} SOL)", 
                    total_reclaimed, total_reclaimed as f64 / 1_000_000_000.0);
            } else {
                info!("\nNo LUTs ready to close yet.");
            }
            
            return Ok(());
        }
        Some(config::Command::CheckAccounts) => {
            info!("Checking all Evore program accounts...\n");
            crank.check_all_accounts()?;
            return Ok(());
        }
        Some(config::Command::Pipeline) => {
            info!("Starting new pipeline architecture...");
            
            // Load keypair
            let deploy_authority = Arc::new(config.load_keypair()?);
            info!("Deploy authority: {}", deploy_authority.pubkey());
            
            // Create RPC client
            let rpc_client = Arc::new(solana_client::rpc_client::RpcClient::new_with_commitment(
                config.rpc_url.clone(),
                solana_sdk::commitment_config::CommitmentConfig::confirmed(),
            ));
            
            // Run pipeline
            if let Err(e) = pipeline::run_pipeline(config, rpc_client, deploy_authority).await {
                error!("Pipeline error: {}", e);
                return Err(e.into());
            }
            
            return Ok(());
        }
        Some(config::Command::Run) | None => {
            // Continue to main loop
        }
    }

    info!("Database: {}", config.db_path.display());
    info!("Priority fee: {} microlamports/CU", config.priority_fee);
    
    // Initialize LUT Registry (multi-LUT support)
    let mut registry = LutRegistry::new(&config.rpc_url, crank.deploy_authority_pubkey());
    
    // Load all existing LUTs owned by our authority
    info!("Loading existing LUTs...");
    match registry.load_all_luts() {
        Ok(count) => info!("Found {} LUTs owned by deploy authority", count),
        Err(e) => warn!("Error loading LUTs: {}. Will create as needed.", e),
    }
    
    // Find deployers we manage
    let deployers = crank.find_deployers().await?;
    
    if deployers.is_empty() {
        warn!("No deployers found where we are the deploy_authority");
        warn!("Create a deployer with deploy_authority set to: {}", crank.deploy_authority_pubkey());
        return Ok(());
    }
    
    info!("Managing {} deployers", deployers.len());
    for d in &deployers {
        let fee_str = if d.bps_fee == 0 {
            format!("{} bps", d.bps_fee)
        } else {
            format!("{} lamports (flat)", d.flat_fee)
        };
        info!("  - Manager: {} (fee: {})", d.manager_address, fee_str);
    }
    
    // Ensure shared LUT exists
    info!("Ensuring shared LUT exists with static accounts...");
    match crank.ensure_shared_lut(&mut registry).await {
        Ok(addr) => info!("Shared LUT ready: {}", addr),
        Err(e) => {
            error!("Failed to setup shared LUT: {}", e);
            return Err(e.into());
        }
    }
    
    // Ensure all miners have LUTs
    info!("Ensuring all miners have LUTs...");
    match crank.ensure_all_miner_luts(&mut registry, &deployers, AUTH_ID).await {
        Ok(created) => {
            if created > 0 {
                info!("Created {} new miner LUTs", created);
            } else {
                info!("All miners already have LUTs");
            }
        }
        Err(e) => {
            error!("Failed to setup miner LUTs: {}", e);
            return Err(e.into());
        }
    }

    // Wrap registry in Arc<RwLock> for sharing across async tasks
    let registry = Arc::new(RwLock::new(registry));
    
    // Initialize miner cache for reduced RPC usage
    let mut miner_cache = miner_cache::MinerCache::new();
    
    // Main loop
    let poll_interval = Duration::from_millis(config.poll_interval_ms);
    info!("Starting main loop (poll interval: {}ms)", config.poll_interval_ms);
    info!("Strategy: deploy {} lamports/square, {} squares, {} slots before end",
        DEPLOY_AMOUNT_LAMPORTS, SQUARES_MASK.count_ones(), DEPLOY_SLOTS_BEFORE_END);
    info!("Max batch size: {} (limited by 64 account limit)", MAX_BATCH_SIZE);
    
    let mut last_round_id: Option<u64> = None;
    
    loop {
        // Check pending transactions first
        if let Err(e) = crank.check_pending_txs().await {
            error!("Error checking pending txs: {}", e);
        }
        
        // Run the deployment strategy with cached miner data
        if let Err(e) = run_strategy(&crank, &deployers, &mut last_round_id, &mut miner_cache, &registry).await {
            error!("Strategy error: {}", e);
        }
        
        tokio::time::sleep(poll_interval).await;
    }
}

/// Deployment strategy - customize this for your use case
/// Uses miner cache to minimize RPC calls
async fn run_strategy(
    crank: &crank::Crank,
    deployers: &[config::DeployerInfo],
    last_round_id: &mut Option<u64>,
    miner_cache: &mut miner_cache::MinerCache,
    registry: &Arc<RwLock<LutRegistry>>,
) -> Result<(), crank::CrankError> {
    // Get current board state (single RPC call)
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
    }
    
    // Refresh miner cache (batched RPC call - only when needed)
    // This fetches all miner accounts and balances in bulk
    if let Err(e) = miner_cache.refresh(crank.rpc_client(), deployers, AUTH_ID, board.round_id) {
        error!("Failed to refresh miner cache: {}", e);
        return Err(e);
    }
    
    // Don't deploy if too close to round end (transaction won't land in time)
    if slots_remaining < MIN_SLOTS_TO_DEPLOY {
        return Ok(());
    }
    
    // Only deploy when close to round end
    if slots_remaining > DEPLOY_SLOTS_BEFORE_END {
        return Ok(());
    }
    
    // Calculate required balance once (no RPC needed, just math)
    let required = crank::Crank::calculate_required_balance_simple(
        DEPLOY_AMOUNT_LAMPORTS,
        SQUARES_MASK,
        deployers.first().map(|d| d.flat_fee).unwrap_or(0),
        1, // flat fee type
    );
    
    // Collect deployers for deployment using cached data
    let mut to_deploy: Vec<(&config::DeployerInfo, u64, u64, u64, u32, Option<u64>)> = Vec::new();
    // (deployer, checkpoint_round, miner_address, has_sol_to_recycle)
    let mut checkpoint_only: Vec<(&config::DeployerInfo, u64, solana_sdk::pubkey::Pubkey, bool)> = Vec::new();
    
    for deployer in deployers {
        // Get miner address for this deployer
        let miner_address = match miner_cache.get_miner_address_for_deployer(&deployer.deployer_address) {
            Some(addr) => addr,
            None => continue, // Not in cache yet
        };
        
        // Check if already deployed this round using cache
        if miner_cache.has_deployed_in_round(&miner_address, board.round_id) {
            continue; // Already deployed, skip silently
        }
        
        // Check if checkpoint is needed using cache
        let checkpoint_round = miner_cache.needs_checkpoint(&miner_address);
        
        // Get cached balance
        let balance = miner_cache.get_balance(&miner_address).unwrap_or(0);
        
        // Check if miner has SOL rewards to recycle
        let has_sol_to_recycle = miner_cache.has_sol_to_recycle(&miner_address);
        
        if balance >= required {
            info!(
                "Adding {} to deploy batch: balance {} >= required {} lamports{}",
                deployer.manager_address, balance, required,
                if checkpoint_round.is_some() { format!(" (will checkpoint round {})", checkpoint_round.unwrap()) } else { "".to_string() }
            );
            to_deploy.push((deployer, AUTH_ID, board.round_id, DEPLOY_AMOUNT_LAMPORTS, SQUARES_MASK, checkpoint_round));
        } else if checkpoint_round.is_some() {
            // Not enough to deploy but needs checkpoint
            checkpoint_only.push((deployer, checkpoint_round.unwrap(), miner_address, has_sol_to_recycle));
        }
        // Don't log insufficient balance every poll - too noisy
    }
    
    // Execute checkpoint-only for miners that need it
    if !checkpoint_only.is_empty() {
        let with_recycle = checkpoint_only.iter().filter(|(_, _, _, has_sol)| *has_sol).count();
        let without_recycle = checkpoint_only.len() - with_recycle;
        info!("Executing {} checkpoint operations ({} with recycle, {} without)", 
            checkpoint_only.len(), with_recycle, without_recycle);
        for (deployer, round, _miner_addr, has_sol_to_recycle) in checkpoint_only {
            let op_name = if has_sol_to_recycle { "Checkpoint+recycle" } else { "Checkpoint" };
            match crank.execute_checkpoint_recycle(deployer, AUTH_ID, round, has_sol_to_recycle).await {
                Ok(sig) => {
                    info!("✓ {} for {}: {}", op_name, deployer.manager_address, sig);
                    // Invalidate cache after checkpoint
                    miner_cache.invalidate_balances();
                }
                Err(e) => error!("✗ {} failed for {}: {}", op_name, deployer.manager_address, e),
            }
        }
    }
    
    // Execute deploys in batches using multi-LUT
    if !to_deploy.is_empty() {
        info!("Deploying for {} managers (round {})", to_deploy.len(), board.round_id);
        
        let reg = registry.read().await;
        
        for batch in to_deploy.chunks(MAX_BATCH_SIZE) {
            let miner_addresses: Vec<_> = batch.iter()
                .filter_map(|(d, _, _, _, _, _)| miner_cache.get_miner_address_for_deployer(&d.deployer_address))
                .collect();
            let batch_vec: Vec<_> = batch.to_vec();
            let checkpoints_in_batch = batch.iter().filter(|(_, _, _, _, _, cp)| cp.is_some()).count();
            
            // Use multi-LUT transaction
            match crank.execute_batched_autodeploys_multi_lut(&reg, batch_vec).await {
                Ok(sig) => {
                    info!("✓ Autodeploy ({} deployers, {} checkpoints): {}", 
                        batch.len(), checkpoints_in_batch, sig);
                    // Mark miners as deployed in cache
                    miner_cache.mark_deployed(&miner_addresses, board.round_id);
                }
                Err(e) => {
                    error!("✗ Autodeploy failed: {}", e);
                    // Invalidate cache on failure to get fresh data next time
                    miner_cache.invalidate_balances();
                }
            }
        }
    }
    
    Ok(())
}
