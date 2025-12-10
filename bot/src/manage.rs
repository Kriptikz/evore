//! Manage Module - Account discovery and miner management
//!
//! Provides functionality for:
//! - Loading signer keypairs from a directory
//! - Discovering manager accounts by authority
//! - Discovering miner accounts for each manager
//! - Supporting legacy (secondary) program miners

use std::path::{Path, PathBuf};
use std::sync::Arc;

use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_filter::{Memcmp, MemcmpEncodedBytes, RpcFilterType};
use solana_sdk::commitment_config::CommitmentConfig;
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::{read_keypair_file, Keypair};

use evore::ore_api::{miner_pda, Miner};
use evore::state::Manager;
use steel::AccountDeserialize;

use crate::config::ManageConfig;

/// Current Evore program ID
pub const EVORE_PROGRAM_ID: Pubkey = evore::ID;

/// Extension trait for ManageConfig
pub trait ManageConfigExt {
    fn get_secondary_program_id(&self) -> Option<Pubkey>;
}

impl ManageConfigExt for ManageConfig {
    /// Parse secondary program ID as Pubkey
    fn get_secondary_program_id(&self) -> Option<Pubkey> {
        self.secondary_program_id.as_ref().and_then(|s| s.parse().ok())
    }
}

/// A discovered miner with its associated data
#[derive(Debug, Clone)]
pub struct DiscoveredMiner {
    /// The miner's authority PDA (Evore auth_pda)
    pub authority_pda: Pubkey,
    /// The ORE miner PDA
    pub miner_pda: Pubkey,
    /// The miner account data
    pub miner: Miner,
    /// The manager pubkey this miner belongs to
    pub manager: Pubkey,
    /// Auth ID used to derive this miner
    pub auth_id: u64,
    /// The signer (authority) pubkey that controls this manager
    pub signer: Pubkey,
    /// Program ID (current or legacy)
    pub program_id: Pubkey,
    /// Whether this is a legacy miner (from secondary program)
    pub is_legacy: bool,
    /// SOL balance of the authority PDA
    pub auth_pda_balance: u64,
}

impl DiscoveredMiner {
    /// Check if miner needs checkpoint (round_id > checkpoint_id)
    pub fn needs_checkpoint(&self) -> bool {
        self.miner.round_id > self.miner.checkpoint_id
    }
    
    /// Get claimable SOL in lamports
    pub fn claimable_sol(&self) -> u64 {
        self.miner.rewards_sol
    }
    
    /// Get claimable ORE (raw units)
    pub fn claimable_ore(&self) -> u64 {
        self.miner.rewards_ore
    }
}

/// Account discovery result
#[derive(Debug, Clone)]
pub struct DiscoveryResult {
    /// Loaded signers
    pub signers: Vec<(Pubkey, PathBuf)>,
    /// Discovered managers (signer -> managers)
    pub managers: Vec<(Pubkey, Pubkey)>, // (signer, manager)
    /// Discovered miners
    pub miners: Vec<DiscoveredMiner>,
    /// Legacy miners (from secondary program)
    pub legacy_miners: Vec<DiscoveredMiner>,
}

impl DiscoveryResult {
    pub fn new() -> Self {
        Self {
            signers: Vec::new(),
            managers: Vec::new(),
            miners: Vec::new(),
            legacy_miners: Vec::new(),
        }
    }
    
    /// Total miner count (current + legacy)
    pub fn total_miners(&self) -> usize {
        self.miners.len() + self.legacy_miners.len()
    }
}

/// Load all signer keypairs from a directory
pub fn load_signers_from_directory(dir: &Path) -> Result<Vec<(Arc<Keypair>, PathBuf)>, String> {
    if !dir.exists() {
        return Err(format!("Signers directory does not exist: {:?}", dir));
    }
    
    if !dir.is_dir() {
        return Err(format!("Signers path is not a directory: {:?}", dir));
    }
    
    let mut signers = Vec::new();
    
    // Read all .json files in directory
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {:?}: {}", dir, e))?;
    
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let path = entry.path();
        
        // Only process .json files
        if path.extension().map_or(false, |ext| ext == "json") {
            match read_keypair_file(&path) {
                Ok(keypair) => {
                    signers.push((Arc::new(keypair), path));
                }
                Err(e) => {
                    // Log but continue - might not be a keypair file
                    eprintln!("Warning: Failed to load keypair from {:?}: {}", path, e);
                }
            }
        }
    }
    
    Ok(signers)
}

/// Get managers where the given signer is the authority
/// Uses getProgramAccounts with memcmp filter on authority field (offset 8)
pub fn get_managers_by_authority(
    rpc: &RpcClient,
    signer: &Pubkey,
    program_id: &Pubkey,
) -> Result<Vec<(Pubkey, Manager)>, String> {
    // Manager account layout:
    // Offset 0-7: Discriminator (8 bytes)
    // Offset 8-39: Authority pubkey (32 bytes)
    let filters = vec![
        RpcFilterType::Memcmp(Memcmp::new(
            8, // Skip discriminator
            MemcmpEncodedBytes::Base58(signer.to_string()),
        )),
    ];
    
    let config = RpcProgramAccountsConfig {
        filters: Some(filters),
        account_config: RpcAccountInfoConfig {
            encoding: Some(solana_account_decoder::UiAccountEncoding::Base64),
            commitment: Some(CommitmentConfig::confirmed()),
            ..Default::default()
        },
        ..Default::default()
    };
    
    let accounts = rpc
        .get_program_accounts_with_config(program_id, config)
        .map_err(|e| format!("Failed to get program accounts: {}", e))?;
    
    let mut managers = Vec::new();
    for (pubkey, account) in accounts {
        // Try to deserialize as Manager
        if let Ok(manager) = Manager::try_from_bytes(&account.data) {
            managers.push((pubkey, manager.clone()));
        }
    }
    
    Ok(managers)
}

/// Seed for managed miner auth PDA
const MANAGED_MINER_AUTH: &[u8] = b"managed-miner-auth";

/// Calculate managed miner auth PDA for a manager and auth_id
/// This is the authority PDA that owns the ORE miner account
pub fn managed_miner_auth_pda(manager: &Pubkey, auth_id: u64, program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, manager.as_ref(), &auth_id.to_le_bytes()],
        program_id,
    ).0
}

/// Get all miners for a manager (iterates auth_id from 1 until not found)
pub fn get_miners_for_manager(
    rpc: &RpcClient,
    manager: &Pubkey,
    signer: &Pubkey,
    program_id: &Pubkey,
    is_legacy: bool,
) -> Result<Vec<DiscoveredMiner>, String> {
    let mut miners = Vec::new();
    
    for auth_id in 1u64.. {
        // Calculate the managed miner auth PDA (this is the authority for the ORE miner)
        let auth_pda = managed_miner_auth_pda(manager, auth_id, program_id);
        // Then get the ORE miner PDA using the auth PDA as authority
        let (ore_miner_pda, _) = miner_pda(auth_pda);
        
        // Try to fetch the miner account
        match rpc.get_account(&ore_miner_pda) {
            Ok(account) => {
                // Try to deserialize as Miner
                if let Ok(miner) = Miner::try_from_bytes(&account.data) {
                    // Fetch the auth PDA balance
                    let auth_pda_balance = rpc.get_balance(&auth_pda).unwrap_or(0);
                    
                    miners.push(DiscoveredMiner {
                        authority_pda: auth_pda,
                        miner_pda: ore_miner_pda,
                        miner: miner.clone(),
                        manager: *manager,
                        auth_id,
                        signer: *signer,
                        program_id: *program_id,
                        is_legacy,
                        auth_pda_balance,
                    });
                } else {
                    // Invalid data, stop iteration
                    break;
                }
            }
            Err(_) => {
                // Account not found, stop iteration
                break;
            }
        }
    }
    
    Ok(miners)
}

/// Discover all accounts (signers, managers, miners)
pub fn discover_accounts(
    rpc: &RpcClient,
    config: &ManageConfig,
) -> Result<DiscoveryResult, String> {
    use solana_sdk::signer::Signer;
    
    let mut result = DiscoveryResult::new();
    
    // Load signers
    let signers_path = config.signers_path.as_ref()
        .ok_or("No signers_path configured")?;
    
    let signers = load_signers_from_directory(signers_path)?;
    
    if signers.is_empty() {
        return Err("No signer keypairs found in directory".to_string());
    }
    
    // Store signer pubkeys
    result.signers = signers.iter()
        .map(|(kp, path)| (kp.pubkey(), path.clone()))
        .collect();
    
    // For each signer, find managers
    for (signer_keypair, _path) in &signers {
        let signer_pubkey = signer_keypair.pubkey();
        
        // Get managers for current program
        match get_managers_by_authority(rpc, &signer_pubkey, &EVORE_PROGRAM_ID) {
            Ok(managers) => {
                for (manager_pubkey, _manager) in managers {
                    result.managers.push((signer_pubkey, manager_pubkey));
                    
                    // Get miners for this manager
                    match get_miners_for_manager(rpc, &manager_pubkey, &signer_pubkey, &EVORE_PROGRAM_ID, false) {
                        Ok(miners) => {
                            result.miners.extend(miners);
                        }
                        Err(e) => {
                            eprintln!("Warning: Failed to get miners for manager {}: {}", manager_pubkey, e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Warning: Failed to get managers for signer {}: {}", signer_pubkey, e);
            }
        }
        
        // Get managers for legacy program (if configured)
        if let Some(legacy_program_id) = config.get_secondary_program_id() {
            match get_managers_by_authority(rpc, &signer_pubkey, &legacy_program_id) {
                Ok(managers) => {
                    for (manager_pubkey, _manager) in managers {
                        // Don't duplicate in managers list if same manager
                        if !result.managers.iter().any(|(_, m)| *m == manager_pubkey) {
                            result.managers.push((signer_pubkey, manager_pubkey));
                        }
                        
                        // Get legacy miners for this manager
                        match get_miners_for_manager(rpc, &manager_pubkey, &signer_pubkey, &legacy_program_id, true) {
                            Ok(miners) => {
                                result.legacy_miners.extend(miners);
                            }
                            Err(e) => {
                                eprintln!("Warning: Failed to get legacy miners for manager {}: {}", manager_pubkey, e);
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Warning: Failed to get legacy managers for signer {}: {}", signer_pubkey, e);
                }
            }
        }
    }
    
    Ok(result)
}

/// Get the signer keypair for a miner from loaded signers
pub fn get_signer_for_miner<'a>(
    miner: &DiscoveredMiner,
    signers: &'a [(Arc<Keypair>, PathBuf)],
) -> Option<&'a Arc<Keypair>> {
    use solana_sdk::signer::Signer;
    signers.iter()
        .find(|(kp, _)| kp.pubkey() == miner.signer)
        .map(|(kp, _)| kp)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_manage_config_default() {
        let config = ManageConfig::default();
        assert!(!config.is_valid());
        assert!(config.get_secondary_program_id().is_none());
    }
    
    #[test]
    fn test_managed_miner_auth_pda() {
        let manager = Pubkey::new_unique();
        let auth_id = 1u64;
        let pda = managed_miner_auth_pda(&manager, auth_id, &EVORE_PROGRAM_ID);
        
        // Verify it's deterministic
        let pda2 = managed_miner_auth_pda(&manager, auth_id, &EVORE_PROGRAM_ID);
        assert_eq!(pda, pda2);
        
        // Different auth_id should give different PDA
        let pda3 = managed_miner_auth_pda(&manager, 2, &EVORE_PROGRAM_ID);
        assert_ne!(pda, pda3);
    }
}
