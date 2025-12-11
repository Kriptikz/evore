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
mod sender;

use clap::Parser;
use config::Config;
use std::time::Duration;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

// =============================================================================
// DEPLOYMENT STRATEGY - Customize these for your use case
// =============================================================================

/// Amount to deploy per square in lamports (0.01 SOL = 10_000_000 lamports)
const DEPLOY_AMOUNT_LAMPORTS: u64 = 10_000_000;

/// Minimum balance required in autodeploy_balance PDA to attempt a deploy
const MIN_BALANCE_LAMPORTS: u64 = 100_000_000; // 0.1 SOL

/// Which auth_id to deploy for (each manager can have multiple managed miners)
const AUTH_ID: u64 = 0;

/// Squares mask - which squares to deploy to (0x1FFFFFF = all 25 squares)
const SQUARES_MASK: u32 = 0x1FFFFFF;

/// How many slots before round end to trigger deployment
const DEPLOY_SLOTS_BEFORE_END: u64 = 5;

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
    
    info!("Evore Autodeploy Crank starting...");
    info!("RPC URL: {}", config.rpc_url);
    info!("Database: {}", config.db_path.display());
    info!("Priority fee: {} microlamports/CU", config.priority_fee);
    info!("Jito tip: {} lamports", config.jito_tip);
    
    // Initialize database
    info!("Initializing database...");
    let db_pool = db::init_db(&config.db_path).await?;
    info!("Database initialized");
    
    // Create crank instance
    let crank = crank::Crank::new(config.clone(), db_pool).await?;
    info!("Deploy authority: {}", crank.deploy_authority_pubkey());
    
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
    
    loop {
        // Check pending transactions first
        if let Err(e) = crank.check_pending_txs().await {
            error!("Error checking pending txs: {}", e);
        }
        
        // Run the deployment strategy
        if let Err(e) = run_strategy(&crank, &deployers, &mut last_round_id).await {
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
) -> Result<(), crank::CrankError> {
    // Get current board state
    let (board, current_slot) = crank.get_board()?;
    let slots_remaining = board.end_slot.saturating_sub(current_slot);
    
    // Check if this is a new round
    let is_new_round = last_round_id.map_or(true, |id| id != board.round_id);
    if is_new_round {
        info!("New round detected: {} (ends in {} slots)", board.round_id, slots_remaining);
        *last_round_id = Some(board.round_id);
    }
    
    // Only deploy when close to round end
    if slots_remaining > DEPLOY_SLOTS_BEFORE_END {
        return Ok(());
    }
    
    info!("Round {} ending soon ({} slots left) - checking deployers", board.round_id, slots_remaining);
    
    // Check each deployer
    for deployer in deployers {
        // Check if we have enough balance
        let balance = crank.get_autodeploy_balance(deployer)?;
        if balance < MIN_BALANCE_LAMPORTS {
            warn!(
                "Skipping {}: insufficient balance ({} < {} lamports)",
                deployer.manager_address, balance, MIN_BALANCE_LAMPORTS
            );
            continue;
        }
        
        info!(
            "Deploying for manager {} (round {}, balance: {} lamports)",
            deployer.manager_address, board.round_id, balance
        );
        
        match crank.execute_autodeploy(
            deployer,
            AUTH_ID,
            board.round_id,
            DEPLOY_AMOUNT_LAMPORTS,
            SQUARES_MASK,
        ).await {
            Ok(sig) => {
                info!("Autodeploy submitted: {}", sig);
            }
            Err(e) => {
                error!("Autodeploy failed for {}: {}", deployer.manager_address, e);
            }
        }
    }
    
    Ok(())
}
