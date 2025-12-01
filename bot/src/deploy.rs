use solana_sdk::{
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use std::time::{Duration, Instant};
use tokio::time::sleep;

use crate::client::EvoreClient;
use crate::slot_tracker::SlotTracker;

/// Parameters for EV deployment
#[derive(Debug, Clone)]
pub struct EvDeployParams {
    pub bankroll: u64,
    pub max_per_square: u64,
    pub min_bet: u64,
    pub ore_value: u64,
    pub slots_left: u64,
}

impl Default for EvDeployParams {
    fn default() -> Self {
        Self {
            bankroll: 100_000_000,      // 0.1 SOL
            max_per_square: 100_000_000, // 0.1 SOL
            min_bet: 10_000,             // 0.00001 SOL
            ore_value: 800_000_000,      // 0.8 SOL
            slots_left: 2,
        }
    }
}

/// Priority fee in microlamports per compute unit (for future use)
/// 100,000 microlamports/CU * 1,400,000 CU = 140,000 lamports = 0.00014 SOL
#[allow(dead_code)]
const PRIORITY_FEE_MICROLAMPORTS: u64 = 100_000;

/// Build EV deploy transaction
pub fn build_ev_deploy_tx(
    signer: &Keypair,
    manager: &Pubkey,
    auth_id: u64,
    round_id: u64,
    params: &EvDeployParams,
    recent_blockhash: Hash,
) -> Transaction {
    let cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(1_400_000);
    // Priority fee disabled for now
    // let cu_price_ix = ComputeBudgetInstruction::set_compute_unit_price(PRIORITY_FEE_MICROLAMPORTS);
    let deploy_ix = evore::instruction::ev_deploy(
        signer.pubkey(),
        *manager,
        auth_id,
        round_id,
        params.bankroll,
        params.max_per_square,
        params.min_bet,
        params.ore_value,
        params.slots_left,
    );

    let mut tx = Transaction::new_with_payer(&[cu_limit_ix, deploy_ix], Some(&signer.pubkey()));
    tx.sign(&[signer], recent_blockhash);
    tx
}

/// Build checkpoint transaction
pub fn build_checkpoint_tx(
    signer: &Keypair,
    manager: &Pubkey,
    auth_id: u64,
    round_id: u64,
    recent_blockhash: Hash,
) -> Transaction {
    let checkpoint_ix = evore::instruction::mm_checkpoint(
        signer.pubkey(),
        *manager,
        round_id,
        auth_id,
    );

    let mut tx = Transaction::new_with_payer(&[checkpoint_ix], Some(&signer.pubkey()));
    tx.sign(&[signer], recent_blockhash);
    tx
}

/// Build claim SOL transaction
pub fn build_claim_sol_tx(
    signer: &Keypair,
    manager: &Pubkey,
    auth_id: u64,
    recent_blockhash: Hash,
) -> Transaction {
    let claim_ix = evore::instruction::mm_claim_sol(
        signer.pubkey(),
        *manager,
        auth_id,
    );

    let mut tx = Transaction::new_with_payer(&[claim_ix], Some(&signer.pubkey()));
    tx.sign(&[signer], recent_blockhash);
    tx
}

/// Single deployment using websocket slot tracking
/// Sends transactions every 100ms until slot changes past end_slot
pub async fn single_deploy(
    client: &EvoreClient,
    slot_tracker: &SlotTracker,
    signer: &Keypair,
    manager: &Pubkey,
    auth_id: u64,
    params: &EvDeployParams,
) -> Result<Vec<Signature>, Box<dyn std::error::Error>> {
    println!("=== Single Deploy ===\n");
    
    // Get PDAs
    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(*manager, auth_id);
    let (ore_miner_pda, _) = evore::ore_api::miner_pda(managed_miner_auth);
    
    // Show account balances
    println!("--- Account Balances ---");
    let signer_balance = client.rpc.get_balance(&signer.pubkey()).unwrap_or(0);
    println!("Signer ({}):", signer.pubkey());
    println!("  Balance: {} lamports ({:.6} SOL)", signer_balance, signer_balance as f64 / 1e9);
    
    let auth_balance = client.rpc.get_balance(&managed_miner_auth).unwrap_or(0);
    println!("Managed Miner Auth ({}):", managed_miner_auth);
    println!("  Balance: {} lamports ({:.6} SOL)", auth_balance, auth_balance as f64 / 1e9);
    
    if let Ok(Some(miner)) = client.get_miner(&managed_miner_auth) {
        println!("ORE Miner ({}):", ore_miner_pda);
        println!("  Last Round:     {}", miner.round_id);
        println!("  Checkpointed:   {}", miner.checkpoint_id);
        println!("  Rewards SOL:    {} lamports ({:.6} SOL)", miner.rewards_sol, miner.rewards_sol as f64 / 1e9);
        println!("  Rewards ORE:    {} ({:.9} ORE)", miner.rewards_ore, miner.rewards_ore as f64 / 1e11);
    } else {
        println!("ORE Miner: Not created yet (first deploy will create it)");
    }
    println!();
    
    // Get current state
    let board = client.get_board()?;
    let current_slot = slot_tracker.get_slot();
    
    println!("--- Round Info ---");
    println!("Round ID:     {}", board.round_id);
    println!("Start Slot:   {}", board.start_slot);
    if board.end_slot == u64::MAX {
        println!("End Slot:     MAX (waiting for first deployer)");
    } else {
        println!("End Slot:     {}", board.end_slot);
    }
    println!("Current Slot: {}", current_slot);
    
    // Handle round lifecycle edge cases
    if board.end_slot == u64::MAX {
        println!("\n⏳ Round not started yet (end_slot=MAX). Waiting for first deployer...");
        // Wait for round to actually start
        loop {
            sleep(Duration::from_millis(500)).await;
            if let Ok(b) = client.get_board() {
                if b.end_slot != u64::MAX {
                    println!("✓ Round started! New end_slot: {}", b.end_slot);
                    // Recurse with updated board
                    return Box::pin(single_deploy(client, slot_tracker, signer, manager, auth_id, params)).await;
                }
            }
            print!("\r  Waiting... slot {}   ", slot_tracker.get_slot());
            std::io::Write::flush(&mut std::io::stdout())?;
        }
    }
    
    if current_slot >= board.end_slot {
        let slots_past = current_slot.saturating_sub(board.end_slot);
        if slots_past < evore::ore_api::INTERMISSION_SLOTS {
            println!("\n⏳ Round ended, in intermission ({}/{} slots)...", slots_past, evore::ore_api::INTERMISSION_SLOTS);
        } else {
            println!("\n⏳ Round ended, waiting for reset...");
        }
        return Err("Round not active".into());
    }
    
    let slots_remaining = board.end_slot.saturating_sub(current_slot);
    println!("Slots Left:   {}", slots_remaining);
    println!();
    
    // Calculate deploy window
    // For single send (slots_left > 10), wait 1 extra slot to ensure on-chain check passes
    let deploy_start_slot = if params.slots_left > 10 {
        board.end_slot.saturating_sub(params.slots_left - 1) // Wait for 1 fewer slot remaining
    } else {
        board.end_slot.saturating_sub(params.slots_left)
    };
    
    // Wait until one slot BEFORE deploy_start_slot, then wait 200ms more
    // This starts sending ~200ms before the actual target slot (halfway through previous slot)
    let wait_until_slot = deploy_start_slot.saturating_sub(1);
    
    if current_slot < wait_until_slot {
        let target_slots_left = board.end_slot.saturating_sub(deploy_start_slot);
        println!("--- Waiting for Deploy Window ---");
        println!("Target: slot {} ({} slots left)", deploy_start_slot, target_slots_left);
        println!("Will start sending at slot {} + 100ms", wait_until_slot);
        
        // Show countdown while waiting for slot before target
        let mut last_slot = current_slot;
        loop {
            let now_slot = slot_tracker.get_slot();
            if now_slot >= wait_until_slot {
                break;
            }
            if now_slot != last_slot {
                let slots_until = wait_until_slot.saturating_sub(now_slot);
                let slots_left = board.end_slot.saturating_sub(now_slot);
                print!("\r  Slot {} | {} slots until pre-deploy | {} slots until end   ", now_slot, slots_until, slots_left);
                std::io::Write::flush(&mut std::io::stdout())?;
                last_slot = now_slot;
            }
            sleep(Duration::from_millis(50)).await;
        }
        
        // Wait additional 100ms (quarter through the slot before target)
        println!("\n  Slot {} reached, waiting 100ms...", wait_until_slot);
        sleep(Duration::from_millis(100)).await;
        println!("✓ Deploy window reached! Starting to send...");
    } else if current_slot < deploy_start_slot {
        // We're already in the slot before target, just wait the 100ms
        println!("--- Deploy Window ---");
        println!("Already at slot {}, waiting 100ms before sending...", current_slot);
        sleep(Duration::from_millis(100)).await;
        println!("✓ Starting to send...");
    }
    
    // Determine send strategy based on slots_left
    let send_interval_ms = if params.slots_left > 10 {
        0 // Single send, no spam
    } else if params.slots_left >= 5 {
        400 // Medium urgency: every 400ms
    } else {
        100 // High urgency: every 100ms
    };
    
    println!();
    if send_interval_ms == 0 {
        println!("📤 Single send mode (slots_left > 10)");
    } else {
        println!("🚀 Spam mode: sending every {}ms until slot {} (end)", send_interval_ms, board.end_slot);
    }
    println!();
    
    let mut signatures: Vec<Signature> = Vec::new();
    let mut tx_count = 0;
    let start = Instant::now();
    
    loop {
        let current = slot_tracker.get_slot();
        
        // Stop if we've reached end slot (last deployable slot is end_slot - 1)
        // ORE deploy fails if clock.slot >= board.end_slot
        if current >= board.end_slot {
            println!("\n⏱️  Reached end slot {}, stopping (last deployable was {})", board.end_slot, board.end_slot - 1);
            break;
        }
        
        // Get fresh blockhash from tracker
        let blockhash = slot_tracker.get_blockhash();
        
        // Skip if blockhash is default (not yet received)
        if blockhash == Hash::default() {
            sleep(Duration::from_millis(10)).await;
            continue;
        }
        
        let tx = build_ev_deploy_tx(
            signer,
            manager,
            auth_id,
            board.round_id,
            params,
            blockhash,
        );
        
        tx_count += 1;
        match client.send_transaction_no_wait(&tx) {
            Ok(sig) => {
                println!("  [{}] slot={} sig={}", tx_count, current, sig);
                signatures.push(sig);
            }
            Err(e) => {
                println!("  [{}] slot={} error: {}", tx_count, current, e);
            }
        }
        
        // If single send mode, just break after first tx
        if send_interval_ms == 0 {
            break;
        }
        
        // Wait before next tx
        sleep(Duration::from_millis(send_interval_ms)).await;
    }
    
    println!("\n✅ Sent {} transactions in {:?}", signatures.len(), start.elapsed());
    
    // Wait a bit then check confirmations
    println!("\nWaiting 5s for confirmations...");
    sleep(Duration::from_secs(5)).await;
    
    let mut confirmed = 0;
    for sig in &signatures {
        if client.confirm_transaction(sig).unwrap_or(false) {
            println!("  ✓ Confirmed: {}", sig);
            confirmed += 1;
        }
    }
    
    println!("\n📊 Result: {}/{} transactions confirmed", confirmed, signatures.len());
    
    Ok(signatures)
}

/// Continuous deployment loop using websocket slot tracking
pub async fn continuous_deploy(
    client: &EvoreClient,
    slot_tracker: &SlotTracker,
    signer: &Keypair,
    manager: &Pubkey,
    auth_id: u64,
    params: &EvDeployParams,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Continuous Deploy Mode ===\n");
    println!("Signer:  {}", signer.pubkey());
    println!("Manager: {}", manager);
    println!("Auth ID: {}", auth_id);
    println!("Bankroll: {} lamports ({:.4} SOL)", params.bankroll, params.bankroll as f64 / 1e9);
    println!("Deploy at: {} slots before end", params.slots_left);
    println!("\nPress Ctrl+C to stop\n");
    
    let mut last_round_deployed: Option<u64> = None;
    let mut last_round_checkpointed: Option<u64> = None;
    
    loop {
        // Get current state
        let board = match client.get_board() {
            Ok(b) => b,
            Err(e) => {
                println!("Error getting board: {}", e);
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        
        let current_slot = slot_tracker.get_slot();
        
        // Check if this is a new round we haven't deployed to
        let already_deployed = last_round_deployed == Some(board.round_id);
        
        // Handle round lifecycle states:
        // 1. board.end_slot == u64::MAX: Reset done, waiting for first deployer to start round
        // 2. current_slot >= board.end_slot: Round ended, in intermission or waiting for reset
        // 3. current_slot < board.end_slot: Round active
        
        if board.end_slot == u64::MAX {
            // Reset happened but no one has started the round yet
            print!("\rRound {}: Waiting for round to start (end_slot=MAX)...   ", board.round_id);
            std::io::Write::flush(&mut std::io::stdout())?;
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        
        if current_slot >= board.end_slot {
            // Round ended - could be in intermission or waiting for reset
            let slots_past_end = current_slot.saturating_sub(board.end_slot);
            if slots_past_end < evore::ore_api::INTERMISSION_SLOTS {
                print!("\rRound {}: Intermission ({}/{} slots)...   ", 
                       board.round_id, slots_past_end, evore::ore_api::INTERMISSION_SLOTS);
            } else {
                print!("\rRound {}: Waiting for reset...   ", board.round_id);
            }
            std::io::Write::flush(&mut std::io::stdout())?;
            sleep(Duration::from_millis(500)).await;
            continue;
        }
        
        let slots_remaining = board.end_slot.saturating_sub(current_slot);
        
        // Early checkpoint: if >50 slots left AND we have a previous round to checkpoint
        if slots_remaining > 50 {
            if let Some(last_round) = last_round_deployed {
                // Only checkpoint if we haven't already
                if last_round_checkpointed != Some(last_round) {
                    println!("\n--- Early checkpoint (>50 slots left): Round {} ---", last_round);
                    
                    // Wait for blockhash
                    let blockhash = loop {
                        let bh = slot_tracker.get_blockhash();
                        if bh != Hash::default() {
                            break bh;
                        }
                        sleep(Duration::from_millis(100)).await;
                    };
                    
                    // Checkpoint
                    let checkpoint_tx = build_checkpoint_tx(signer, manager, auth_id, last_round, blockhash);
                    match client.rpc.send_and_confirm_transaction(&checkpoint_tx) {
                        Ok(sig) => {
                            println!("✓ Checkpoint confirmed: {}", sig);
                            last_round_checkpointed = Some(last_round);
                        }
                        Err(e) => println!("✗ Checkpoint failed: {}", e),
                    }
                    
                    // Claim SOL only if there's something to claim
                    sleep(Duration::from_millis(500)).await;
                    let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(*manager, auth_id);
                    if let Ok(Some(miner)) = client.get_miner(&managed_miner_auth) {
                        if miner.rewards_sol > 0 {
                            let blockhash = slot_tracker.get_blockhash();
                            let claim_tx = build_claim_sol_tx(signer, manager, auth_id, blockhash);
                            match client.rpc.send_and_confirm_transaction(&claim_tx) {
                                Ok(sig) => println!("✓ Claim SOL confirmed: {} ({} lamports)", sig, miner.rewards_sol),
                                Err(e) => println!("✗ Claim SOL failed: {}", e),
                            }
                        } else {
                            println!("ℹ No SOL rewards to claim");
                        }
                    }
                    
                    continue;
                }
            }
        }
        
        // If already deployed this round, just wait
        if already_deployed {
            print!("\rRound {}: slot {} / {} ({} left), already deployed", 
                   board.round_id, current_slot, board.end_slot, slots_remaining);
            std::io::Write::flush(&mut std::io::stdout())?;
            sleep(Duration::from_millis(100)).await;
            continue;
        }
        
        // Calculate deploy window
        let deploy_start_slot = board.end_slot.saturating_sub(params.slots_left);
        
        // If in deploy window, deploy!
        if current_slot >= deploy_start_slot {
            println!("\n\n🎯 Deploy window! Round {} - slot {} (end: {})", 
                     board.round_id, current_slot, board.end_slot);
            
            match single_deploy(client, slot_tracker, signer, manager, auth_id, params).await {
                Ok(sigs) => {
                    if !sigs.is_empty() {
                        last_round_deployed = Some(board.round_id);
                    }
                }
                Err(e) => {
                    println!("Deploy error: {}", e);
                }
            }
        } else {
            // Not in window yet, show status
            let slots_until_deploy = deploy_start_slot.saturating_sub(current_slot);
            print!("\rRound {}: slot {} / {} ({} slots until deploy)", 
                   board.round_id, current_slot, board.end_slot, slots_until_deploy);
            std::io::Write::flush(&mut std::io::stdout())?;
            sleep(Duration::from_millis(100)).await;
        }
    }
}
