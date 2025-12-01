use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::time::Duration;

use evore::ore_api::{board_pda, round_pda, Board, Round};
use evore::state::{managed_miner_auth_pda, Manager};
use steel::AccountDeserialize;

pub struct EvoreClient {
    pub rpc: RpcClient,
}

impl EvoreClient {
    pub fn new(rpc_url: &str) -> Self {
        let rpc = RpcClient::new_with_timeout_and_commitment(
            rpc_url.to_string(),
            Duration::from_secs(30),
            CommitmentConfig::confirmed(),
        );
        Self { rpc }
    }

    /// Get current slot
    pub fn get_slot(&self) -> Result<u64, Box<dyn std::error::Error>> {
        Ok(self.rpc.get_slot()?)
    }

    /// Get board state (contains current round_id and end_slot)
    pub fn get_board(&self) -> Result<Board, Box<dyn std::error::Error>> {
        let board_address = board_pda().0;
        let account = self.rpc.get_account(&board_address)?;
        
        // try_from_bytes handles discriminator
        let board = Board::try_from_bytes(&account.data)?;
        Ok(*board)
    }

    /// Get round state
    pub fn get_round(&self, round_id: u64) -> Result<Round, Box<dyn std::error::Error>> {
        let round_address = round_pda(round_id).0;
        let account = self.rpc.get_account(&round_address)?;
        
        // try_from_bytes handles discriminator
        let round = Round::try_from_bytes(&account.data)?;
        Ok(*round)
    }

    /// Send transaction without waiting for confirmation
    /// Returns signature immediately for later confirmation
    pub fn send_transaction_no_wait(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature, Box<dyn std::error::Error>> {
        let signature = self.rpc.send_transaction(transaction)?;
        Ok(signature)
    }

    /// Confirm a transaction signature
    pub fn confirm_transaction(&self, signature: &Signature) -> Result<bool, Box<dyn std::error::Error>> {
        match self.rpc.confirm_transaction(signature) {
            Ok(confirmed) => Ok(confirmed),
            Err(_) => Ok(false),
        }
    }

    /// Get managed miner auth PDA address for a manager and auth_id
    pub fn get_managed_miner_auth_address(manager: &Pubkey, auth_id: u64) -> (Pubkey, u8) {
        managed_miner_auth_pda(*manager, auth_id)
    }
    
    /// Get Manager account data (returns None if account doesn't exist)
    pub fn get_manager(&self, manager_address: &Pubkey) -> Result<Option<Manager>, Box<dyn std::error::Error>> {
        match self.rpc.get_account(manager_address) {
            Ok(account) => {
                let manager = Manager::try_from_bytes(&account.data)?;
                Ok(Some(*manager))
            }
            Err(e) => {
                // Check if it's an "account not found" error
                let err_str = e.to_string();
                if err_str.contains("AccountNotFound") || err_str.contains("could not find account") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
}

/// Display helper for managed miner auth PDA
pub fn print_managed_miner_info(manager: &Pubkey, auth_id: u64) {
    let (pda, bump) = EvoreClient::get_managed_miner_auth_address(manager, auth_id);
    println!("Manager:              {}", manager);
    println!("Auth ID:              {}", auth_id);
    println!("Managed Miner Auth:   {}", pda);
    println!("Bump:                 {}", bump);
    println!();
}

