//! Core crank logic
//!
//! Finds deployers where we are the deploy_authority and executes autodeploys

use evore::{
    consts::DEPLOY_FEE,
    instruction::{mm_autodeploy, mm_autocheckpoint, recycle_sol},
    ore_api::{board_pda, miner_pda, round_pda, Board, Miner, Round},
    state::{autodeploy_balance_pda, managed_miner_auth_pda, Deployer},
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
    lut::LutManager,
    sender::{get_random_jito_tip_account, TxSender},
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
        
        let sender = TxSender::new(
            config.helius_api_key.clone(), 
            config.rpc_url.clone(),
            config.use_jito,
        );
        
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
    
    /// Find all deployer accounts where we are the deploy_authority
    /// This also handles migration from old deployer format to new format
    pub async fn find_deployers(&self) -> Result<Vec<DeployerInfo>, CrankError> {
        let deploy_authority_pubkey = self.deploy_authority.pubkey();
        
        info!("Scanning for deployers with deploy_authority: {}", deploy_authority_pubkey);
        
        // Use getProgramAccounts to find all Deployer accounts
        let accounts = self.rpc_client.get_program_accounts_with_config(
            &evore::id(),
            solana_client::rpc_config::RpcProgramAccountsConfig {
                filters: Some(vec![
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
        
        let mut deployers = Vec::new();
        
        // Old deployer size: 8 discriminator + 32 manager_key + 32 deploy_authority + 8 fee_bps = 80
        const OLD_DEPLOYER_SIZE: usize = 80;
        // New deployer size: 8 discriminator + 32 manager_key + 32 deploy_authority + 8 fee + 8 fee_type = 88
        const NEW_DEPLOYER_SIZE: usize = 88;
        
        for (deployer_address, account) in accounts {
            // First try to parse as new Deployer
            match Deployer::try_from_bytes(&account.data) {
                Ok(deployer) => {
                    let manager_address = deployer.manager_key;
                    let (autodeploy_balance_address, _) = autodeploy_balance_pda(deployer_address);
                    
                    let fee_str = format!("{} bps + {} lamports flat", deployer.bps_fee, deployer.flat_fee);
                    
                    deployers.push(DeployerInfo {
                        deployer_address,
                        manager_address,
                        autodeploy_balance_address,
                        bps_fee: deployer.bps_fee,
                        flat_fee: deployer.flat_fee,
                    });
                    
                    info!(
                        "Found deployer: {} for manager: {} (fee: {})",
                        deployer_address, manager_address, fee_str
                    );
                }
                Err(_) => {
                    // Check if this is an old format deployer that needs migration
                      warn!(
                          "Deployer account {} has unexpected size {} (expected {} or {})",
                          deployer_address, account.data.len(), NEW_DEPLOYER_SIZE, OLD_DEPLOYER_SIZE
                      );
                }
            }
        }
        
        info!("Found {} deployers", deployers.len());
        
        Ok(deployers)
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
    
    /// Get autodeploy balance for a deployer
    pub fn get_autodeploy_balance(&self, deployer: &DeployerInfo) -> Result<u64, CrankError> {
        self.rpc_client.get_balance(&deployer.autodeploy_balance_address)
            .map_err(|e| CrankError::Rpc(e.to_string()))
    }
    
    // Constants matching the program's process_mm_autodeploy.rs
    const AUTH_PDA_RENT: u64 = 890_880;
    const ORE_CHECKPOINT_FEE: u64 = 10_000;
    const ORE_MINER_SIZE: usize = 8 + 584; // discriminator + Miner struct size
    // Rent for the autodeploy_balance PDA (0-byte account)
    const AUTODEPLOY_BALANCE_RENT: u64 = 890_880;
    
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
        
        // Total funds needed from autodeploy_balance
        // IMPORTANT: The autodeploy_balance PDA needs to stay rent-exempt after transfers
        let total_needed = transfer_to_miner
            .saturating_add(deployer_fee)
            .saturating_add(protocol_fee)
            .saturating_add(Self::AUTODEPLOY_BALANCE_RENT); // Keep PDA rent-exempt
        
        info!(
            "Required balance: deploy={}, deployer_fee={}, protocol_fee={}, transfer_to_miner={} (auth_balance={}, miner_rent={}), autodeploy_rent={}, total={}",
            total_deployed, deployer_fee, protocol_fee, transfer_to_miner, current_auth_balance, miner_rent, Self::AUTODEPLOY_BALANCE_RENT, total_needed
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
        // - auth rent + checkpoint fee + miner rent + autodeploy_balance rent
        const MAX_OVERHEAD: u64 = 890_880 + 10_000 + 2_500_000 + 890_880; // ~0.0043 SOL
        
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
    
    /// Execute checkpoint and recycle only (no deploy)
    /// Use this when balance is too low to deploy but we still want to claim winnings
    pub async fn execute_checkpoint_recycle(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        checkpoint_round: u64,
    ) -> Result<String, CrankError> {
        info!(
            "Executing checkpoint+recycle for manager {} auth_id {} (checkpointing round {})",
            deployer.manager_address, auth_id, checkpoint_round
        );
        
        let payer = &self.deploy_authority;
        
        // Get recent blockhash
        let (recent_blockhash, _) = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let mut instructions = Vec::new();
        
        // ~150k CU for checkpoint + recycle
        instructions.push(ComputeBudgetInstruction::set_compute_unit_limit(200_000));
        instructions.push(ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee));
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
        // Checkpoint
        instructions.push(mm_autocheckpoint(
            payer.pubkey(),
            deployer.manager_address,
            checkpoint_round,
            auth_id,
        ));
        
        // Recycle
        instructions.push(recycle_sol(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
        ));
        
        let mut tx = Transaction::new_with_payer(&instructions, Some(&payer.pubkey()));
        tx.sign(&[payer], recent_blockhash);
        
        let signature = tx.signatures[0].to_string();
        
        match self.sender.send_and_confirm_rpc(&tx, 60).await {
            Ok(sig) => {
                info!("✓ Checkpoint+recycle confirmed: {}", sig);
                Ok(sig.to_string())
            }
            Err(e) => {
                error!("✗ Checkpoint+recycle failed: {}", e);
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
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
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
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
        // Just the deploy (no checkpoint)
        instructions.push(mm_autodeploy(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
            round_id,
            amount,
            squares_mask,
            deployer.bps_fee,
            deployer.flat_fee,
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
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
        // Add all deploys (no checkpoint)
        for (deployer, auth_id, round_id, amount, squares_mask) in &deploys {
            instructions.push(mm_autodeploy(
                payer.pubkey(),
                deployer.manager_address,
                *auth_id,
                *round_id,
                *amount,
                *squares_mask,
                deployer.bps_fee,
                deployer.flat_fee,
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
        
        // Jito tip (once for the whole batch)
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
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
                deployer.bps_fee,
                deployer.flat_fee,
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
                self.config.jito_tip / deploys.len() as u64, // Split tip across deploys
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
            self.config.jito_tip,
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
        
        // Jito tip instruction
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
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
            deployer.bps_fee,
            deployer.flat_fee,
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
    
    /// Extend LUT with deployer accounts
    pub async fn extend_lut_with_deployers(
        &self,
        lut_manager: &mut LutManager,
        deployers: &[DeployerInfo],
        auth_id: u64,
        round_id: u64,
    ) -> Result<usize, CrankError> {
        let missing = lut_manager.get_missing_deployer_addresses(deployers, auth_id, round_id);
        
        if missing.is_empty() {
            return Ok(0);
        }
        
        info!("Adding {} addresses to LUT", missing.len());
        
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
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
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
        
        // Jito tip
        instructions.push(system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        ));
        
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
                deployer.bps_fee,
                deployer.flat_fee,
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
                self.config.jito_tip / deploys.len() as u64,
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
