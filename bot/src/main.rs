use std::sync::Arc;

use clap::{Parser, Subcommand};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
};
use tokio::sync::mpsc;

mod blockhash_cache;
mod board_tracker;
mod bot_runner;
mod bot_state;
mod bot_task;
mod client;
mod config;
mod coordinator;
mod deploy;
mod ev_calculator;
mod manage;
mod manage_tui;
mod miner_tracker;
mod round_tracker;
mod sender;
mod shutdown;
mod slot_tracker;
mod treasury_tracker;
mod tui;
mod tx_pipeline;

use bot_task::{run_bot_task, BotConfig};
use client::{print_managed_miner_info, EvoreClient};
use deploy::{continuous_deploy, single_deploy, EvDeployParams};
use slot_tracker::{http_to_ws_url, SlotTracker};
use tui::{App, BotState, TuiUpdate};

#[derive(Parser, Debug)]
#[command(name = "evore-bot")]
#[command(about = "Evore deployment bot for ORE v3")]
struct Args {
    /// RPC URL (HTTP)
    #[arg(long, env = "RPC_URL", default_value = "https://api.mainnet-beta.solana.com")]
    rpc_url: String,

    /// WebSocket URL (optional, derived from RPC URL if not provided)
    #[arg(long, env = "WS_URL")]
    ws_url: Option<String>,

    /// Path to signer keypair file (pays fees, signs transactions)
    #[arg(long, env = "KEYPAIR_PATH")]
    keypair: Option<String>,

    /// Path to manager keypair file (owns Manager account, controls managed miners)
    #[arg(long, env = "MANAGER_PATH")]
    manager_path: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show current round status
    Status,
    
    /// Show managed miner auth PDA info
    Info {
        /// Auth ID (default: 1)
        #[arg(long, default_value = "1")]
        auth_id: u64,
    },
    
    /// Single EV deployment (spam mode at round end)
    Deploy {
        /// Bankroll in lamports
        #[arg(long)]
        bankroll: u64,
        
        /// Max per square in lamports
        #[arg(long, default_value = "100000000")]
        max_per_square: u64,
        
        /// Min bet in lamports
        #[arg(long, default_value = "10000")]
        min_bet: u64,
        
        /// ORE value in lamports (for EV calculation)
        #[arg(long, default_value = "800000000")]
        ore_value: u64,
        
        /// Slots left threshold for deployment
        #[arg(long, default_value = "2")]
        slots_left: u64,
        
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
    },
    
    /// Continuous deployment loop (auto checkpoint & claim)
    Run {
        /// Bankroll in lamports
        #[arg(long)]
        bankroll: u64,
        
        /// Max per square in lamports
        #[arg(long, default_value = "0_010_000_000")]
        max_per_square: u64,
        
        /// Min bet in lamports
        #[arg(long, default_value = "0_001_000_000")]
        min_bet: u64,
        
        /// ORE value in lamports (for EV calculation)
        #[arg(long, default_value = "0_500_000_000")]
        ore_value: u64,
        
        /// Slots left threshold for deployment
        #[arg(long, default_value = "2")]
        slots_left: u64,
        
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
    },
    
    /// Checkpoint a round (auto-detects round_id from miner account if not specified)
    Checkpoint {
        /// Round ID to checkpoint (optional - auto-detected from miner if not provided)
        #[arg(long)]
        round_id: Option<u64>,
        
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
    },
    
    /// Claim SOL rewards
    ClaimSol {
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
    },
    
    /// Create a new Manager account
    CreateManager,
    
    /// Live TUI dashboard with real-time updates
    Dashboard {
        /// Path to TOML config file (overrides CLI args)
        #[arg(long)]
        config: Option<String>,
        
        /// Bankroll in lamports (ignored if --config provided)
        #[arg(long, default_value = "220000000")]
        bankroll: u64,
        
        /// Max per square in lamports
        #[arg(long, default_value = "10000000")]
        max_per_square: u64,
        
        /// Min bet in lamports
        #[arg(long, default_value = "1000000")]
        min_bet: u64,
        
        /// ORE value in lamports (for EV calculation)
        #[arg(long, default_value = "500000000")]
        ore_value: u64,
        
        /// Slots left threshold for deployment
        #[arg(long, default_value = "1")]
        slots_left: u64,
        
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
        
        /// Deploy strategy (EV, Percentage, Manual)
        #[arg(long, default_value = "EV")]
        strategy: String,
    },
    
    /// Manage miners - TUI for checkpoint, claim SOL/ORE across all signers
    Manage {
        /// Path to TOML config file (must have [manage] section)
        #[arg(long)]
        config: String,
    },
}

fn load_signer_keypair(path: Option<&String>) -> Result<Keypair, Box<dyn std::error::Error>> {
    let keypair_path = path
        .map(|p| p.to_string())
        .or_else(|| std::env::var("KEYPAIR_PATH").ok())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            format!("{}/.config/solana/id.json", home)
        });
    
    let keypair = read_keypair_file(&keypair_path)
        .map_err(|e| format!("Failed to read signer keypair from {}: {}", keypair_path, e))?;
    
    Ok(keypair)
}

fn load_manager_keypair(path: Option<&String>) -> Result<Keypair, Box<dyn std::error::Error>> {
    let manager_path = path
        .map(|p| p.to_string())
        .or_else(|| std::env::var("MANAGER_PATH").ok())
        .ok_or("MANAGER_PATH not set. Please provide --manager-path or set MANAGER_PATH env var")?;
    
    let keypair = read_keypair_file(&manager_path)
        .map_err(|e| format!("Failed to read manager keypair from {}: {}", manager_path, e))?;
    
    Ok(keypair)
}

fn get_ws_url(args: &Args) -> String {
    args.ws_url.clone().unwrap_or_else(|| http_to_ws_url(&args.rpc_url))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    
    let args = Args::parse();
    let client = EvoreClient::new(&args.rpc_url);
    
    match &args.command {
        Commands::Status => {
            println!("=== Evore Bot Status ===\n");
            
            // Use RPC for status (no need for websocket)
            let slot = client.get_slot()?;
            println!("Current Slot: {}", slot);
            
            let board = client.get_board()?;
            println!("Round ID:     {}", board.round_id);
            println!("Start Slot:   {}", board.start_slot);
            println!("End Slot:     {}", board.end_slot);
            
            let slots_remaining = board.end_slot.saturating_sub(slot);
            println!("Slots Left:   {}", slots_remaining);
            
            println!("\n--- Round {} Deployments ---", board.round_id);
            let round = client.get_round(board.round_id)?;
            let total: u64 = round.deployed.iter().sum();
            println!("Total Deployed: {} lamports ({:.4} SOL)", total, total as f64 / 1e9);
            
            for (i, amount) in round.deployed.iter().enumerate() {
                if *amount > 0 {
                    println!("  Square {}: {} ({:.4} SOL)", i, amount, *amount as f64 / 1e9);
                }
            }
        }
        
        Commands::Info { auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            println!("Signer:               {}", signer.pubkey());
            print_managed_miner_info(&manager, *auth_id);
        }
        
        Commands::Deploy { bankroll, max_per_square, min_bet, ore_value, slots_left, auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            let params = EvDeployParams {
                bankroll: *bankroll,
                max_per_square: *max_per_square,
                min_bet: *min_bet,
                ore_value: *ore_value,
                slots_left: *slots_left,
                attempts: 0,
            };
            
            println!("Signer:  {}", signer.pubkey());
            println!("Manager: {}", manager);
            
            // Start slot tracker
            let ws_url = get_ws_url(&args);
            println!("WS URL:  {}", ws_url);
            
            let slot_tracker = SlotTracker::new(&ws_url);
            slot_tracker.start_slot_subscription()?;
            
            // Wait for initial slot data
            println!("\nConnecting to websocket...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            
            if slot_tracker.get_slot() == 0 {
                return Err("Failed to get slot from websocket".into());
            }
            println!("Connected! Current slot: {}\n", slot_tracker.get_slot());
            
            single_deploy(&client, &slot_tracker, &signer, &manager, *auth_id, &params).await?;
        }
        
        Commands::Run { bankroll, max_per_square, min_bet, ore_value, slots_left, auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            let params = EvDeployParams {
                bankroll: *bankroll,
                max_per_square: *max_per_square,
                min_bet: *min_bet,
                ore_value: *ore_value,
                slots_left: *slots_left,
                attempts: 0,
            };
            
            // Start slot tracker
            let ws_url = get_ws_url(&args);
            println!("WS URL:  {}", ws_url);
            
            let slot_tracker = SlotTracker::new(&ws_url);
            slot_tracker.start_slot_subscription()?;
            
            // Wait for initial slot data
            println!("Connecting to websocket...");
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            
            if slot_tracker.get_slot() == 0 {
                return Err("Failed to get slot from websocket".into());
            }
            println!("Connected! Current slot: {}\n", slot_tracker.get_slot());
            
            continuous_deploy(&client, &slot_tracker, &signer, &manager, *auth_id, &params).await?;
        }
        
        Commands::Checkpoint { round_id, auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            // Get managed miner auth PDA (this is the miner authority)
            let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager, *auth_id);
            
            println!("=== Checkpoint ===\n");
            println!("Signer:             {}", signer.pubkey());
            println!("Manager:            {}", manager);
            println!("Managed Miner Auth: {}", managed_miner_auth);
            println!();
            
            // Get miner account to check status
            let miner = match client.get_miner(&managed_miner_auth)? {
                Some(m) => m,
                None => {
                    println!("✗ No miner account found for this managed miner auth.");
                    println!("  Have you deployed to any rounds yet?");
                    return Ok(());
                }
            };
            
            println!("--- Miner Status ---");
            println!("Last Round Played:   {}", miner.round_id);
            println!("Last Checkpointed:   {}", miner.checkpoint_id);
            println!("Claimable SOL:       {} lamports ({:.6} SOL)", miner.rewards_sol, miner.rewards_sol as f64 / 1e9);
            println!("Claimable ORE:       {} ({:.9} ORE)", miner.rewards_ore, miner.rewards_ore as f64 / 1e11);
            println!();
            
            // Determine which round to checkpoint
            let target_round = match round_id {
                Some(id) => *id,
                None => {
                    // Auto-detect: checkpoint the last round played if not yet checkpointed
                    if miner.round_id > miner.checkpoint_id {
                        miner.round_id
                    } else {
                        println!("✓ No checkpoint needed - miner is up to date!");
                        println!("  round_id ({}) == checkpoint_id ({})", miner.round_id, miner.checkpoint_id);
                        return Ok(());
                    }
                }
            };
            
            // Verify this round needs checkpointing
            if target_round <= miner.checkpoint_id {
                println!("✗ Round {} already checkpointed (checkpoint_id = {})", target_round, miner.checkpoint_id);
                return Ok(());
            }
            
            if target_round > miner.round_id {
                println!("✗ Round {} hasn't been played yet (last played = {})", target_round, miner.round_id);
                return Ok(());
            }
            
            // Check if round has ended
            let board = client.get_board()?;
            let current_slot = client.get_slot()?;
            
            // If target round is the current round, make sure it's ended
            if target_round == board.round_id && current_slot <= board.end_slot {
                println!("✗ Round {} is still active!", target_round);
                println!("  Current slot: {}, End slot: {}", current_slot, board.end_slot);
                println!("  Wait for the round to end before checkpointing.");
                return Ok(());
            }
            
            println!("Checkpointing round {}...", target_round);
            
            let blockhash = client.get_latest_blockhash()?;
            let tx = deploy::build_checkpoint_tx(&signer, &manager, *auth_id, target_round, blockhash);
            
            match client.send_and_confirm_transaction(&tx) {
                Ok(sig) => println!("✓ Checkpoint confirmed: {}", sig),
                Err(e) => println!("✗ Checkpoint failed: {}", e),
            }
        }
        
        Commands::ClaimSol { auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            println!("Claiming SOL for auth_id {}...", auth_id);
            println!("Signer:  {}", signer.pubkey());
            println!("Manager: {}", manager);
            
            let blockhash = client.get_latest_blockhash()?;
            let tx = deploy::build_claim_sol_tx(&signer, &manager, *auth_id, blockhash);
            
            match client.send_and_confirm_transaction(&tx) {
                Ok(sig) => println!("✓ Claim SOL confirmed: {}", sig),
                Err(e) => println!("✗ Claim SOL failed: {}", e),
            }
        }
        
        Commands::CreateManager => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            println!("=== Manager Account ===");
            println!("Signer (will be authority): {}", signer.pubkey());
            println!("Manager address:            {}", manager);
            println!();
            
            // Check if Manager already exists
            match client.get_manager(&manager)? {
                Some(manager_data) => {
                    println!("✓ Manager account already exists!");
                    println!();
                    println!("--- On-Chain Data ---");
                    println!("Authority: {}", manager_data.authority);
                    println!();
                    
                    if manager_data.authority == signer.pubkey() {
                        println!("✓ Your signer IS the authority - you can deploy!");
                    } else {
                        println!("✗ Your signer is NOT the authority!");
                        println!("  Expected: {}", manager_data.authority);
                        println!("  Got:      {}", signer.pubkey());
                        println!();
                        println!("To fix: Use the keypair for {} as KEYPAIR_PATH", manager_data.authority);
                    }
                }
                None => {
                    println!("Manager account does not exist. Creating...");
                    
                    let ix = evore::instruction::create_manager(signer.pubkey(), manager);
                    let blockhash = client.get_latest_blockhash()?;
                    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
                        &[ix],
                        Some(&signer.pubkey()),
                        &[&signer, &manager_keypair],
                        blockhash,
                    );
                    
                    match client.send_and_confirm_transaction(&tx) {
                        Ok(sig) => {
                            println!("✓ Manager created: {}", sig);
                            println!();
                            
                            // Read back and display
                            if let Ok(Some(manager_data)) = client.get_manager(&manager) {
                                println!("--- On-Chain Data ---");
                                println!("Authority: {}", manager_data.authority);
                            }
                        }
                        Err(e) => println!("✗ Create Manager failed: {}", e),
                    }
                }
            }
        }
        
        Commands::Dashboard { config: config_path, bankroll, max_per_square, min_bet, ore_value, slots_left, auth_id, strategy } => {
            // If config file provided, use the new multi-bot system
            if let Some(config_file) = config_path {
                run_dashboard_with_config(&args.rpc_url, get_ws_url(&args), config_file).await?;
            } else {
                // Legacy single-bot mode using CLI args
                let signer = load_signer_keypair(args.keypair.as_ref())?;
                let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
                let manager = manager_keypair.pubkey();
                
                let params = EvDeployParams {
                    bankroll: *bankroll,
                    max_per_square: *max_per_square,
                    min_bet: *min_bet,
                    ore_value: *ore_value,
                    slots_left: *slots_left,
                    attempts: 0,
                };
                
                run_dashboard(
                    &args.rpc_url,
                    get_ws_url(&args),
                    &signer,
                    manager,
                    *auth_id,
                    params,
                    strategy.clone(),
                    client,
                ).await?;
            }
        }
        
        Commands::Manage { config: config_path } => {
            run_manage_tui(&args.rpc_url, config_path).await?;
        }
    }
    
    Ok(())
}

async fn run_dashboard(
    rpc_url: &str,
    ws_url: String,
    signer: &Keypair,
    manager: Pubkey,
    auth_id: u64,
    params: EvDeployParams,
    strategy: String,
    client: EvoreClient,
) -> Result<(), Box<dyn std::error::Error>> {
    // Set panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));
    
    // Initialize TUI
    let mut terminal = tui::init()?;
    
    // Create app state
    let mut app = App::new(rpc_url);
    
    // Create bot state
    let bot_name = format!("bot-{}", auth_id);
    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager, auth_id);
    let bot = BotState::new(
        bot_name.clone(),
        0,  // bot_index (single bot mode)
        auth_id,
        strategy,
        params.bankroll,
        params.slots_left,
        signer.pubkey(),
        manager,
        managed_miner_auth,
        5000,   // priority_fee (default)
        200_000, // jito_tip (default)
        params.max_per_square,
        params.min_bet,
        params.ore_value,
        0,  // percentage (unused for EV strategy)
        0,  // squares_count (unused for EV strategy)
    );
    app.add_bot(bot);
    
    // Start slot tracker
    let slot_tracker = Arc::new(SlotTracker::new(&ws_url));
    slot_tracker.start_slot_subscription()?;
    
    // Wait for initial connection
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    
    // Wrap client and signer in Arc for sharing with bot task
    let client = Arc::new(client);
    let signer_bytes = signer.to_bytes();
    let signer = Arc::new(Keypair::try_from(signer_bytes.as_slice())?);
    
    // Create channel for bot → TUI updates
    let (update_tx, mut update_rx) = mpsc::unbounded_channel::<TuiUpdate>();
    
    // Create bot config
    let bot_config = BotConfig {
        name: bot_name,
        bot_index: 0,
        auth_id,
        manager,
        params,
    };
    
    // Spawn bot task
    let bot_client = client.clone();
    let bot_slot_tracker = slot_tracker.clone();
    let bot_signer = signer.clone();
    tokio::spawn(async move {
        run_bot_task(bot_config, bot_client, bot_slot_tracker, bot_signer, update_tx).await;
    });
    
    // Main TUI loop - just render and poll updates
    let result = run_tui_loop(&mut terminal, &mut app, &mut update_rx, &slot_tracker).await;
    
    // Always restore terminal
    tui::restore()?;
    
    result
}

/// Simple TUI loop - just polls for updates from bot tasks and renders
/// 
/// Bot logic runs in separate tokio task, TUI just displays state
async fn run_tui_loop(
    terminal: &mut tui::Tui,
    app: &mut App,
    update_rx: &mut mpsc::UnboundedReceiver<TuiUpdate>,
    slot_tracker: &Arc<SlotTracker>,
) -> Result<(), Box<dyn std::error::Error>> {
    while app.running {
        // Poll for updates from bot task (non-blocking)
        loop {
            match update_rx.try_recv() {
                Ok(update) => app.apply_update(update),
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    // Bot task died - exit
                    app.running = false;
                    break;
                }
            }
        }
        
        // Update slot from tracker (for display freshness)
        // Note: blockhash is updated via TuiUpdate::SlotUpdate from bot task
        app.update_slot(slot_tracker.get_slot());
        
        // Draw UI
        terminal.draw(|frame| tui::draw(frame, app))?;
        
        // Handle input (non-blocking check)
        match tui::handle_input(app)? {
            tui::InputResult::Quit => break,
            tui::InputResult::ReloadConfig(_) => {
                // Config reload not supported in single-bot mode
                app.set_status("Config reload not available".to_string(), true);
            }
            tui::InputResult::TogglePause(_) => {
                // Pause not supported in legacy single-bot mode
                app.set_status("Pause not available in legacy mode".to_string(), true);
            }
            tui::InputResult::Continue => {}
        }
        
        // Small sleep to prevent busy loop
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
    }
    
    Ok(())
}

/// Run dashboard using TOML config file with new multi-bot architecture
async fn run_dashboard_with_config(
    rpc_url: &str,
    ws_url: String,
    config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::Config;
    use crate::coordinator::RoundCoordinator;
    use crate::shutdown::spawn_shutdown_handler;
    use std::path::Path;
    
    // Load config
    let config = Config::load(Path::new(config_path))?;
    
    if config.bots.is_empty() {
        return Err("No bots defined in config file".into());
    }
    
    println!("=== Evore Multi-Bot Dashboard ===");
    println!("Config: {}", config_path);
    println!("Bots:   {}", config.bots.len());
    for bot in &config.bots {
        println!("  - {} (auth_id={}, strategy={:?})", bot.name, bot.auth_id, bot.strategy);
    }
    println!();
    
    // Create RPC client for setup
    let setup_client = client::EvoreClient::new(rpc_url);
    
    // Ensure all manager accounts exist
    println!("Checking manager accounts...");
    for bot_config in &config.bots {
        let manager_path = config.get_manager_path(bot_config);
        let manager_keypair = solana_sdk::signature::read_keypair_file(&manager_path)
            .map_err(|e| format!("Failed to load manager from {:?}: {}", manager_path, e))?;
        
        let signer_path = config.get_signer_path(bot_config);
        let signer_keypair = solana_sdk::signature::read_keypair_file(&signer_path)
            .map_err(|e| format!("Failed to load signer from {:?}: {}", signer_path, e))?;
        
        let manager_pubkey = manager_keypair.pubkey();
        
        match setup_client.get_manager(&manager_pubkey)? {
            Some(manager_data) => {
                println!("  ✓ {} - Manager exists (authority: {})", 
                    bot_config.name, 
                    if manager_data.authority == signer_keypair.pubkey() { "valid" } else { "MISMATCH!" }
                );
            }
            None => {
                println!("  → {} - Creating manager account...", bot_config.name);
                
                let ix = evore::instruction::create_manager(signer_keypair.pubkey(), manager_pubkey);
                let blockhash = setup_client.get_latest_blockhash()?;
                let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
                    &[ix],
                    Some(&signer_keypair.pubkey()),
                    &[&signer_keypair, &manager_keypair],
                    blockhash,
                );
                
                match setup_client.send_and_confirm_transaction(&tx) {
                    Ok(sig) => println!("    ✓ Created: {}", sig),
                    Err(e) => return Err(format!("Failed to create manager for {}: {}", bot_config.name, e).into()),
                }
            }
        }
    }
    println!();
    
    // Set panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = tui::restore();
        original_hook(panic_info);
    }));
    
    // Initialize TUI
    let mut terminal = tui::init()?;
    
    // Create app state
    let mut app = App::new(rpc_url);
    app.set_config_path(config_path.to_string());
    
    // Add bot states to app
    for (index, bot_config) in config.bots.iter().enumerate() {
        // Extract strategy params
        let (max_per_square, min_bet, ore_value, percentage, squares_count) = match &bot_config.strategy_params {
            crate::config::StrategyParams::EV { max_per_square, min_bet, ore_value } => {
                (*max_per_square, *min_bet, *ore_value, 0, 0)
            }
            crate::config::StrategyParams::Percentage { percentage, squares_count } => {
                (0, 0, 0, *percentage, *squares_count)
            }
            crate::config::StrategyParams::Manual { .. } => {
                (0, 0, 0, 0, 0)
            }
        };
        
        // Load manager to get pubkey
        let manager_path = config.get_manager_path(bot_config);
        let manager_keypair = solana_sdk::signature::read_keypair_file(&manager_path)
            .map_err(|e| format!("Failed to load manager from {:?}: {}", manager_path, e))?;
        
        let signer_path = config.get_signer_path(bot_config);
        let signer_keypair = solana_sdk::signature::read_keypair_file(&signer_path)
            .map_err(|e| format!("Failed to load signer from {:?}: {}", signer_path, e))?;
        
        let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager_keypair.pubkey(), bot_config.auth_id);
        let mut bot_state = BotState::new(
            bot_config.name.clone(),
            index,  // bot_index for unique icon assignment
            bot_config.auth_id,
            format!("{:?}", bot_config.strategy),
            bot_config.bankroll,
            bot_config.slots_left,
            signer_keypair.pubkey(),
            manager_keypair.pubkey(),
            managed_miner_auth,
            bot_config.priority_fee,
            bot_config.jito_tip,
            max_per_square,
            min_bet,
            ore_value,
            percentage,
            squares_count,
        );
        // Set initial pause state from config
        if bot_config.paused_on_startup {
            bot_state.is_paused = true;
            bot_state.status = tui::BotStatus::Paused;
        }
        app.add_bot(bot_state);
    }
    
    // Create channel for updates
    let (update_tx, mut update_rx) = mpsc::unbounded_channel::<TuiUpdate>();
    
    // Create coordinator
    let mut coordinator = RoundCoordinator::new(rpc_url, &ws_url, update_tx.clone())
        .map_err(|e| format!("Failed to create coordinator: {}", e))?;
    coordinator.start_services()
        .map_err(|e| format!("Failed to start services: {}", e))?;
    
    // Wait for services to initialize
    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    
    // Spawn bots from config
    coordinator.spawn_bots_from_config(&config)?;
    
    // Get RPS tracker from coordinator for shared tracking
    let rps_tracker = coordinator.get_rps_tracker();
    
    // Create miner tracker for per-bot deployment polling
    let mut miner_tracker = miner_tracker::MinerTracker::new(rpc_url, Arc::clone(&rps_tracker), update_tx.clone());
    for (index, bot_config) in config.bots.iter().enumerate() {
        // Get manager pubkey to derive miner PDA
        let manager_path = config.get_manager_path(bot_config);
        if let Ok(manager_keypair) = solana_sdk::signature::read_keypair_file(&manager_path) {
            let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager_keypair.pubkey(), bot_config.auth_id);
            // Add the authority (managed_miner_auth), the tracker will derive the miner PDA
            miner_tracker.add_miner(index, managed_miner_auth);
        }
    }
    miner_tracker.start();
    
    // Create treasury tracker for ORE treasury data
    let treasury_tracker = treasury_tracker::TreasuryTracker::new(rpc_url, Arc::clone(&rps_tracker), update_tx.clone());
    treasury_tracker.start();
    
    println!("Started {} bot(s). Press 'q' to quit.\n", coordinator.bot_count());
    
    // Setup shutdown handler
    let shutdown = spawn_shutdown_handler();
    
    // Create slot tracker for TUI updates
    let slot_tracker = Arc::new(SlotTracker::new(&ws_url));
    slot_tracker.start_slot_subscription()?;
    
    // Main TUI loop
    let result = async {
        while app.running && !shutdown.is_shutdown() {
            // Poll for updates from bot tasks
            loop {
                match update_rx.try_recv() {
                    Ok(update) => app.apply_update(update),
                    Err(mpsc::error::TryRecvError::Empty) => break,
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        app.running = false;
                        break;
                    }
                }
            }
            
            // Update slot from tracker
            // Note: blockhash is updated via TuiUpdate::SlotUpdate from bot tasks
            app.update_slot(slot_tracker.get_slot());
            
            // Update network stats from sender and trackers
            let ping_stats = coordinator.get_ping_stats();
            app.network_stats.sender_east_latency_ms = ping_stats.get_east_latency();
            app.network_stats.sender_west_latency_ms = ping_stats.get_west_latency();
            
            // Update connection statuses
            app.network_stats.slot_ws = if coordinator.is_slot_ws_connected() {
                tui::ConnectionStatus::Connected
            } else {
                tui::ConnectionStatus::Disconnected
            };
            app.network_stats.board_ws = if coordinator.is_board_ws_connected() {
                tui::ConnectionStatus::Connected
            } else {
                tui::ConnectionStatus::Disconnected
            };
            app.network_stats.round_ws = if coordinator.is_round_ws_connected() {
                tui::ConnectionStatus::Connected
            } else {
                tui::ConnectionStatus::Disconnected
            };
            app.network_stats.rpc = if coordinator.is_rpc_connected() {
                tui::ConnectionStatus::Connected
            } else {
                tui::ConnectionStatus::Disconnected
            };
            
            // Update RPS and totals (RPC and Sender)
            app.network_stats.rpc_rps = coordinator.get_rpc_rps();
            app.network_stats.sender_rps = coordinator.get_sender_rps();
            app.network_stats.rpc_total = coordinator.get_rpc_total();
            app.network_stats.sender_total = coordinator.get_sender_total();
            
            // Draw UI
            terminal.draw(|frame| tui::draw(frame, &app))?;
            
            // Handle input
            match tui::handle_input(&mut app)? {
                tui::InputResult::Quit => break,
                tui::InputResult::TogglePause(bot_idx) => {
                    // Toggle pause state for bot
                    match coordinator.toggle_bot_pause(bot_idx).await {
                        Ok(is_paused) => {
                            // Get bot name first, then update state
                            let bot_name = app.bots.get(bot_idx).map(|b| b.name.clone());
                            
                            // Update TUI state
                            if let Some(bot) = app.bots.get_mut(bot_idx) {
                                bot.is_paused = is_paused;
                                if is_paused {
                                    bot.status = tui::BotStatus::Paused;
                                } else {
                                    bot.status = tui::BotStatus::Loading;
                                }
                            }
                            
                            // Show status message
                            if let Some(name) = bot_name {
                                if is_paused {
                                    app.set_status(format!("⏸️ {} paused", name), false);
                                } else {
                                    app.set_status(format!("▶️ {} resumed", name), false);
                                }
                            }
                        }
                        Err(e) => {
                            app.set_status(format!("Pause error: {}", e), true);
                        }
                    }
                }
                tui::InputResult::ReloadConfig(bot_idx) => {
                    // Try to reload config from file
                    let config_path_clone = app.config_path.clone();
                    if let Some(config_path) = config_path_clone {
                        match Config::load(Path::new(&config_path)) {
                            Ok(new_config) => {
                                // Find the bot config by index
                                if let Some(new_bot_config) = new_config.bots.get(bot_idx) {
                                    // Update the bot's runtime config (actual deployment values)
                                    if let Err(e) = coordinator.update_bot_config(bot_idx, new_bot_config).await {
                                        app.set_status(format!("Config update error: {}", e), true);
                                        continue;
                                    }
                                    
                                    // Update the bot's display config (TUI)
                                    let bot_name = app.bots.get(bot_idx).map(|b| b.name.clone());
                                    if let Some(bot) = app.bots.get_mut(bot_idx) {
                                        // Update bankroll and fees
                                        bot.bankroll = new_bot_config.bankroll;
                                        bot.slots_left_threshold = new_bot_config.slots_left;
                                        bot.priority_fee = new_bot_config.priority_fee;
                                        bot.jito_tip = new_bot_config.jito_tip;
                                        
                                        // Update strategy params
                                        match &new_bot_config.strategy_params {
                                            crate::config::StrategyParams::EV { max_per_square, min_bet, ore_value } => {
                                                bot.max_per_square = *max_per_square;
                                                bot.min_bet = *min_bet;
                                                bot.ore_value = *ore_value;
                                            }
                                            crate::config::StrategyParams::Percentage { percentage, squares_count } => {
                                                bot.percentage = *percentage;
                                                bot.squares_count = *squares_count;
                                            }
                                            _ => {}
                                        }
                                    }
                                    
                                    if let Some(name) = bot_name {
                                        app.set_status(format!("Config reloaded: {}", name), false);
                                    }
                                } else {
                                    app.set_status(format!("Bot {} not found in config", bot_idx), true);
                                }
                            }
                            Err(e) => {
                                app.set_status(format!("Config error: {}", e), true);
                            }
                        }
                    } else {
                        app.set_status("No config path set".to_string(), true);
                    }
                }
                tui::InputResult::Continue => {}
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }.await;
    
    // Cleanup
    coordinator.abort_all();
    tui::restore()?;
    
    result
}

/// Run the manage TUI for managing miners across all signers
async fn run_manage_tui(
    rpc_url: &str,
    config_path: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::Config;
    use crate::manage::{discover_accounts, load_signers_from_directory};
    use crate::manage_tui::{self, ManageApp};
    use std::path::Path;
    use solana_client::rpc_client::RpcClient;
    
    // Load config
    let config = Config::load(Path::new(config_path))?;
    
    if !config.manage.is_valid() {
        return Err("No [manage] section with signers_path in config file".into());
    }
    
    println!("=== Evore Miner Management ===");
    println!("Config: {}", config_path);
    println!("Signers path: {:?}", config.manage.signers_path);
    if let Some(ref secondary) = config.manage.secondary_program_id {
        println!("Secondary program: {}", secondary);
    }
    println!();
    
    // Create RPC client
    let rpc = RpcClient::new(rpc_url.to_string());
    
    // Load signers
    println!("Loading signers...");
    let signers_path = config.manage.signers_path.as_ref().unwrap();
    let signers = load_signers_from_directory(signers_path)?;
    println!("  Found {} signer(s)", signers.len());
    
    // Discover accounts
    println!("Discovering accounts...");
    let discovery = discover_accounts(&rpc, &config.manage)?;
    println!("  Managers: {}", discovery.managers.len());
    println!("  Miners: {}", discovery.miners.len());
    println!("  Legacy miners: {}", discovery.legacy_miners.len());
    println!();
    
    if discovery.miners.is_empty() && discovery.legacy_miners.is_empty() {
        println!("No miners found. Check that:");
        println!("  - signers_path contains valid keypair files (*.json)");
        println!("  - The signers are authorities on Manager accounts");
        println!("  - The managers have associated Miner accounts");
        return Ok(());
    }
    
    // Set panic hook to restore terminal on panic
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let _ = manage_tui::restore();
        original_hook(panic_info);
    }));
    
    // Initialize TUI
    let mut terminal = manage_tui::init()?;
    
    // Create app state
    let mut app = ManageApp::new(rpc_url, config.manage.clone(), discovery, signers);
    
    // Main TUI loop
    let result = async {
        while app.running {
            // Draw UI
            terminal.draw(|frame| manage_tui::draw(frame, &app))?;
            
            // Handle input
            match manage_tui::handle_input(&mut app)? {
                manage_tui::InputResult::Quit => break,
                manage_tui::InputResult::Refresh => {
                    app.set_status("Refreshing...".to_string(), false);
                    app.refreshing = true;
                    
                    // Re-discover accounts
                    match discover_accounts(&rpc, &config.manage) {
                        Ok(new_discovery) => {
                            app.discovery = new_discovery.clone();
                            app.all_miners = new_discovery.miners.clone();
                            app.all_miners.extend(new_discovery.legacy_miners.clone());
                            app.set_status(format!("Refreshed: {} miners", app.all_miners.len()), false);
                        }
                        Err(e) => {
                            app.set_status(format!("Refresh failed: {}", e), true);
                        }
                    }
                    app.refreshing = false;
                }
                manage_tui::InputResult::ExecuteAction(miner_idx, action) => {
                    // Clone the miner data we need to avoid borrow conflicts
                    let miner_data = app.all_miners.get(miner_idx).cloned();
                    
                    match miner_data {
                        None => {
                            app.set_status("Miner not found".to_string(), true);
                        }
                        Some(miner) => {
                            // Find the signer keypair for this miner
                            let signer = {
                                use solana_sdk::signer::Signer;
                                app.signers.iter()
                                    .find(|(kp, _)| kp.pubkey() == miner.signer)
                                    .map(|(kp, _)| kp.clone())
                            };
                            
                            match signer {
                                None => {
                                    app.set_status("Signer keypair not found".to_string(), true);
                                }
                                Some(signer_keypair) => {
                                    app.set_status(format!("Executing {}...", action.as_str()), false);
                                    
                                    let blockhash = rpc.get_latest_blockhash()
                                        .map_err(|e| format!("Failed to get blockhash: {}", e));
                                    
                                    match blockhash {
                                        Ok(bh) => {
                                            let tx_result = execute_miner_action(
                                                &rpc,
                                                &signer_keypair,
                                                &miner,
                                                action,
                                                bh,
                                            );
                                            
                                            match tx_result {
                                                Ok(sig) => {
                                                    app.log_tx(miner_idx, action, Some(sig), None);
                                                    app.set_status(format!("✓ {} complete: {}...", action.as_str(), &sig.to_string()[..8]), false);
                                                }
                                                Err(e) => {
                                                    app.log_tx(miner_idx, action, None, Some(e.clone()));
                                                    app.set_status(format!("✗ {} failed: {}", action.as_str(), e), true);
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            app.set_status(e, true);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                manage_tui::InputResult::Continue => {}
            }
            
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        }
        Ok::<(), Box<dyn std::error::Error>>(())
    }.await;
    
    // Restore terminal
    manage_tui::restore()?;
    
    result
}

/// Execute a miner action (checkpoint, claim_sol, claim_ore)
fn execute_miner_action(
    rpc: &solana_client::rpc_client::RpcClient,
    signer: &std::sync::Arc<Keypair>,
    miner: &manage::DiscoveredMiner,
    action: manage_tui::MinerAction,
    blockhash: solana_sdk::hash::Hash,
) -> Result<solana_sdk::signature::Signature, String> {
    
    let tx = match action {
        manage_tui::MinerAction::Checkpoint => {
            if miner.is_legacy {
                return Err("Checkpoint not supported for legacy miners".to_string());
            }
            
            // Build checkpoint transaction
            let round_id = miner.miner.round_id;
            deploy::build_checkpoint_tx(
                signer.as_ref(),
                &miner.manager,
                miner.auth_id,
                round_id,
                blockhash,
            )
        }
        manage_tui::MinerAction::ClaimSol => {
            if miner.is_legacy {
                // For legacy miners, we need to build the tx with the legacy program ID
                // For now, use the same instruction but note this may need adjustment
                // based on the legacy program's instruction format
                deploy::build_claim_sol_tx(
                    signer.as_ref(),
                    &miner.manager,
                    miner.auth_id,
                    blockhash,
                )
            } else {
                deploy::build_claim_sol_tx(
                    signer.as_ref(),
                    &miner.manager,
                    miner.auth_id,
                    blockhash,
                )
            }
        }
        manage_tui::MinerAction::ClaimOre => {
            // Build claim ORE transaction
            deploy::build_claim_ore_tx(
                signer.as_ref(),
                &miner.manager,
                miner.auth_id,
                blockhash,
            )
        }
    };
    
    // Send and confirm transaction
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(sig) => Ok(sig),
        Err(e) => Err(format!("{}", e)),
    }
}
