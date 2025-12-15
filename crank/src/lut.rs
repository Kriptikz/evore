//! Address Lookup Table (LUT) management
//!
//! Manages LUTs for efficient transaction packing with many accounts.
//! 
//! Architecture:
//! - One shared LUT for static accounts (10 accounts that never change)
//! - One LUT per miner containing their 7 specific accounts
//! - Round address is NOT in any LUT (changes each round, can't remove from LUT)
//!
//! The LutRegistry tracks:
//! - shared_lut: The shared LUT address for static accounts
//! - miner_luts: HashMap<miner_auth_pda, lut_address> for quick lookup

use evore::{
    ore_api::{board_pda, config_pda, miner_pda, automation_pda, PROGRAM_ID as ORE_PROGRAM_ID, TREASURY_ADDRESS},
    entropy_api::{self, PROGRAM_ID as ENTROPY_PROGRAM_ID},
    state::{deployer_pda, managed_miner_auth_pda},
    consts::FEE_COLLECTOR,
};
use solana_sdk::address_lookup_table::{
    instruction::{create_lookup_table, extend_lookup_table, deactivate_lookup_table, close_lookup_table},
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
use std::collections::{HashMap, HashSet};
use tracing::{info, debug, warn};

/// Get the static shared accounts (accounts that don't change between rounds)
/// These are shared by mm_autodeploy, mm_autocheckpoint, and recycle_sol instructions.
///
/// Total: 10 fixed accounts (including deploy authority)
/// - deploy_authority: the crank's signer (appears in every tx)
/// - evore_program: the Evore program ID (called by every instruction)
/// - system_program, ore_program, entropy_program: program IDs
/// - FEE_COLLECTOR: protocol fee destination
/// - board_address: ORE board state  
/// - config_address: ORE config
/// - treasury_address: ORE treasury (used in checkpoint)
/// - entropy_var_address: entropy randomness source (derived from board)
pub fn get_static_shared_accounts(deploy_authority: Pubkey) -> Vec<Pubkey> {
    let (board_address, _) = board_pda();
    let (config_address, _) = config_pda();
    // entropy_var is derived from board_address with id=0
    let (entropy_var_address, _) = entropy_api::var_pda(board_address, 0);
    
    vec![
        deploy_authority,      // The crank's signer - included in every instruction
        evore::id(),           // The Evore program - called by every instruction
        system_program::id(),
        ORE_PROGRAM_ID,
        ENTROPY_PROGRAM_ID,
        FEE_COLLECTOR,
        board_address,
        config_address,
        TREASURY_ADDRESS,
        entropy_var_address,
    ]
}

/// Get the 5 accounts specific to a miner (for per-miner LUT)
/// These are derived from manager + auth_id
pub fn get_miner_accounts(manager: Pubkey, auth_id: u64) -> Vec<Pubkey> {
    let (deployer_addr, _) = deployer_pda(manager);
    let (managed_miner_auth, _) = managed_miner_auth_pda(manager, auth_id);
    let (ore_miner, _) = miner_pda(managed_miner_auth);
    let (automation, _) = automation_pda(managed_miner_auth);

    vec![
        manager,
        deployer_addr,
        managed_miner_auth,
        ore_miner,
        automation,
    ]
}

/// Get the miner_auth PDA for a manager/auth_id (used as key in miner_luts map)
pub fn get_miner_auth_pda(manager: Pubkey, auth_id: u64) -> Pubkey {
    let (managed_miner_auth, _) = managed_miner_auth_pda(manager, auth_id);
    managed_miner_auth
}

/// LUT status information for validation and cleanup
#[derive(Debug, Clone)]
pub struct LutStatus {
    pub address: Pubkey,
    pub account_count: usize,
    pub deactivation_slot: Option<u64>,
    pub is_shared: bool,
    pub miner_auth: Option<Pubkey>,
    pub is_valid: bool,
    pub validation_error: Option<String>,
}

/// Registry that manages multiple LUTs:
/// - One shared LUT for static accounts
/// - Per-miner LUTs for miner-specific accounts
pub struct LutRegistry {
    rpc_client: RpcClient,
    authority: Pubkey,
    /// The shared LUT containing static accounts
    shared_lut: Option<Pubkey>,
    /// Cached addresses in the shared LUT
    shared_lut_accounts: HashSet<Pubkey>,
    /// Map from miner_auth PDA to their LUT address
    miner_luts: HashMap<Pubkey, Pubkey>,
    /// Cached LUT accounts for quick access
    lut_cache: HashMap<Pubkey, AddressLookupTableAccount>,
}

impl LutRegistry {
    pub fn new(rpc_url: &str, authority: Pubkey) -> Self {
        let rpc_client = RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        );
        
        Self {
            rpc_client,
            authority,
            shared_lut: None,
            shared_lut_accounts: HashSet::new(),
            miner_luts: HashMap::new(),
            lut_cache: HashMap::new(),
        }
    }
    
    /// Get the authority pubkey
    pub fn authority(&self) -> Pubkey {
        self.authority
    }
    
    /// Get the shared LUT address
    pub fn shared_lut(&self) -> Option<Pubkey> {
        self.shared_lut
    }
    
    /// Set the shared LUT address
    pub fn set_shared_lut(&mut self, lut_address: Pubkey) {
        self.shared_lut = Some(lut_address);
    }
    
    /// Get the miner LUTs map
    pub fn miner_luts(&self) -> &HashMap<Pubkey, Pubkey> {
        &self.miner_luts
    }
    
    /// Get miner LUT address for a miner_auth
    pub fn get_miner_lut(&self, miner_auth: &Pubkey) -> Option<&Pubkey> {
        self.miner_luts.get(miner_auth)
    }
    
    /// Load a LUT and cache it
    pub fn load_lut(&mut self, lut_address: Pubkey) -> Result<AddressLookupTableAccount, LutError> {
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
        let lut_account = AddressLookupTableAccount {
            key: lut_address,
            addresses: lookup_table.addresses.to_vec(),
        };
        
        self.lut_cache.insert(lut_address, lut_account.clone());
        
        Ok(lut_account)
    }
    
    /// Load the shared LUT
    pub fn load_shared_lut(&mut self, lut_address: Pubkey) -> Result<AddressLookupTableAccount, LutError> {
        let lut_account = self.load_lut(lut_address)?;
        
        self.shared_lut = Some(lut_address);
        self.shared_lut_accounts.clear();
        for addr in &lut_account.addresses {
            self.shared_lut_accounts.insert(*addr);
        }
        
        info!("Loaded shared LUT {} with {} addresses", lut_address, lut_account.addresses.len());
        Ok(lut_account)
    }
    
    /// Load all LUTs owned by our authority
    /// Returns the count of LUTs found
    pub fn load_all_luts(&mut self) -> Result<usize, LutError> {
        info!("Scanning for LUTs owned by authority {}...", self.authority);
        
        // Get all LUT accounts owned by the AddressLookupTable program
        // and filter by our authority
        let lut_program_id = solana_sdk::address_lookup_table::program::id();
        
        let accounts = self.rpc_client.get_program_accounts_with_config(
            &lut_program_id,
            solana_client::rpc_config::RpcProgramAccountsConfig {
                filters: Some(vec![
                    // Filter by authority (at offset 22 in LUT account data)
                    solana_client::rpc_filter::RpcFilterType::Memcmp(
                        solana_client::rpc_filter::Memcmp::new_base58_encoded(
                            22, // Authority offset in AddressLookupTable
                            self.authority.as_ref(),
                        ),
                    ),
                ]),
                account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        ).map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let num_luts = accounts.len();
        info!("Found {} LUTs owned by authority", num_luts);
        
        for (lut_address, account) in accounts {
            let lookup_table = match AddressLookupTable::deserialize(&account.data) {
                Ok(lt) => lt,
                Err(e) => {
                    warn!("Failed to deserialize LUT {}: {:?}", lut_address, e);
                    continue;
                }
            };
            
            let addresses: Vec<Pubkey> = lookup_table.addresses.to_vec();
            
            // Cache the LUT
            self.lut_cache.insert(lut_address, AddressLookupTableAccount {
                key: lut_address,
                addresses: addresses.clone(),
            });
            
            // Determine if this is the shared LUT or a miner LUT
            // Shared LUT should contain the static shared accounts
            let static_accounts = get_static_shared_accounts(self.authority);
            let has_all_static = static_accounts.iter().all(|acc| addresses.contains(acc));

            if has_all_static && self.shared_lut.is_none() {
                // This looks like the shared LUT
                self.shared_lut = Some(lut_address);
                for addr in &addresses {
                    self.shared_lut_accounts.insert(*addr);
                }
                info!("  Identified shared LUT: {} ({} addresses)", lut_address, addresses.len());
            } else if addresses.len() == 5 {
                // This looks like a miner LUT (5 accounts per miner)
                // miner_auth is at index 2 (after manager, deployer)
                let miner_auth = addresses[2];
                self.miner_luts.insert(miner_auth, lut_address);
                debug!("  Identified miner LUT: {} for miner_auth {}", lut_address, miner_auth);
            } else if addresses.len() == 6 || addresses.len() == 7 {
                // Legacy LUT formats - will be marked invalid
                let miner_auth = if addresses.len() == 6 { addresses[3] } else { addresses[4] };
                debug!("  Legacy miner LUT ({} accounts): {} for miner_auth {} - will be marked invalid",
                    addresses.len(), lut_address, miner_auth);
            } else {
                debug!("  Unknown LUT: {} ({} addresses)", lut_address, addresses.len());
            }
        }
        
        info!("Loaded {} miner LUTs", self.miner_luts.len());
        
        Ok(num_luts)
    }
    
    /// Register a miner LUT (after creating it)
    pub fn register_miner_lut(&mut self, miner_auth: Pubkey, lut_address: Pubkey, addresses: Vec<Pubkey>) {
        self.miner_luts.insert(miner_auth, lut_address);
        self.lut_cache.insert(lut_address, AddressLookupTableAccount {
            key: lut_address,
            addresses,
        });
    }
    
    /// Get missing static addresses from the shared LUT
    pub fn get_missing_shared_addresses(&self) -> Vec<Pubkey> {
        get_static_shared_accounts(self.authority)
            .into_iter()
            .filter(|addr| !self.shared_lut_accounts.contains(addr))
            .collect()
    }
    
    /// Check if a miner has a LUT
    pub fn has_miner_lut(&self, miner_auth: &Pubkey) -> bool {
        self.miner_luts.contains_key(miner_auth)
    }
    
    /// Get LUT accounts for a list of miner_auth PDAs (for building transactions)
    /// Returns the shared LUT + all relevant miner LUTs
    pub fn get_luts_for_miners(&self, miner_auths: &[Pubkey]) -> Vec<AddressLookupTableAccount> {
        let mut luts = Vec::new();
        
        // Always include shared LUT if available
        if let Some(shared_addr) = self.shared_lut {
            if let Some(lut_account) = self.lut_cache.get(&shared_addr) {
                luts.push(lut_account.clone());
            }
        }
        
        // Add miner-specific LUTs
        for miner_auth in miner_auths {
            if let Some(lut_addr) = self.miner_luts.get(miner_auth) {
                if let Some(lut_account) = self.lut_cache.get(lut_addr) {
                    luts.push(lut_account.clone());
                }
            }
        }
        
        luts
    }
    
    /// Create a new LUT instruction
    pub fn create_lut_instruction(&self, recent_slot: u64) -> Result<(Instruction, Pubkey), LutError> {
        let (create_ix, lut_address) = create_lookup_table(
            self.authority,
            self.authority,
            recent_slot,
        );
        
        Ok((create_ix, lut_address))
    }
    
    /// Extend LUT with new addresses
    pub fn extend_lut_instruction(&self, lut_address: Pubkey, new_addresses: Vec<Pubkey>) -> Result<Instruction, LutError> {
        if new_addresses.is_empty() {
            return Err(LutError::NoNewAddresses);
        }
        
        let extend_ix = extend_lookup_table(
            lut_address,
            self.authority,
            Some(self.authority),
            new_addresses,
        );
        
        Ok(extend_ix)
    }
    
    /// Deactivate LUT instruction
    pub fn deactivate_lut_instruction(&self, lut_address: Pubkey) -> Instruction {
        deactivate_lookup_table(lut_address, self.authority)
    }
    
    /// Close LUT instruction
    pub fn close_lut_instruction(&self, lut_address: Pubkey, recipient: Pubkey) -> Instruction {
        close_lookup_table(lut_address, self.authority, recipient)
    }
    
    /// Get deactivation status for a LUT
    pub fn get_deactivation_status(&self, lut_address: Pubkey) -> Result<Option<u64>, LutError> {
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
        Ok(lookup_table.meta.deactivation_slot.into())
    }
    
    /// Build a versioned transaction with multiple LUTs
    pub fn build_versioned_tx(
        &self,
        payer: &Keypair,
        instructions: Vec<Instruction>,
        lut_accounts: Vec<AddressLookupTableAccount>,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction, LutError> {
        let message = V0Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &lut_accounts,
            recent_blockhash,
        ).map_err(|e| LutError::Compile(e.to_string()))?;
        
        let versioned_message = VersionedMessage::V0(message);
        let tx = VersionedTransaction::try_new(versioned_message, &[payer])
            .map_err(|e| LutError::Sign(e.to_string()))?;
        
        Ok(tx)
    }
    
    /// Build a versioned transaction without LUT
    pub fn build_versioned_tx_no_lut(
        payer: &Keypair,
        instructions: Vec<Instruction>,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction, LutError> {
        let message = V0Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &[],
            recent_blockhash,
        ).map_err(|e| LutError::Compile(e.to_string()))?;
        
        let versioned_message = VersionedMessage::V0(message);
        let tx = VersionedTransaction::try_new(versioned_message, &[payer])
            .map_err(|e| LutError::Sign(e.to_string()))?;
        
        Ok(tx)
    }
    
    /// Refresh the cache for a specific LUT
    pub fn refresh_lut_cache(&mut self, lut_address: Pubkey) -> Result<(), LutError> {
        let lut_account = self.load_lut(lut_address)?;

        // Update shared LUT accounts if this is the shared LUT
        if Some(lut_address) == self.shared_lut {
            self.shared_lut_accounts.clear();
            for addr in &lut_account.addresses {
                self.shared_lut_accounts.insert(*addr);
            }
        }

        Ok(())
    }

    /// Get all LUTs owned by authority with their status
    pub fn get_all_luts_with_status(&self) -> Result<Vec<LutStatus>, LutError> {
        let lut_program_id = solana_sdk::address_lookup_table::program::id();

        let accounts = self.rpc_client.get_program_accounts_with_config(
            &lut_program_id,
            solana_client::rpc_config::RpcProgramAccountsConfig {
                filters: Some(vec![
                    solana_client::rpc_filter::RpcFilterType::Memcmp(
                        solana_client::rpc_filter::Memcmp::new_base58_encoded(
                            22,
                            self.authority.as_ref(),
                        ),
                    ),
                ]),
                account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                    encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
                    ..Default::default()
                },
                ..Default::default()
            },
        ).map_err(|e| LutError::Rpc(e.to_string()))?;

        let static_accounts = get_static_shared_accounts(self.authority);
        let mut results = Vec::new();

        for (lut_address, account) in accounts {
            let lookup_table = match AddressLookupTable::deserialize(&account.data) {
                Ok(lt) => lt,
                Err(_) => continue,
            };

            let addresses: Vec<Pubkey> = lookup_table.addresses.to_vec();
            let deactivation_slot: Option<u64> = lookup_table.meta.deactivation_slot.into();

            // Check if this is the shared LUT
            let has_all_static = static_accounts.iter().all(|acc| addresses.contains(acc));
            let is_shared = has_all_static;

            // Check if this is a miner LUT and validate it
            let mut miner_auth = None;
            let mut is_valid = true;
            let mut validation_error = None;

            if !is_shared {
                if addresses.len() == 5 {
                    // Valid per-miner LUT (5 accounts)
                    // miner_auth at index 2, automation at index 4
                    let miner_auth_in_lut = addresses[2];
                    let automation_in_lut = addresses[4];
                    let expected_automation = automation_pda(miner_auth_in_lut).0;

                    if automation_in_lut != expected_automation {
                        is_valid = false;
                        validation_error = Some(format!(
                            "Wrong automation: expected {}, got {}",
                            expected_automation, automation_in_lut
                        ));
                    }
                    miner_auth = Some(miner_auth_in_lut);
                } else if addresses.len() == 6 || addresses.len() == 7 {
                    // Legacy formats
                    is_valid = false;
                    validation_error = Some(format!("Legacy {}-account format", addresses.len()));
                    miner_auth = Some(if addresses.len() == 6 { addresses[3] } else { addresses[4] });
                } else {
                    is_valid = false;
                    validation_error = Some(format!("Unknown LUT format ({} accounts)", addresses.len()));
                }
            }

            results.push(LutStatus {
                address: lut_address,
                account_count: addresses.len(),
                deactivation_slot,
                is_shared,
                miner_auth,
                is_valid,
                validation_error,
            });
        }

        Ok(results)
    }

    /// Get unused/invalid LUTs that should be deactivated
    pub fn get_unused_luts(&self) -> Result<Vec<LutStatus>, LutError> {
        let all_luts = self.get_all_luts_with_status()?;
        Ok(all_luts.into_iter()
            .filter(|lut| !lut.is_valid && lut.deactivation_slot.is_none())
            .collect())
    }

    /// Get LUTs that are deactivating or ready to close
    pub fn get_deactivating_luts(&self) -> Result<Vec<(LutStatus, u64)>, LutError> {
        let all_luts = self.get_all_luts_with_status()?;
        let current_slot = self.rpc_client.get_slot()
            .map_err(|e| LutError::Rpc(e.to_string()))?;

        Ok(all_luts.into_iter()
            .filter_map(|lut| {
                if let Some(deactivation_slot) = lut.deactivation_slot {
                    // Calculate slots remaining (513 slot cooldown)
                    let slots_since = current_slot.saturating_sub(deactivation_slot);
                    let slots_remaining = 513u64.saturating_sub(slots_since);
                    Some((lut, slots_remaining))
                } else {
                    None
                }
            })
            .collect())
    }
}

// Keep the old LutManager for backwards compatibility with existing commands
// This can be deprecated later

/// Legacy LUT Manager (single LUT)
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
    
    pub fn load_lut(&mut self, lut_address: Pubkey) -> Result<AddressLookupTableAccount, LutError> {
        self.lut_address = Some(lut_address);
        
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
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
    
    pub fn create_lut_instruction(&self, recent_slot: u64) -> Result<(Instruction, Pubkey), LutError> {
        let (create_ix, lut_address) = create_lookup_table(
            self.authority,
            self.authority,
            recent_slot,
        );
        
        Ok((create_ix, lut_address))
    }
    
    pub fn set_lut_address(&mut self, lut_address: Pubkey) {
        self.lut_address = Some(lut_address);
    }
    
    pub fn lut_address(&self) -> Option<Pubkey> {
        self.lut_address
    }
    
    pub fn extend_lut_instruction(&self, new_addresses: Vec<Pubkey>) -> Result<Instruction, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        
        if new_addresses.is_empty() {
            return Err(LutError::NoNewAddresses);
        }
        
        let extend_ix = extend_lookup_table(
            lut_address,
            self.authority,
            Some(self.authority),
            new_addresses,
        );
        
        Ok(extend_ix)
    }
    
    pub fn get_missing_static_addresses(&self) -> Vec<Pubkey> {
        get_static_shared_accounts(self.authority)
            .into_iter()
            .filter(|addr| !self.cached_accounts.contains(addr))
            .collect()
    }
    
    pub fn add_to_cache(&mut self, addresses: &[Pubkey]) {
        for addr in addresses {
            self.cached_accounts.insert(*addr);
        }
    }
    
    pub fn deactivate_lut_instruction(&self) -> Result<Instruction, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        Ok(deactivate_lookup_table(lut_address, self.authority))
    }
    
    pub fn close_lut_instruction(&self, recipient: Pubkey) -> Result<Instruction, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        Ok(close_lookup_table(lut_address, self.authority, recipient))
    }
    
    pub fn get_deactivation_status(&self) -> Result<Option<u64>, LutError> {
        let lut_address = self.lut_address.ok_or(LutError::NoLut)?;
        
        let account = self.rpc_client.get_account(&lut_address)
            .map_err(|e| LutError::Rpc(e.to_string()))?;
        
        let lookup_table = AddressLookupTable::deserialize(&account.data)
            .map_err(|e| LutError::Deserialize(format!("{:?}", e)))?;
        
        Ok(lookup_table.meta.deactivation_slot.into())
    }
    
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
    
    pub fn build_versioned_tx_no_lut(
        payer: &Keypair,
        instructions: Vec<Instruction>,
        recent_blockhash: solana_sdk::hash::Hash,
    ) -> Result<VersionedTransaction, LutError> {
        let message = V0Message::try_compile(
            &payer.pubkey(),
            &instructions,
            &[],
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
    #[error("LUT not deactivated yet")]
    NotDeactivated,
    #[error("LUT still in cooldown (deactivated at slot {0}, need to wait ~512 slots)")]
    StillInCooldown(u64),
}
