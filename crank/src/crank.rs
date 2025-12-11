//! Core crank logic
//!
//! Finds deployers where we are the deploy_authority and executes autodeploys

use evore::{
    consts::DEPLOY_FEE,
    instruction::mm_autodeploy,
    ore_api::{board_pda, round_pda, Board, Round},
    state::{autodeploy_balance_pda, deployer_pda, Deployer},
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
        
        let sender = TxSender::new(config.helius_api_key.clone(), config.use_jito);
        
        Ok(Self {
            config,
            rpc_client,
            deploy_authority,
            sender,
            db_pool,
        })
    }
    
    /// Find all deployer accounts where we are the deploy_authority
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
        
        for (deployer_address, account) in accounts {
            match Deployer::try_from_bytes(&account.data) {
                Ok(deployer) => {
                    let manager_address = deployer.manager_key;
                    let (autodeploy_balance_address, _) = autodeploy_balance_pda(deployer_address);
                    
                    deployers.push(DeployerInfo {
                        deployer_address,
                        manager_address,
                        autodeploy_balance_address,
                        fee_bps: deployer.fee_bps,
                    });
                    
                    info!(
                        "Found deployer: {} for manager: {} (fee: {} bps)",
                        deployer_address, manager_address, deployer.fee_bps
                    );
                }
                Err(e) => {
                    warn!("Failed to deserialize deployer account {}: {:?}", deployer_address, e);
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
    
    /// Execute an autodeploy for a deployer
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
        let deployer_fee = total_deployed * deployer.fee_bps / 10_000;
        let protocol_fee = DEPLOY_FEE;
        
        info!(
            "Executing autodeploy for manager {} auth_id {} round {} - {} squares, {} lamports each",
            deployer.manager_address, auth_id, round_id, num_squares, amount
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
            deployer.fee_bps,
            recent_blockhash,
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
        
        // Send transaction
        match self.sender.send_all(&tx).await {
            Ok(_) => {
                info!("Sent autodeploy tx: {}", signature);
            }
            Err(e) => {
                error!("Failed to send tx {}: {}", signature, e);
                db::update_tx_failed(&self.db_pool, &signature, &e.to_string())
                    .await
                    .ok();
                return Err(CrankError::Send(e.to_string()));
            }
        }
        
        Ok(signature)
    }
    
    /// Build an autodeploy transaction
    fn build_autodeploy_tx(
        &self,
        deployer: &DeployerInfo,
        auth_id: u64,
        round_id: u64,
        amount: u64,
        squares_mask: u32,
        expected_fee: u64,
        recent_blockhash: Hash,
    ) -> Result<Transaction, CrankError> {
        let payer = &self.deploy_authority;
        
        // Compute budget instruction
        let cu_limit_ix = ComputeBudgetInstruction::set_compute_unit_limit(400_000);
        let cu_price_ix = ComputeBudgetInstruction::set_compute_unit_price(self.config.priority_fee);
        
        // Jito tip instruction
        let tip_ix = system_instruction::transfer(
            &payer.pubkey(),
            &get_random_jito_tip_account(),
            self.config.jito_tip,
        );
        
        // Autodeploy instruction
        let autodeploy_ix = mm_autodeploy(
            payer.pubkey(),
            deployer.manager_address,
            auth_id,
            round_id,
            amount,
            squares_mask,
            expected_fee,
        );
        
        let mut tx = Transaction::new_with_payer(
            &[cu_limit_ix, cu_price_ix, tip_ix, autodeploy_ix],
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
        
        let current_slot = self.rpc_client.get_slot()
            .map_err(|e| CrankError::Rpc(e.to_string()))?;
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        
        for tx in pending_txs {
            let signature = solana_sdk::signature::Signature::from_str(&tx.signature)
                .map_err(|e| CrankError::Parse(e.to_string()))?;
            
            // Check if blockhash has expired
            // Rough estimate: 150 slots from last_valid_blockheight
            let estimated_expiry_slot = tx.last_valid_blockheight as u64;
            if current_slot > estimated_expiry_slot + 150 {
                info!("Transaction {} expired (current slot {} > expiry {})", 
                    tx.signature, current_slot, estimated_expiry_slot);
                db::update_tx_expired(&self.db_pool, &tx.signature)
                    .await
                    .ok();
                continue;
            }
            
            // Check transaction status
            match self.rpc_client.get_signature_status_with_commitment(
                &signature,
                CommitmentConfig::confirmed(),
            ) {
                Ok(Some(result)) => {
                    match result {
                        Ok(()) => {
                            info!("Transaction {} confirmed", tx.signature);
                            // Get slot info
                            let slot = self.rpc_client.get_signature_status(&signature)
                                .ok()
                                .flatten()
                                .map(|_| current_slot);
                            
                            db::update_tx_confirmed(
                                &self.db_pool,
                                &tx.signature,
                                now,
                                slot.unwrap_or(current_slot),
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
                    // Still pending
                    debug!("Transaction {} still pending", tx.signature);
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
