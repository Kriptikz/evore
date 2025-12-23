//! Core crank logic
//!
//! Finds deployers where we are the deploy_authority and executes autodeploys

use evore::{
    consts::DEPLOY_FEE,
    instruction::{
        mm_full_autodeploy,
        // Legacy instructions (kept for backward compatibility)
        mm_autodeploy, mm_autocheckpoint, recycle_sol,
    },
    ore_api::{board_pda, miner_pda, round_pda, Board, Miner, Round},
    state::{managed_miner_auth_pda, Deployer},
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    compute_budget::ComputeBudgetInstruction,
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_instruction,
    transaction::Transaction,
};
use sqlx::{Pool, Sqlite};
use std::time::{SystemTime, UNIX_EPOCH};
use steel::AccountDeserialize;
use tracing::{debug, error, info, warn};

use crate::{
    config::{Config, DeployerInfo},
    db,
    lut::{LutManager, LutRegistry, get_miner_accounts, get_miner_auth_pda},
    sender::TxSender,
};

/// The crank runner
pub struct Crank {
    config: Config,
    rpc_client: RpcClient,
    deploy_authority: Keypair,
    sender: TxSender,
    db_pool: Pool<Sqlite>,
}

impl Crank {
    pub async fn new(config: Config, db_pool: Pool<Sqlite>) -> Result<Self, CrankError> {
        let deploy_authority = config.load_keypair()
            .map_err(|e| CrankError::KeypairLoad(e.to_string()))?;
        
        let rpc_client = RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        );
        
        let sender = TxSender::new(config.rpc_url.clone());
        
        Ok(Self {
            config,
            rpc_client,
            deploy_authority,
            sender,
            db_pool,
        })
    }
    
    /// Send a simple test transaction (0 lamport transfer to self)
    pub async fn send_test_transaction(&self) -> Result<String, CrankError> {
        let payer = &self.deploy_authority;
        
        info!("Sending test transaction from {}", payer.pubkey());
        
        // Get recent blockhash
        let recent_blockhash = self.rpc_client
            .get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        // Simple memo-like instruction (transfer 0 to self)
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(5000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            system_instruction::transfer(&payer.pubkey(), &payer.pubkey(), 0),
        ];
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        let signature = tx.signatures[0].to_string();
        info!("Test tx signature: {}", signature);
        
        // Send and confirm via standard RPC
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("Test transaction confirmed: {}", sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("Test transaction failed: {}", e);
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
    
    /// Send and confirm a transaction via standard RPC (for debugging)
    pub async fn send_and_confirm(&self, tx: &Transaction) -> Result<String, CrankError> {
        match self.sender.send_and_confirm_rpc(tx, 60).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Get a reference to the RPC client (for miner cache)
    pub fn rpc_client(&self) -> &RpcClient {
        &self.rpc_client
    }
    
    /// Find all deployer accounts where we are the deploy_authority
    /// Uses optimized GPA with data size filter for efficient bulk fetching
    pub async fn find_deployers(&self) -> Result<Vec<DeployerInfo>, CrankError> {
        let deploy_authority_pubkey = self.deploy_authority.pubkey();
        
        // Deployer size: 8 discriminator + 32 manager_key + 32 deploy_authority + 8 bps_fee + 8 flat_fee + 8 expected_bps_fee + 8 expected_flat_fee + 8 max_per_round = 112
        const DEPLOYER_SIZE: u64 = 112;
        
        info!("Scanning for deployers with deploy_authority: {} (data_size={})", 
            deploy_authority_pubkey, DEPLOYER_SIZE);
        
        // Use getProgramAccounts with optimized filters:
        // 1. Data size filter - most efficient, filters on server side
        // 2. Discriminator filter - ensures we get Deployer accounts
        // 3. Deploy authority filter - only accounts we manage
        let accounts = self.rpc_client.get_program_accounts_with_config(
            &evore::id(),
            solana_client::rpc_config::RpcProgramAccountsConfig {
                filters: Some(vec![
                    // Filter by data size first (most efficient filter)
                    solana_client::rpc_filter::RpcFilterType::DataSize(DEPLOYER_SIZE),
                    // Filter by account discriminator (Deployer = 101)
                    solana_client::rpc_filter::RpcFilterType::Memcmp(
                        solana_client::rpc_filter::Memcmp::new_base58_encoded(
                            0,
                            &[101, 0, 0, 0, 0, 0, 0, 0], // EvoreAccount::Deployer discriminator
                        ),
                    ),
                    // Filter by deploy_authority (offset: 8 discriminator + 32 manager_key = 40)
                    solana_client::rpc_filter::RpcFilterType::Memcmp(
                        solana_client::rpc_filter::Memcmp::new_base58_encoded(
                            40,
                            deploy_authority_pubkey.as_ref(),
                        ),
                    ),
                ]),
                account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        ).map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        info!("GPA returned {} deployer accounts", accounts.len());
        
        let mut deployers = Vec::new();
        
        for (deployer_address, account) in accounts {
            match Deployer::try_from_bytes(&account.data) {
                Ok(deployer) => {
                    let manager_address = deployer.manager_key;
                    let fee_str = format!("{} bps + {} lamports flat", deployer.bps_fee, deployer.flat_fee);
                    let expected_str = format!("expected: {} bps + {} lamports", deployer.expected_bps_fee, deployer.expected_flat_fee);

                    deployers.push(DeployerInfo {
                        deployer_address,
                        manager_address,
                        bps_fee: deployer.bps_fee,
                        flat_fee: deployer.flat_fee,
                        expected_bps_fee: deployer.expected_bps_fee,
                        expected_flat_fee: deployer.expected_flat_fee,
                        max_per_round: deployer.max_per_round,
                    });
                    
                    debug!(
                        "Found deployer: {} for manager: {} (fee: {}, {}, max_per_round: {})",
                        deployer_address, manager_address, fee_str, expected_str, deployer.max_per_round
                    );
                }
                Err(e) => {
                    warn!(
                        "Failed to parse deployer {}: {:?}",
                        deployer_address, e
                    );
                }
            }
        }
        
        Ok(deployers)
    }
    
    /// Check all Evore program accounts
    pub fn check_all_accounts(&self) -> Result<(), CrankError> {
        info!("Loading all accounts for Evore program {}...", evore::id());
        
        // Account sizes
        const MANAGER_SIZE: usize = 40;     // 8 discriminator + 32 authority
        const DEPLOYER_SIZE: usize = 112;   // 8 + 32 + 32 + 8 + 8 + 8 + 8 + 8 (with max_per_round)
        
        // Discriminators
        const MANAGER_DISCRIMINATOR: u8 = 100;
        const DEPLOYER_DISCRIMINATOR: u8 = 101;
        
        // Get all accounts owned by the Evore program
        let accounts = self.rpc_client.get_program_accounts(&evore::id())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        info!("Found {} total accounts", accounts.len());
        
        let mut managers = Vec::new();
        let mut deployers = Vec::new();
        let mut unknown = Vec::new();
        
        for (address, account) in &accounts {
            let data = &account.data;
            let size = data.len();
            
            // Check discriminator
            let discriminator = if size >= 8 { data[0] } else { 255 };
            
            match (discriminator, size) {
                (d, s) if d == MANAGER_DISCRIMINATOR && s == MANAGER_SIZE => {
                    managers.push(*address);
                }
                (d, s) if d == DEPLOYER_DISCRIMINATOR && s == DEPLOYER_SIZE => {
                    deployers.push(*address);
                }
                _ => {
                    unknown.push((*address, discriminator, size));
                }
            }
        }
        
        // Print summary
        info!("\n=== Evore Program Account Summary ===");
        info!("Manager accounts (40 bytes): {}", managers.len());
        info!("Deployer accounts (112 bytes): {}", deployers.len());
        
        if !unknown.is_empty() {
            warn!("\n⚠ Found {} unknown/unexpected accounts:", unknown.len());
            for (addr, disc, size) in &unknown {
                warn!("  - {} (discriminator: {}, size: {} bytes)", addr, disc, size);
            }
        }
        
        if unknown.is_empty() {
            info!("\n✓ All accounts are in expected format!");
        }
        
        Ok(())
    }
    
    /// Get current ORE board state
    pub fn get_board(&self) -> Result<(Board, u64), CrankError> {
        let (board_address, _) = board_pda();
        
        let account = self.rpc_client.get_account(&board_address)
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let board = Board::try_from_bytes(&account.data)
            .map_err(|e| CrankError::Deserialize(format!("{:?}", e)))?;
        
        let current_slot = self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        Ok((*board, current_slot))
    }
    
    /// Get current ORE round state
    pub fn get_round(&self, round_id: u64) -> Result<Round, CrankError> {
        let (round_address, _) = round_pda(round_id);
        
        let account = self.rpc_client.get_account(&round_address)
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let round = Round::try_from_bytes(&account.data)
            .map_err(|e| CrankError::Deserialize(format!("{:?}", e)))?;
        
        Ok(*round)
    }
    
    /// Get balance for a managed miner auth PDA
    pub fn get_miner_balance(&self, deployer: &DeployerInfo, auth_id: u64) -> Result<u64, CrankError> {
        let (managed_miner_auth, _) = managed_miner_auth_pda(deployer.manager_address, auth_id);
        self.rpc_client.get_balance(&managed_miner_auth)
            .map_err(|e| CrankError::Rpc(e.to_string()))
    }

    // Constants matching the program's process_mm_autodeploy.rs
    const AUTH_PDA_RENT: u64 = 890_880;
    const ORE_CHECKPOINT_FEE: u64 = 10_000;
    const ORE_MINER_SIZE: usize = 8 + 584; // discriminator + Miner struct size
    
    /// Calculate the required balance for a deploy, checking actual account states
    pub fn calculate_required_balance_with_state(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        amount_per_square: u64,
        squares_mask: u32,
    ) -> Result<u64, CrankError> {
        let num_squares = squares_mask.count_ones() as u64;
        let total_deployed = amount_per_square * num_squares;
        
        // Calculate deployer fee (bps_fee + flat_fee are additive)
        let bps_fee_amount = total_deployed * deployer.bps_fee / 10_000;
        let deployer_fee = bps_fee_amount + deployer.flat_fee;
        
        let protocol_fee = DEPLOY_FEE;
        
        // Check managed_miner_auth balance
        let (managed_miner_auth, _) = managed_miner_auth_pda(deployer.manager_address, auth_id);
        let current_auth_balance = self.rpc_client.get_balance(&managed_miner_auth).unwrap_or(0);
        
        // Check if ORE miner exists
        let (ore_miner_address, _) = miner_pda(managed_miner_auth);
        let miner_exists = self.rpc_client.get_account(&ore_miner_address).is_ok();
        
        // Calculate miner rent if account doesn't exist
        let miner_rent = if !miner_exists {
            // Approximate rent for ORE miner account
            let rent = solana_sdk::rent::Rent::default();
            rent.minimum_balance(Self::ORE_MINER_SIZE)
        } else {
            0
        };
        
        // Required balance for managed_miner_auth
        let required_miner_balance = Self::AUTH_PDA_RENT
            .saturating_add(Self::ORE_CHECKPOINT_FEE)
            .saturating_add(total_deployed)
            .saturating_add(miner_rent);
        
        // How much needs to be transferred to the miner auth
        let transfer_to_miner = required_miner_balance.saturating_sub(current_auth_balance);
        
        // Total funds needed in managed_miner_auth
        // IMPORTANT: The managed_miner_auth needs to stay rent-exempt after transfers
        let total_needed = required_miner_balance
            .saturating_add(deployer_fee)
            .saturating_add(protocol_fee);
        
        info!(
            "Required balance: deploy={}, deployer_fee={}, protocol_fee={}, current_auth_balance={}, miner_rent={}, total={}",
            total_deployed, deployer_fee, protocol_fee, current_auth_balance, miner_rent, total_needed
        );
        
        Ok(total_needed)
    }
    
    /// Simple calculation without RPC calls (conservative estimate)
    /// fee_type: 0 = percentage (basis points), 1 = flat (lamports)
    pub fn calculate_required_balance_simple(amount_per_square: u64, squares_mask: u32, fee: u64, fee_type: u64) -> u64 {
        let num_squares = squares_mask.count_ones() as u64;
        let total_deployed = amount_per_square * num_squares;
        let deployer_fee = if fee_type == 0 {
            // Percentage (basis points)
            total_deployed * fee / 10_000
        } else {
            // Flat fee (lamports)
            fee
        };
        let protocol_fee = DEPLOY_FEE;
        
        // Conservative overhead for first-time deploy:
        // - auth rent + checkpoint fee + miner rent
        const MAX_OVERHEAD: u64 = 890_880 + 10_000 + 2_500_000; // ~0.0034 SOL
        
        total_deployed + deployer_fee + protocol_fee + MAX_OVERHEAD
    }
    
    /// Get miner checkpoint status for a manager/auth_id
    /// Returns (checkpoint_id, last_played_round_id) or None if the miner account doesn't exist yet
    pub fn get_miner_checkpoint_status(&self, manager: Pubkey, auth_id: u64) -> Result<Option<(u64, u64)>, CrankError> {
        let (managed_miner_auth, _) = managed_miner_auth_pda(manager, auth_id);
        let (ore_miner_address, _) = miner_pda(managed_miner_auth);
        
        match self.rpc_client.get_account(&ore_miner_address) {
            Ok(account) => {
                let miner = Miner::try_from_bytes(&account.data)
                    .map_err(|e| CrankError::Deserialize(format!("{:?}", e)))?;
                Ok(Some((miner.checkpoint_id, miner.round_id)))
            }
            Err(e) => {
                // Account doesn't exist - miner hasn't deployed yet
                if e.to_string().contains("AccountNotFound") {
                    Ok(None)
                } else {
                    Err(CrankError::Rpc(e.to_string()))
                }
            }
        }
    }
    
    /// Check if a deployer needs checkpointing
    pub fn needs_checkpoint(&self, deployer: &DeployerInfo, auth_id: u64) -> Result<Option<u64>, CrankError> {
        match self.get_miner_checkpoint_status(deployer.manager_address, auth_id)? {
            Some((checkpoint_id, miner_round_id)) => {
                if checkpoint_id < miner_round_id {
                    Ok(Some(miner_round_id))
                } else {
                    Ok(None)
                }
            }
            None => Ok(None),
        }
    }
    
    /// Execute checkpoint and optionally recycle (no deploy)
    /// Use this when balance is too low to deploy but we still want to checkpoint/claim winnings
    /// Only includes recycle if should_recycle is true (i.e., miner has SOL rewards to claim)
    pub async fn execute_checkpoint_recycle(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        checkpoint_round: u64,
        should_recycle: bool,
    ) -> Result<String, CrankError> {
        let op_name = if should_recycle { "checkpoint+recycle" } else { "checkpoint" };
        info!(
            "Executing {} for manager {} auth_id {} (checkpointing round {})",
            op_name, deployer.manager_address, auth_id, checkpoint_round
        );
        
        let payer = &self.deploy_authority;
        
        // Get recent blockhash
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // ~150k CU for checkpoint + recycle, ~100k for checkpoint only
        let cu_limit = if should_recycle { 200_000 } else { 150_000 };
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Checkpoint
        instructions.push(mm_autocheckpoint(
            payer.pubkey(),
            deployer.manager_address,
            checkpoint_round,
            auth_id,
        ));
        
        // Only include recycle if there's SOL to recycle
        if should_recycle {
            instructions.push(recycle_sol(
                payer.pubkey(),
                deployer.manager_address,
                auth_id,
            ));
        }
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        let signature = tx.signatures[0].to_string();
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ {} confirmed: {}", op_name, sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ {} failed: {}", op_name, e);
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
    
    /// Execute batched checkpoint+recycle for multiple deployers
    pub async fn execute_batched_checkpoint_recycle(
        &self,
        checkpoints: Vec<(&DeployerInfo, u64, u64)>, // (deployer, auth_id, checkpoint_round)
    ) -> Result<String, CrankError> {
        if checkpoints.is_empty() {
            return Err(CrankError::Send("No checkpoints to batch".to_string()));
        }
        
        let payer = &self.deploy_authority;
        
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // ~150k CU per checkpoint+recycle
        let cu_limit = (checkpoints.len() as u32 * 150_000).min(1_400_000);
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Add checkpoint + recycle for each
        for (deployer, auth_id, checkpoint_round) in &checkpoints {
            instructions.push(mm_autocheckpoint(
                payer.pubkey(),
                deployer.manager_address,
                *checkpoint_round,
                *auth_id,
            ));
            instructions.push(recycle_sol(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
            ));
        }
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Execute autodeploy WITHOUT checkpoint (checkpoint done separately)
    pub async fn execute_autodeploy_no_checkpoint(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        round_id: u64,
        amount: u64,
        squares_mask: u32,
    ) -> Result<String, CrankError> {
        let payer = &self.deploy_authority;
        
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(1_400_000));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Just the deploy (no checkpoint)
        instructions.push(mm_autodeploy(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
            round_id,
            amount,
            squares_mask,
        ));
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Execute batched autodeploys WITHOUT checkpoint (checkpoint done separately)
    pub async fn execute_batched_autodeploys_no_checkpoint(
        &self,
        deploys: Vec<(&DeployerInfo, u64, u64, u64, u32)>, // (deployer, auth_id, round_id, amount, mask)
    ) -> Result<String, CrankError> {
        if deploys.is_empty() {
            return Err(CrankError::Send("No deploys to batch".to_string()));
        }
        
        let payer = &self.deploy_authority;
        
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // ~500k CU per deploy
        let cu_limit = (deploys.len() as u32 * 500_000).min(1_400_000);
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Add all deploys (no checkpoint)
        for (deployer, auth_id, round_id, amount, squares_mask) in &deploys {
            instructions.push(mm_autodeploy(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
            ));
        }
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Execute batched autodeploys for multiple deployers in one transaction
    /// Each autodeploy uses ~60k CU, so we can fit ~10 in one tx
    pub async fn execute_batched_autodeploys(
        &self,
        deploys: Vec<(&DeployerInfo, u64, u64, u64, u32, Option<u64>)>, // (deployer, auth_id, round_id, amount, mask, checkpoint_round)
    ) -> Result<String, CrankError> {
        if deploys.is_empty() {
            return Err(CrankError::Send("No deploys to batch".to_string()));
        }
        
        let payer = &self.deploy_authority;
        
        // Get recent blockhash
        let (recent_blockhash, last_valid_blockheight) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // Calculate CU needed: ~60k per deploy, ~100k for checkpoint+recycle if needed
        let has_checkpoint = deploys.iter().any(|(_, _, _, _, _, cp)| cp.is_some());
        let cu_per_deploy = 70_000u32; // ~60k actual + buffer
        let checkpoint_cu = if has_checkpoint { 150_000u32 } else { 0 };
        let total_cu = checkpoint_cu + (deploys.len() as u32 * cu_per_deploy) + 50_000; // +50k buffer
        
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(total_cu.min(1_400_000)));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Add checkpoint + recycle for each deployer that needs it, then all deploys
        for (deployer, auth_id, _, _, _, checkpoint_round) in &deploys {
            if let Some(round_to_checkpoint) = checkpoint_round {
                instructions.push(mm_autocheckpoint(
                    payer.pubkey(),
                    deployer.manager_address,
                    *round_to_checkpoint,
                    *auth_id,
                ));
                instructions.push(recycle_sol(
                    payer.pubkey(),
                    deployer.manager_address,
                    *auth_id,
                ));
            }
        }
        
        // Add all deploy instructions
        for (deployer, auth_id, round_id, amount, squares_mask, _) in &deploys {
            instructions.push(mm_autodeploy(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
            ));
        }
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        let signature = tx.signatures[0].to_string();
        
        // Record all deploys in database
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        for (deployer, auth_id, round_id, amount, squares_mask, _) in &deploys {
            let num_squares = squares_mask.count_ones();
            let total_deployed = amount * num_squares as u64;
            let bps_fee_amount = total_deployed * deployer.bps_fee / 10_000;
            let deployer_fee = bps_fee_amount + deployer.flat_fee;
            
            db::insert_tx(
                &self.db_pool,
                &signature,
                &deployer.manager_address.to_string(),
                &deployer.deployer_address.to_string(),
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
                num_squares,
                total_deployed,
                deployer_fee,
                DEPLOY_FEE,
                self.config.priority_fee,
                0, // No Jito tip
                last_valid_blockheight,
                now,
            ).await.ok(); // Ignore duplicate key errors for batched txs
        }
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ Batched autodeploy ({} deploys) confirmed: {}", deploys.len(), sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ Batched autodeploy failed: {}", e);
                for (_, _, _, _, _, _) in &deploys {
                    db::update_tx_failed(&self.db_pool, &signature, &e.to_string())
                        .await
                        .ok();
                }
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
    
    /// Execute an autodeploy for a single deployer
    pub async fn execute_autodeploy(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        round_id: u64,
        amount: u64,
        squares_mask: u32,
    ) -> Result<String, CrankError> {
        let num_squares = squares_mask.count_ones();
        let total_deployed = amount * num_squares as u64;
        let bps_fee_amount = total_deployed * deployer.bps_fee / 10_000;
        let deployer_fee = bps_fee_amount + deployer.flat_fee;
        let protocol_fee = DEPLOY_FEE;
        
        // Check if checkpoint is needed
        let checkpoint_round = self.needs_checkpoint(deployer, auth_id)?;
        
        if checkpoint_round.is_some() {
            debug!("Will checkpoint round {} for manager {}", checkpoint_round.unwrap(), deployer.manager_address);
        }
        
        info!(
            "Executing autodeploy for manager {} auth_id {} round {} - {} squares, {} lamports each{}",
            deployer.manager_address, auth_id, round_id, num_squares, amount,
            if checkpoint_round.is_some() { format!(" (checkpointing round {})", checkpoint_round.unwrap()) } else { "".to_string() }
        );
        
        // Get recent blockhash
        let (recent_blockhash, last_valid_blockheight) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        // Build transaction
        let tx = self.build_autodeploy_tx(
            deployer,
            auth_id,
            round_id,
            amount,
            squares_mask,
            recent_blockhash,
            checkpoint_round,
        )?;
        
        // Get signature before sending
        let signature = tx.signatures[0].to_string();
        
        // Record transaction in database
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        db::insert_tx(
            &self.db_pool,
            &signature,
            &deployer.manager_address.to_string(),
            &deployer.deployer_address.to_string(),
            auth_id,
            round_id,
            amount,
            squares_mask,
            num_squares,
            total_deployed,
            deployer_fee,
            protocol_fee,
            self.config.priority_fee,
            0, // No Jito tip
            last_valid_blockheight,
            now,
        ).await.map_err(|e| CrankError::Database(e.to_string()))?;
        
        // Send and confirm transaction
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ Autodeploy confirmed: {}", sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ Autodeploy failed: {}", e);
                db::update_tx_failed(&self.db_pool, &signature, &e.to_string())
                    .await
                    .ok();
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
    
    /// Build an autodeploy transaction with optional checkpoint and recycle_sol
    fn build_autodeploy_tx(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        round_id: u64,
        amount: u64,
        squares_mask: u32,
        recent_blockhash: Hash,
        checkpoint_round: Option<u64>,
    ) -> Result<Transaction, CrankError> {
        let payer = &self.deploy_authority;
        
        // Start building instructions
        let mut instructions = Vec::new();
        
        // Compute budget instruction (adjust based on whether checkpoint is included)
        let cu_limit = if checkpoint_round.is_some() { 800_000 } else { 1_400_000 };
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Autocheckpoint instruction - checkpoint the round the miner last played in
        if let Some(round_to_checkpoint) = checkpoint_round {
            instructions.push(mm_autocheckpoint(
                payer.pubkey(),
                deployer.manager_address,
                round_to_checkpoint,
                auth_id,
            ));
        }
        
        // Recycle SOL instruction - always include (no-op if nothing to recycle)
        instructions.push(recycle_sol(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
        ));
        
        // Autodeploy instruction
        instructions.push(mm_autodeploy(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
            round_id,
            amount,
            squares_mask,
        ));
        
        let mut tx = Transaction::new_with_payer(
            &instructions,
            Some(&payer.pubkey()),
        );
        
        tx.sign(&[payer], recent_blockhash);
        
        Ok(tx)
    }
    
    /// Check and update pending transaction statuses
    pub async fn check_pending_txs(&self) -> Result<(), CrankError> {
        let pending_txs = db::get_pending_txs(&self.db_pool)
            .await
            .map_err(|e| CrankError::Database(e.to_string()))?;
        
        if pending_txs.is_empty() {
            return Ok(());
        }
        
        debug!("Checking {} pending transactions", pending_txs.len());
        
        // Get current blockheight for expiry comparison (not slot)
        let current_blockheight = self.rpc_client.get_block_height()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let current_slot = self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        for tx in pending_txs {
            let signature = solana_sdk::signature::Signature::from_str(&tx.signature)
                .map_err(|e| CrankError::Parse(e.to_string()))?;
            
            // Check transaction status first
            match self.rpc_client.get_signature_status_with_commitment(
                &signature,
                CommitmentConfig::confirmed(),
            ) {
                Ok(Some(result)) => {
                    match result {
                        Ok(()) => {
                            info!("Transaction {} confirmed", tx.signature);
                            
                            db::update_tx_confirmed(
                                &self.db_pool,
                                &tx.signature,
                                now,
                                current_slot,
                                None,
                            ).await.ok();
                            
                            // Check finalization
                            if let Ok(Some(Ok(()))) = self.rpc_client.get_signature_status_with_commitment(
                                &signature,
                                CommitmentConfig::finalized(),
                            ) {
                                info!("Transaction {} finalized", tx.signature);
                                db::update_tx_finalized(&self.db_pool, &tx.signature, now)
                                    .await
                                    .ok();
                            }
                        }
                        Err(e) => {
                            error!("Transaction {} failed: {:?}", tx.signature, e);
                            db::update_tx_failed(&self.db_pool, &tx.signature, &format!("{:?}", e))
                                .await
                                .ok();
                        }
                    }
                }
                Ok(None) => {
                    // Transaction not found - check if blockhash has expired
                    // last_valid_blockheight is the blockheight after which the tx is invalid
                    let last_valid = tx.last_valid_blockheight as u64;
                    if current_blockheight > last_valid {
                        info!("Transaction {} expired (blockheight {} > last_valid {})", 
                            tx.signature, current_blockheight, last_valid);
                        db::update_tx_expired(&self.db_pool, &tx.signature)
                            .await
                            .ok();
                    } else {
                        debug!("Transaction {} still pending (blockheight {}, valid until {})", 
                            tx.signature, current_blockheight, last_valid);
                    }
                }
                Err(e) => {
                    warn!("Error checking tx {}: {}", tx.signature, e);
                }
            }
        }
        
        Ok(())
    }
    
    /// Get the deploy authority public key
    pub fn deploy_authority_pubkey(&self) -> Pubkey {
        self.deploy_authority.pubkey()
    }
    
    /// Update expected fees for a deployer (as deploy_authority)
    /// This allows the deploy_authority to protect itself from fee changes by the manager
    /// Returns Ok(None) if the expected fees are already set correctly (no tx needed)
    /// Returns Ok(Some(signature)) if a transaction was sent
    pub async fn update_expected_fees(
        &self,
        deployer: &DeployerInfo,
        expected_bps_fee: u64,
        expected_flat_fee: u64,
    ) -> Result<Option<String>, CrankError> {
        // Check if already set to the desired values
        if deployer.expected_bps_fee == expected_bps_fee && deployer.expected_flat_fee == expected_flat_fee {
            return Ok(None);
        }
        
        let payer = &self.deploy_authority;
        
        // Build the update_deployer instruction
        // As deploy_authority, we can only update expected_* fields
        // We pass the current fees (they won't change) and set the expected fees
        let ix = evore::instruction::update_deployer(
            payer.pubkey(),
            deployer.manager_address,
            payer.pubkey(),  // Keep ourselves as deploy_authority
            deployer.bps_fee,  // Keep current bps_fee
            deployer.flat_fee,  // Keep current flat_fee
            expected_bps_fee,
            expected_flat_fee,
            deployer.max_per_round,  // Keep current max_per_round
        );
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(100_000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            ix,
        ];
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        // Send and confirm
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => Ok(Some(sig.to_string())),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Create a new Address Lookup Table
    pub async fn create_lut(&self, lut_manager: &mut LutManager) -> Result<Pubkey, CrankError> {
        let payer = &self.deploy_authority;
        
        // Get recent slot for LUT derivation
        let recent_slot = self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let (create_ix, lut_address) = lut_manager.create_lut_instruction(recent_slot)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(50_000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            create_ix,
        ];
        
        let tx = LutManager::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        // Send and confirm
        match self.sender.send_and_confirm_versioned_rpc(&tx, 60).await {
            Ok(_sig) => {
                lut_manager.set_lut_address(lut_address);
                info!("LUT created: {}", lut_address);
                Ok(lut_address)
            }
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Extend LUT with static shared accounts (one-time setup)
    /// Does NOT include round accounts - those are added separately per round
    pub async fn extend_lut_with_static_accounts(
        &self,
        lut_manager: &mut LutManager,
    ) -> Result<usize, CrankError> {
        let missing = lut_manager.get_missing_static_addresses();
        
        if missing.is_empty() {
            return Ok(0);
        }
        
        info!("Adding {} static shared addresses to LUT", missing.len());
        
        let payer = &self.deploy_authority;
        let mut total_added = 0;
        
        // LUT extension has a limit of ~30 addresses per tx
        for chunk in missing.chunks(25) {
            let extend_ix = lut_manager.extend_lut_instruction(chunk.to_vec())
                .map_err(|e| CrankError::Send(e.to_string()))?;
            
            let recent_blockhash = self.rpc_client.get_latest_blockhash()
                .map_err(|e| CrankError::Rpc(e.to_string()))?;
            
            let instructions = vec![
                ComputeBudgetInstruction::set_compute_unit_limit(100_000),
                ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
                extend_ix,
            ];
            
            let tx = LutManager::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
                .map_err(|e| CrankError::Send(e.to_string()))?;
            
            match self.sender.send_and_confirm_versioned_rpc(&tx, 60).await {
                Ok(_sig) => {
                    lut_manager.add_to_cache(chunk);
                    total_added += chunk.len();
                    info!("Added {} addresses to LUT ({} total)", chunk.len(), total_added);
                }
                Err(e) => {
                    error!("Failed to extend LUT: {}", e);
                    return Err(CrankError::Send(e.to_string()));
                }
            }
            
            // Wait a bit between extensions
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        
        // Wait for LUT to activate (1 slot)
        info!("Waiting for LUT addresses to activate...");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        
        Ok(total_added)
    }
    
    /// Deactivate a LUT (first step before closing)
    pub async fn deactivate_lut(&self, lut_manager: &LutManager) -> Result<(), CrankError> {
        let payer = &self.deploy_authority;
        
        let deactivate_ix = lut_manager.deactivate_lut_instruction()
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(50_000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            deactivate_ix,
        ];
        
        let tx = LutManager::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        self.sender.send_and_confirm_versioned_rpc(&tx, 60).await
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        Ok(())
    }
    
    /// Close a deactivated LUT and reclaim rent
    /// Returns the amount of lamports reclaimed
    pub async fn close_lut(&self, lut_manager: &LutManager) -> Result<u64, CrankError> {
        let payer = &self.deploy_authority;
        let lut_address = lut_manager.lut_address().ok_or(CrankError::Send("No LUT address".to_string()))?;
        
        // Get LUT balance before closing
        let lut_balance = self.rpc_client.get_balance(&lut_address)
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let close_ix = lut_manager.close_lut_instruction(payer.pubkey())
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(50_000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            close_ix,
        ];
        
        let tx = LutManager::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        self.sender.send_and_confirm_versioned_rpc(&tx, 60).await
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        Ok(lut_balance)
    }
    
    /// Get the current slot
    pub fn get_current_slot(&self) -> Result<u64, CrankError> {
        self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))
    }
    
    // =========================================================================
    // LutRegistry methods (multi-LUT support)
    // =========================================================================
    
    /// Create a new LUT and return its address
    pub async fn create_lut_for_registry(&self, registry: &LutRegistry) -> Result<Pubkey, CrankError> {
        let payer = &self.deploy_authority;
        
        let recent_slot = self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let (create_ix, lut_address) = registry.create_lut_instruction(recent_slot)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        let recent_blockhash = self.rpc_client.get_latest_blockhash()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let instructions = vec![
            ComputeBudgetInstruction::set_compute_unit_limit(50_000),
            ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
            create_ix,
        ];
        
        let tx = LutRegistry::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        self.sender.send_and_confirm_versioned_rpc(&tx, 60).await
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        info!("Created LUT: {}", lut_address);
        Ok(lut_address)
    }
    
    /// Extend a LUT with addresses
    pub async fn extend_lut_for_registry(
        &self,
        registry: &LutRegistry,
        lut_address: Pubkey,
        addresses: Vec<Pubkey>,
    ) -> Result<(), CrankError> {
        if addresses.is_empty() {
            return Ok(());
        }
        
        let payer = &self.deploy_authority;
        
        // Chunk addresses (max ~25 per tx)
        for chunk in addresses.chunks(25) {
            let extend_ix = registry.extend_lut_instruction(lut_address, chunk.to_vec())
                .map_err(|e| CrankError::Send(e.to_string()))?;
            
            let recent_blockhash = self.rpc_client.get_latest_blockhash()
                .map_err(|e| CrankError::Rpc(e.to_string()))?;
            
            let instructions = vec![
                ComputeBudgetInstruction::set_compute_unit_limit(100_000),
                ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee),
                extend_ix,
            ];
            
            let tx = LutRegistry::build_versioned_tx_no_lut(payer, instructions, recent_blockhash)
                .map_err(|e| CrankError::Send(e.to_string()))?;
            
            self.sender.send_and_confirm_versioned_rpc(&tx, 60).await
                .map_err(|e| CrankError::Send(e.to_string()))?;
            
            debug!("Extended LUT {} with {} addresses", lut_address, chunk.len());
            
            // Small delay between extensions
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        
        Ok(())
    }
    
    /// Ensure the shared LUT exists and has all static accounts
    pub async fn ensure_shared_lut(&self, registry: &mut LutRegistry) -> Result<Pubkey, CrankError> {
        // If no shared LUT, create one
        let shared_lut = if let Some(addr) = registry.shared_lut() {
            addr
        } else {
            let addr = self.create_lut_for_registry(registry).await?;
            registry.set_shared_lut(addr);
            
            // Wait for LUT to be active
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            addr
        };
        
        // Check for missing static addresses
        let missing = registry.get_missing_shared_addresses();
        if !missing.is_empty() {
            info!("Adding {} static accounts to shared LUT", missing.len());
            self.extend_lut_for_registry(registry, shared_lut, missing).await?;
            
            // Refresh cache
            registry.refresh_lut_cache(shared_lut)
                .map_err(|e| CrankError::Send(e.to_string()))?;
        }
        
        Ok(shared_lut)
    }
    
    /// Ensure a miner has a LUT with their accounts
    /// Returns the LUT address
    /// Ensure all deployers have their miner accounts in consolidated LUTs
    /// Uses consolidated LUTs with up to 30 miners each
    /// Returns count of miners added to LUTs
    /// Create a LUT for a specific miner
    pub async fn ensure_miner_lut(
        &self,
        registry: &mut LutRegistry,
        deployer: &DeployerInfo,
        auth_id: u64,
    ) -> Result<Pubkey, CrankError> {
        let miner_auth = get_miner_auth_pda(deployer.manager_address, auth_id);

        // Check if miner already has a LUT
        if let Some(lut_addr) = registry.get_miner_lut(&miner_auth) {
            return Ok(*lut_addr);
        }

        // Create new LUT for this miner
        info!("Creating LUT for miner {} (manager: {})", miner_auth, deployer.manager_address);
        let lut_address = self.create_lut_for_registry(registry).await?;

        // Wait for LUT to be active
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        // Get miner accounts and extend LUT
        let miner_accounts = get_miner_accounts(deployer.manager_address, auth_id);
        self.extend_lut_for_registry(registry, lut_address, miner_accounts.clone()).await?;

        // Register in registry
        registry.register_miner_lut(miner_auth, lut_address, miner_accounts);

        info!("Created miner LUT {} for {}", lut_address, miner_auth);
        Ok(lut_address)
    }

    /// Ensure all deployers have miner LUTs
    /// Returns count of new LUTs created
    pub async fn ensure_all_miner_luts(
        &self,
        registry: &mut LutRegistry,
        deployers: &[DeployerInfo],
        auth_id: u64,
    ) -> Result<usize, CrankError> {
        let mut created = 0;

        for deployer in deployers {
            let miner_auth = get_miner_auth_pda(deployer.manager_address, auth_id);

            if !registry.has_miner_lut(&miner_auth) {
                self.ensure_miner_lut(registry, deployer, auth_id).await?;
                created += 1;
            }
        }

        Ok(created)
    }

    /// Execute batched autodeploys using LutRegistry (multiple LUTs)
    /// Uses individual mm_full_autodeploy instructions for each deploy
    pub async fn execute_batched_autodeploys_multi_lut(
        &self,
        registry: &LutRegistry,
        deploys: Vec<(&DeployerInfo, u64, u64, u64, u32, Option<u64>)>, // (deployer, auth_id, round_id, amount, mask, checkpoint_round)
    ) -> Result<String, CrankError> {
        if deploys.is_empty() {
            return Err(CrankError::Send("No deploys to batch".to_string()));
        }

        let payer = &self.deploy_authority;

        let (recent_blockhash, last_valid_blockheight) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;

        // Collect miner_auths for LUT lookup
        let miner_auths: Vec<Pubkey> = deploys.iter()
            .map(|(d, auth_id, _, _, _, _)| get_miner_auth_pda(d.manager_address, *auth_id))
            .collect();

        // Get all relevant LUTs
        let lut_accounts = registry.get_luts_for_miners(&miner_auths);

        let mut instructions = Vec::new();

        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(1_400_000));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));

        // Add mm_full_autodeploy instructions for each deploy
        for (deployer, auth_id, round_id, amount, squares_mask, checkpoint_round) in &deploys {
            // checkpoint_round_id: if checkpoint needed, use that round; otherwise use current round
            let checkpoint_round_id = checkpoint_round.unwrap_or(*round_id);
            
            instructions.push(mm_full_autodeploy(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
                *round_id,
                checkpoint_round_id,
                *amount,
                *squares_mask,
            ));
        }
        
        // Build versioned transaction with multiple LUTs
        let tx = registry.build_versioned_tx(payer, instructions, lut_accounts, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        // Log transaction size and account count
        let tx_bytes = bincode::serialize(&tx).unwrap_or_default();
        let account_count = match &tx.message {
            solana_sdk::message::VersionedMessage::V0(msg) => {
                msg.account_keys.len() + 
                msg.address_table_lookups.iter().map(|l| l.writable_indexes.len() + l.readonly_indexes.len()).sum::<usize>()
            }
            solana_sdk::message::VersionedMessage::Legacy(msg) => msg.account_keys.len(),
        };
        info!("Sending versioned tx: {} bytes (limit 1232), {} accounts (limit 64)", tx_bytes.len(), account_count);
        
        let signature = tx.signatures[0].to_string();
        
        // Record in database
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        for (deployer, auth_id, round_id, amount, squares_mask, _) in &deploys {
            let num_squares = squares_mask.count_ones();
            let total_deployed = amount * num_squares as u64;
            let bps_fee_amount = total_deployed * deployer.bps_fee / 10_000;
            let deployer_fee = bps_fee_amount + deployer.flat_fee;
            
            db::insert_tx(
                &self.db_pool,
                &signature,
                &deployer.manager_address.to_string(),
                &deployer.deployer_address.to_string(),
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
                num_squares,
                total_deployed,
                deployer_fee,
                DEPLOY_FEE,
                self.config.priority_fee,
                0, // No Jito tip
                last_valid_blockheight,
                now,
            ).await.ok();
        }
        
        // Send transaction
        match self.sender.send_and_confirm_versioned_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ Multi-LUT autodeploy ({} deploys, {} LUTs) confirmed: {}", 
                    deploys.len(), registry.get_luts_for_miners(&miner_auths).len(), sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ Multi-LUT autodeploy failed: {}", e);
                for _ in &deploys {
                    db::update_tx_failed(&self.db_pool, &signature, &e.to_string())
                        .await
                        .ok();
                }
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
    
    /// Execute batched checkpoint+recycle using versioned transaction with LUT
    pub async fn execute_batched_checkpoint_recycle_versioned(
        &self,
        lut_manager: &LutManager,
        checkpoints: Vec<(&DeployerInfo, u64, u64)>, // (deployer, auth_id, checkpoint_round)
    ) -> Result<String, CrankError> {
        if checkpoints.is_empty() {
            return Err(CrankError::Send("No checkpoints to batch".to_string()));
        }
        
        let payer = &self.deploy_authority;
        
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // ~150k CU per checkpoint+recycle
        let cu_limit = (checkpoints.len() as u32 * 150_000).min(1_400_000);
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(cu_limit));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Add checkpoint + recycle for each
        for (deployer, auth_id, checkpoint_round) in &checkpoints {
            instructions.push(mm_autocheckpoint(
                payer.pubkey(),
                deployer.manager_address,
                *checkpoint_round,
                *auth_id,
            ));
            instructions.push(recycle_sol(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
            ));
        }
        
        // Build versioned transaction with LUT
        let tx = lut_manager.build_versioned_tx(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        match self.sender.send_and_confirm_versioned_rpc(&tx, 60).await {
            Ok(sig) => Ok(sig.to_string()),
            Err(e) => Err(CrankError::Send(e.to_string())),
        }
    }
    
    /// Execute batched autodeploys using versioned transaction with LUT
    /// Combines checkpoint+recycle+deploy in one transaction (max ~5 deployers)
    pub async fn execute_batched_autodeploys_versioned(
        &self,
        lut_manager: &LutManager,
        deploys: Vec<(&DeployerInfo, u64, u64, u64, u32, Option<u64>)>, // (deployer, auth_id, round_id, amount, mask, checkpoint_round)
    ) -> Result<String, CrankError> {
        if deploys.is_empty() {
            return Err(CrankError::Send("No deploys to batch".to_string()));
        }
        
        let payer = &self.deploy_authority;
        
        let (recent_blockhash, last_valid_blockheight) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(1_400_000));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Add checkpoint + recycle instructions for deployers that need it
        for (deployer, auth_id, _, _, _, checkpoint_round) in &deploys {
            if let Some(cp_round) = checkpoint_round {
                instructions.push(mm_autocheckpoint(
                    payer.pubkey(),
                    deployer.manager_address,
                    *cp_round,
                    *auth_id,
                ));
                instructions.push(recycle_sol(
                    payer.pubkey(),
                    deployer.manager_address,
                    *auth_id,
                ));
            }
        }
        
        // Add all deploy instructions (mm_autodeploy with LUT compression)
        for (deployer, auth_id, round_id, amount, squares_mask, _) in &deploys {
            instructions.push(mm_autodeploy(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
            ));
        }
        
        // Build versioned transaction with LUT
        let tx = lut_manager.build_versioned_tx(payer, instructions, recent_blockhash)
            .map_err(|e| CrankError::Send(e.to_string()))?;
        
        let signature = tx.signatures[0].to_string();
        
        // Record in database
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        for (deployer, auth_id, round_id, amount, squares_mask, _) in &deploys {
            let num_squares = squares_mask.count_ones();
            let total_deployed = amount * num_squares as u64;
            let bps_fee_amount = total_deployed * deployer.bps_fee / 10_000;
            let deployer_fee = bps_fee_amount + deployer.flat_fee;
            
            db::insert_tx(
                &self.db_pool,
                &signature,
                &deployer.manager_address.to_string(),
                &deployer.deployer_address.to_string(),
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
                num_squares,
                total_deployed,
                deployer_fee,
                DEPLOY_FEE,
                self.config.priority_fee,
                0, // No Jito tip
                last_valid_blockheight,
                now,
            ).await.ok();
        }
        
        // Send versioned transaction
        match self.sender.send_and_confirm_versioned_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ Versioned autodeploy ({} deploys with LUT) confirmed: {}", deploys.len(), sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ Versioned autodeploy failed: {}", e);
                for _ in &deploys {
                    db::update_tx_failed(&self.db_pool, &signature, &e.to_string())
                        .await
                        .ok();
                }
                Err(CrankError::Send(e.to_string()))
            }
        }
    }
}

use std::str::FromStr;

#[derive(Debug, thiserror::Error)]
pub enum CrankError {
    #[error("Failed to load keypair: {0}")]
    KeypairLoad(String),
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("Deserialize error: {0}")]
    Deserialize(String),
    #[error("Database error: {0}")]
    Database(String),
    #[error("Send error: {0}")]
    Send(String),
    #[error("Parse error: {0}")]
    Parse(String),
}
