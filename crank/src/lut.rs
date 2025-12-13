//! Address Lookup Table (LUT) management
//!
//! Manages LUTs for efficient transaction packing with many accounts.

use evore::{
    ore_api::{board_pda, round_pda, miner_pda, automation_pda, config_pda, PROGRAM_ID as ORE_PROGRAM_ID},
    entropy_api::PROGRAM_ID as ENTROPY_PROGRAM_ID,
    state::{autodeploy_balance_pda, deployer_pda, managed_miner_auth_pda},
    consts::FEE_COLLECTOR,
};
use solana_sdk::address_lookup_table::{
    instruction::{create_lookup_table, extend_lookup_table},
    state::AddressLookupTable,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    address_lookup_table::AddressLookupTableAccount,
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::{v0::Message as V0Message, VersionedMessage},
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    system_program,
    transaction::VersionedTransaction,
};
use std::collections::HashSet;
use tracing::{debug, info, warn};

use crate::config::DeployerInfo;

/// Shared accounts that are always included in the LUT
pub fn get_shared_accounts(round_id: u64) -> Vec<Pubkey> {
    let (board_address, _) = board_pda();
    let (round_address, _) = round_pda(round_id);
    let (config_address, _) = config_pda();
    
    // These are the shared accounts used by mm_batched_autodeploy
    vec![
        system_program::id(),
        ORE_PROGRAM_ID,
        ENTROPY_PROGRAM_ID,
        FEE_COLLECTOR,
        board_address,
        round_address,
        config_address,
        evore::id(),
    ]
}

/// Get all accounts needed for a single deployer
pub fn get_deployer_accounts(manager: Pubkey, auth_id: u64) -> Vec<Pubkey> {
    let (deployer_addr, _) = deployer_pda(manager);
    let (autodeploy_balance_addr, _) = autodeploy_balance_pda(deployer_addr);
    let (managed_miner_auth, _) = managed_miner_auth_pda(manager, auth_id);
    let (ore_miner, _) = miner_pda(managed_miner_auth);
    let (automation, _) = automation_pda(ore_miner);
    
    vec![
        manager,
        deployer_addr,
        autodeploy_balance_addr,
        managed_miner_auth,
        ore_miner,
        automation,
    ]
}

/// LUT Manager handles creating, extending, and loading LUTs
pub struct LutManager {
    rpc_client: RpcClient,
    authority: Pubkey,
    lut_address: Option<Pubkey>,
    cached_accounts: HashSet<Pubkey>,
}

impl LutManager {
    pub fn new(rpc_url: &str, authority: Pubkey) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        );
        
        Self {
            rpc_client,
            authority,
            lut_address: None,
            cached_accounts: HashSet::new(),
        }
    }
    
    /// Load an existing LUT from address
    pub fn load_lut(&mut self, lut_address: Pubkey) -> Result<AddressLookupTableAccount, LutError> {
        self.lut_address = Some(lut_address);
        
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
        // Cache the addresses
        self.cached_accounts.clear();
        for addr in lookup_table.addresses.as_ref() {
            self.cached_accounts.insert(*addr);
        }
        
        info!("Loaded LUT {} with {} addresses", lut_address, lookup_table.addresses.len());
        
        Ok(AddressLookupTableAccount {
            key: lut_address,
            addresses: lookup_table.addresses.to_vec(),
        })
    }
    
    /// Get the current LUT as AddressLookupTableAccount
    pub fn get_lut_account(&self) -> Result<AddressLookupTableAccount, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
        Ok(AddressLookupTableAccount {
            key: lut_address,
            addresses: lookup_table.addresses.to_vec(),
        })
    }
    
    /// Create a new LUT and return the instruction
    pub fn create_lut_instruction(&self, recent_slot: u64) -> Result<(Instruction, Pubkey), LutError> {
        let (create_ix, lut_address) = create_lookup_table(
            self.authority,
            self.authority,
            recent_slot,
        );
        
        info!("Created LUT instruction, address will be: {}", lut_address);
        
        Ok((create_ix, lut_address))
    }
    
    /// Set the LUT address after creation
    pub fn set_lut_address(&mut self, lut_address: Pubkey) {
        self.lut_address = Some(lut_address);
        info!("LUT address set to: {}", lut_address);
    }
    
    /// Get the LUT address
    pub fn lut_address(&self) -> Option<Pubkey> {
        self.lut_address
    }
    
    /// Extend LUT with new addresses
    pub fn extend_lut_instruction(&self, new_addresses: Vec<Pubkey>) -> Result<Instruction, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        
        if new_addresses.is_empty() {
            return Err(LutError::NoNewAddresses);
        }
        
        // LUT extension has a max of ~30 addresses per tx due to size limits
        // Caller should chunk if needed
        
        let extend_ix = extend_lookup_table(
            lut_address,
            self.authority,
            Some(self.authority),
            new_addresses,
        );
        
        Ok(extend_ix)
    }
    
    /// Check which addresses are missing from the LUT
    pub fn get_missing_addresses(&self, addresses: &[Pubkey]) -> Vec<Pubkey> {
        addresses.iter()
            .filter(|addr| !self.cached_accounts.contains(addr))
            .cloned()
            .collect()
    }
    
    /// Get addresses needed for a list of deployers that are missing from LUT
    pub fn get_missing_deployer_addresses(
        &self,
        deployers: &[DeployerInfo],
        auth_id: u64,
        round_id: u64,
    ) -> Vec<Pubkey> {
        let mut needed = Vec::new();
        
        // Add shared accounts
        for addr in get_shared_accounts(round_id) {
            if !self.cached_accounts.contains(&addr) {
                needed.push(addr);
            }
        }
        
        // Add deployer-specific accounts
        for deployer in deployers {
            for addr in get_deployer_accounts(deployer.manager_address, auth_id) {
                if !self.cached_accounts.contains(&addr) && !needed.contains(&addr) {
                    needed.push(addr);
                }
            }
        }
        
        needed
    }
    
    /// Update cached accounts after extension
    pub fn add_to_cache(&mut self, addresses: &[Pubkey]) {
        for addr in addresses {
            self.cached_accounts.insert(*addr);
        }
    }
    
    /// Check if an address is in the LUT
    pub fn contains(&self, address: &Pubkey) -> bool {
        self.cached_accounts.contains(address)
    }
    
    /// Build a versioned transaction using the LUT
    pub fn build_versioned_tx(
        &self,
        payer: &Keypair,
        instructions: Vec<Instruction>,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction, LutError> {
        let lut_account = self.get_lut_account()?;
        
        let message = V0Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &[lut_account],
            recent_blockhash,
        ).map_err(|e| LutError::Compile(e.to_string()))?;
        
        let versioned_message = VersionedMessage::V0(message);
        let tx = VersionedTransaction::try_new(versioned_message, &[payer])
            .map_err(|e| LutError::Sign(e.to_string()))?;
        
        Ok(tx)
    }
    
    /// Build a versioned transaction without LUT (for create/extend operations)
    pub fn build_versioned_tx_no_lut(
        payer: &Keypair,
        instructions: Vec<Instruction>,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction, LutError> {
        let message = V0Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &[], // No lookup tables
            recent_blockhash,
        ).map_err(|e| LutError::Compile(e.to_string()))?;
        
        let versioned_message = VersionedMessage::V0(message);
        let tx = VersionedTransaction::try_new(versioned_message, &[payer])
            .map_err(|e| LutError::Sign(e.to_string()))?;
        
        Ok(tx)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LutError {
    #[error("RPC error: {0}")]
    Rpc(String),
    #[error("Deserialize error: {0}")]
    Deserialize(String),
    #[error("No LUT address set")]
    NoLut,
    #[error("No new addresses to add")]
    NoNewAddresses,
    #[error("Message compile error: {0}")]
    Compile(String),
    #[error("Sign error: {0}")]
    Sign(String),
}
