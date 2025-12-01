use std::time::Duration;

use clap::{Parser, Subcommand};
use solana_sdk::{
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair},
    signer::Signer,
};

mod client;
mod deploy;
mod slot_tracker;
mod tui;

use client::{print_managed_miner_info, EvoreClient};
use deploy::{continuous_deploy, single_deploy, EvDeployParams};
use slot_tracker::{http_to_ws_url, SlotTracker};
use tui::{App, DeployStatus, EventType};

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
        /// Bankroll in lamports (for EV calculation)
        #[arg(long, default_value = "100000000")]
        bankroll: u64,
        
        /// Auth ID
        #[arg(long, default_value = "1")]
        auth_id: u64,
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
            };
            
            println!("Signer:  {}", signer.pubkey());
            println!("Manager: {}", manager);
            
            // Start slot tracker
            let ws_url = get_ws_url(&args);
            println!("WS URL:  {}", ws_url);
            
            let slot_tracker = SlotTracker::new(&ws_url);
            slot_tracker.start_slot_subscription()?;
            slot_tracker.start_blockhash_subscription(&args.rpc_url)?;
            
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
            };
            
            // Start slot tracker
            let ws_url = get_ws_url(&args);
            println!("WS URL:  {}", ws_url);
            
            let slot_tracker = SlotTracker::new(&ws_url);
            slot_tracker.start_slot_subscription()?;
            slot_tracker.start_blockhash_subscription(&args.rpc_url)?;
            
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
            
            let blockhash = client.rpc.get_latest_blockhash()?;
            let tx = deploy::build_checkpoint_tx(&signer, &manager, *auth_id, target_round, blockhash);
            
            match client.rpc.send_and_confirm_transaction(&tx) {
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
            
            let blockhash = client.rpc.get_latest_blockhash()?;
            let tx = deploy::build_claim_sol_tx(&signer, &manager, *auth_id, blockhash);
            
            match client.rpc.send_and_confirm_transaction(&tx) {
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
                    let blockhash = client.rpc.get_latest_blockhash()?;
                    let tx = solana_sdk::transaction::Transaction::new_signed_with_payer(
                        &[ix],
                        Some(&signer.pubkey()),
                        &[&signer, &manager_keypair],
                        blockhash,
                    );
                    
                    match client.rpc.send_and_confirm_transaction(&tx) {
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
        
        Commands::Dashboard { bankroll, auth_id } => {
            let signer = load_signer_keypair(args.keypair.as_ref())?;
            let manager_keypair = load_manager_keypair(args.manager_path.as_ref())?;
            let manager = manager_keypair.pubkey();
            
            run_dashboard(
                &args.rpc_url,
                get_ws_url(&args),
                signer.pubkey(),
                manager,
                *auth_id,
                *bankroll,
                client,
            ).await?;
        }
    }
    
    Ok(())
}

async fn run_dashboard(
    rpc_url: &str,
    ws_url: String,
    signer: Pubkey,
    manager: Pubkey,
    auth_id: u64,
    bankroll: u64,
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
    let mut app = App::new(rpc_url.to_string(), signer, manager, auth_id);
    app.bankroll = bankroll;
    app.log("Dashboard started", EventType::Info);
    
    // Start slot tracker
    let slot_tracker = SlotTracker::new(&ws_url);
    slot_tracker.start_slot_subscription()?;
    slot_tracker.start_blockhash_subscription(rpc_url)?;
    app.log(format!("Connected to {}", ws_url), EventType::Success);
    
    // Initial fetch
    match client.get_board() {
        Ok(board) => {
            app.log(format!("Round {} loaded", board.round_id), EventType::Info);
            let round_id = board.round_id;
            app.board = Some(board);
            
            if let Ok(round) = client.get_round(round_id) {
                app.round = Some(round);
            }
        }
        Err(e) => {
            app.log(format!("Failed to get board: {}", e), EventType::Error);
        }
    }
    
    let mut last_update = std::time::Instant::now();
    
    // Main loop with error handling
    let result = run_dashboard_loop(&mut terminal, &mut app, &client, &slot_tracker, &mut last_update);
    
    // Always restore terminal
    tui::restore()?;
    
    result
}

fn run_dashboard_loop(
    terminal: &mut tui::Tui,
    app: &mut App,
    client: &EvoreClient,
    slot_tracker: &SlotTracker,
    last_update: &mut std::time::Instant,
) -> Result<(), Box<dyn std::error::Error>> {
    while app.running {
        // Update slot from tracker
        app.update_slot(slot_tracker.get_slot());
        
        // Periodic refresh of board/round (every 2 seconds)
        if last_update.elapsed() > Duration::from_secs(2) {
            if let Ok(board) = client.get_board() {
                let round_id = board.round_id;
                
                // Check if new round
                if app.board.as_ref().map(|b| b.round_id) != Some(round_id) {
                    app.log(format!("New round: {}", round_id), EventType::Info);
                    app.deploy_status = DeployStatus::Idle;
                    app.transactions_sent = 0;
                    app.transactions_confirmed = 0;
                }
                
                app.board = Some(board);
                
                if let Ok(round) = client.get_round(round_id) {
                    app.round = Some(round);
                }
            }
            *last_update = std::time::Instant::now();
        }
        
        // Draw UI
        terminal.draw(|frame| tui::draw(frame, app))?;
        
        // Handle input
        if tui::handle_input(app)? {
            break;
        }
    }
    
    Ok(())
}
