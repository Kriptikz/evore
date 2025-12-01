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
    
    // Get current state
    let board = client.get_board()?;
    let current_slot = slot_tracker.get_slot();
    
    println!("Round ID:     {}", board.round_id);
    println!("Current Slot: {} (via websocket)", current_slot);
    println!("End Slot:     {}", board.end_slot);
    
    let slots_remaining = board.end_slot.saturating_sub(current_slot);
    println!("Slots Left:   {}\n", slots_remaining);
    
    // Calculate deploy window
    // For single send (slots_left > 10), wait 1 extra slot to ensure on-chain check passes
    let deploy_start_slot = if params.slots_left > 10 {
        board.end_slot.saturating_sub(params.slots_left - 1) // Wait for 1 fewer slot remaining
    } else {
        board.end_slot.saturating_sub(params.slots_left)
    };
    
    if current_slot < deploy_start_slot {
        let target_slots_left = board.end_slot.saturating_sub(deploy_start_slot);
        println!("Waiting for deploy window (slot {}, {} slots left)...", deploy_start_slot, target_slots_left);
        slot_tracker.wait_until_slot(deploy_start_slot).await;
        println!("Deploy window reached!");
    }
    
    // Determine send strategy based on slots_left
    let send_interval_ms = if params.slots_left > 10 {
        0 // Single send, no spam
    } else if params.slots_left >= 5 {
        400 // Medium urgency: every 400ms
    } else {
        100 // High urgency: every 100ms
    };
    
    if send_interval_ms == 0 {
        println!("\n📤 Single send mode (slots_left > 10, waiting for {} slots left)\n", params.slots_left - 1);
    } else {
        println!("\n🚀 Spam mode: sending every {}ms until slot {}\n", send_interval_ms, board.end_slot);
    }
    
    let mut signatures: Vec<Signature> = Vec::new();
    let mut tx_count = 0;
    let start = Instant::now();
    
    loop {
        let current = slot_tracker.get_slot();
        
        // Stop if we've passed the end slot
        if current > board.end_slot {
            println!("\n⏱️  Passed end slot {}, stopping", board.end_slot);
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
        let slots_remaining = board.end_slot.saturating_sub(current_slot);
        
        // Check if this is a new round we haven't deployed to
        let already_deployed = last_round_deployed == Some(board.round_id);
        
        // If we're past the round end, do checkpoint and claim
        if current_slot > board.end_slot {
            if let Some(last_round) = last_round_deployed {
                println!("\n--- Round {} ended, checkpointing and claiming ---", last_round);
                
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
                    Ok(sig) => println!("✓ Checkpoint confirmed: {}", sig),
                    Err(e) => println!("✗ Checkpoint failed: {}", e),
                }
                
                // Claim SOL
                sleep(Duration::from_millis(500)).await;
                let blockhash = slot_tracker.get_blockhash();
                let claim_tx = build_claim_sol_tx(signer, manager, auth_id, blockhash);
                match client.rpc.send_and_confirm_transaction(&claim_tx) {
                    Ok(sig) => println!("✓ Claim SOL confirmed: {}", sig),
                    Err(e) => println!("✗ Claim SOL failed: {}", e),
                }
                
                last_round_deployed = None;
            }
            
            // Wait for new round
            println!("\rWaiting for new round...");
            sleep(Duration::from_secs(1)).await;
            continue;
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
