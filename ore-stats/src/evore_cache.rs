//! EVORE Account Caching
//! 
//! Caches all EVORE program accounts (Managers, Deployers, ManagedMinerAuth PDAs)
//! so the frontend can get all read data from the API without its own RPC connection.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use steel::Pubkey;
use tokio::sync::RwLock;

use crate::helius_api::{HeliusApi, ProgramAccountV2, ProgramAccountFilter, GetProgramAccountsV2Options};

// ============================================================================
// EVORE Program Constants
// ============================================================================

/// EVORE program ID
pub const EVORE_PROGRAM_ID: Pubkey = evore::ID;

/// Seeds for PDA derivation
pub const MANAGED_MINER_AUTH: &[u8] = b"managed_miner_auth";
pub const DEPLOYER_SEED: &[u8] = b"deployer";

/// Account discriminators (first byte after 8-byte discriminator)
pub const MANAGER_DISCRIMINATOR: u8 = 100;
pub const DEPLOYER_DISCRIMINATOR: u8 = 101;

/// Account sizes (including 8-byte discriminator)
pub const MANAGER_SIZE: usize = 8 + 32; // discriminator + authority
pub const DEPLOYER_SIZE: usize = 8 + 32 + 32 + 8 + 8 + 8 + 8 + 8; // 112 bytes

// ============================================================================
// EVORE Account Types
// ============================================================================

/// Manager account from EVORE program
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedManager {
    /// The manager account address (PDA)
    pub address: String,
    /// The authority (owner) of this manager
    pub authority: String,
}

/// Deployer account from EVORE program
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedDeployer {
    /// The deployer account address (PDA)
    pub address: String,
    /// The manager account this deployer is for
    pub manager_key: String,
    /// The authority that can execute deploys
    pub deploy_authority: String,
    /// Actual percentage fee in basis points
    pub bps_fee: u64,
    /// Actual flat fee in lamports
    pub flat_fee: u64,
    /// Maximum bps_fee the manager accepts
    pub expected_bps_fee: u64,
    /// Maximum flat_fee the manager accepts
    pub expected_flat_fee: u64,
    /// Maximum lamports to deploy per round (0 = unlimited)
    pub max_per_round: u64,
}

/// ManagedMinerAuth PDA balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAuthBalance {
    /// The ManagedMinerAuth PDA address
    pub address: String,
    /// The manager this auth belongs to
    pub manager: String,
    /// Auth ID (usually 0)
    pub auth_id: u64,
    /// Balance in lamports
    pub balance: u64,
}

// ============================================================================
// EVORE Cache
// ============================================================================

/// Cache for all EVORE program accounts
#[derive(Debug, Default)]
pub struct EvoreCache {
    /// All managers by address
    pub managers: BTreeMap<String, CachedManager>,
    
    /// Manager address → authority mapping for reverse lookup
    pub managers_by_authority: HashMap<String, Vec<String>>,
    
    /// All deployers by address
    pub deployers: BTreeMap<String, CachedDeployer>,
    
    /// Manager address → deployer address
    pub deployer_by_manager: HashMap<String, String>,
    
    /// ManagedMinerAuth PDA address → balance info
    pub auth_balances: HashMap<String, CachedAuthBalance>,
    
    /// Manager address → ManagedMinerAuth PDA address
    pub auth_pda_by_manager: HashMap<String, String>,
    
    /// Last slot when cache was updated
    pub last_updated_slot: u64,
}

impl EvoreCache {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Get all managers for a given authority
    pub fn get_managers_by_authority(&self, authority: &str) -> Vec<&CachedManager> {
        self.managers_by_authority
            .get(authority)
            .map(|addrs| {
                addrs.iter()
                    .filter_map(|addr| self.managers.get(addr))
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// Get deployer for a manager
    pub fn get_deployer_for_manager(&self, manager: &str) -> Option<&CachedDeployer> {
        self.deployer_by_manager
            .get(manager)
            .and_then(|addr| self.deployers.get(addr))
    }
    
    /// Get auth balance for a manager
    pub fn get_auth_balance_for_manager(&self, manager: &str) -> Option<&CachedAuthBalance> {
        self.auth_pda_by_manager
            .get(manager)
            .and_then(|addr| self.auth_balances.get(addr))
    }
    
    /// Update manager in cache
    pub fn upsert_manager(&mut self, manager: CachedManager) {
        // Update authority lookup
        self.managers_by_authority
            .entry(manager.authority.clone())
            .or_default()
            .push(manager.address.clone());
        
        // Deduplicate
        if let Some(addrs) = self.managers_by_authority.get_mut(&manager.authority) {
            addrs.sort();
            addrs.dedup();
        }
        
        self.managers.insert(manager.address.clone(), manager);
    }
    
    /// Update deployer in cache
    pub fn upsert_deployer(&mut self, deployer: CachedDeployer) {
        self.deployer_by_manager.insert(deployer.manager_key.clone(), deployer.address.clone());
        self.deployers.insert(deployer.address.clone(), deployer);
    }
    
    /// Update auth balance in cache
    pub fn upsert_auth_balance(&mut self, auth: CachedAuthBalance) {
        self.auth_pda_by_manager.insert(auth.manager.clone(), auth.address.clone());
        self.auth_balances.insert(auth.address.clone(), auth);
    }
    
    /// Get statistics about the cache
    pub fn stats(&self) -> EvoreCacheStats {
        EvoreCacheStats {
            managers_count: self.managers.len(),
            deployers_count: self.deployers.len(),
            auth_balances_count: self.auth_balances.len(),
            last_updated_slot: self.last_updated_slot,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct EvoreCacheStats {
    pub managers_count: usize,
    pub deployers_count: usize,
    pub auth_balances_count: usize,
    pub last_updated_slot: u64,
}

// ============================================================================
// PDA Derivation
// ============================================================================

/// Derive ManagedMinerAuth PDA address
pub fn managed_miner_auth_pda(manager: &Pubkey, auth_id: u64) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[MANAGED_MINER_AUTH, &manager.to_bytes(), &auth_id.to_le_bytes()],
        &EVORE_PROGRAM_ID,
    )
}

/// Derive Deployer PDA address
pub fn deployer_pda(manager: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[DEPLOYER_SEED, &manager.to_bytes()],
        &EVORE_PROGRAM_ID,
    )
}

// ============================================================================
// Account Parsing
// ============================================================================

/// Parse a Manager account from raw bytes
pub fn parse_manager(address: &str, data: &[u8]) -> Option<CachedManager> {
    // Manager: 8 byte discriminator + 32 byte authority = 40 bytes
    if data.len() < MANAGER_SIZE {
        return None;
    }
    
    // Skip discriminator (8 bytes)
    let authority_bytes: [u8; 32] = data[8..40].try_into().ok()?;
    let authority = Pubkey::from(authority_bytes);
    
    Some(CachedManager {
        address: address.to_string(),
        authority: authority.to_string(),
    })
}

/// Parse a Deployer account from raw bytes
pub fn parse_deployer(address: &str, data: &[u8]) -> Option<CachedDeployer> {
    // Deployer: 8 discriminator + 32 manager + 32 deploy_auth + 8*5 fees = 112 bytes
    if data.len() < DEPLOYER_SIZE {
        return None;
    }
    
    // Skip discriminator (8 bytes)
    let manager_bytes: [u8; 32] = data[8..40].try_into().ok()?;
    let deploy_auth_bytes: [u8; 32] = data[40..72].try_into().ok()?;
    
    let manager_key = Pubkey::from(manager_bytes);
    let deploy_authority = Pubkey::from(deploy_auth_bytes);
    
    // Parse u64 fields
    let bps_fee = u64::from_le_bytes(data[72..80].try_into().ok()?);
    let flat_fee = u64::from_le_bytes(data[80..88].try_into().ok()?);
    let expected_bps_fee = u64::from_le_bytes(data[88..96].try_into().ok()?);
    let expected_flat_fee = u64::from_le_bytes(data[96..104].try_into().ok()?);
    let max_per_round = u64::from_le_bytes(data[104..112].try_into().ok()?);
    
    Some(CachedDeployer {
        address: address.to_string(),
        manager_key: manager_key.to_string(),
        deploy_authority: deploy_authority.to_string(),
        bps_fee,
        flat_fee,
        expected_bps_fee,
        expected_flat_fee,
        max_per_round,
    })
}

// ============================================================================
// Combined AutoMiner Response
// ============================================================================

/// Full data for a user's AutoMiner (combined from multiple accounts)
#[derive(Debug, Clone, Serialize)]
pub struct AutoMinerInfo {
    pub manager: CachedManager,
    pub deployer: Option<CachedDeployer>,
    pub auth_balance: Option<CachedAuthBalance>,
    /// ORE Miner account (if linked)
    pub miner: Option<MinerInfo>,
}

/// Subset of miner info needed for AutoMiner display
#[derive(Debug, Clone, Serialize)]
pub struct MinerInfo {
    pub address: String,
    pub round_id: u64,
    pub checkpoint_id: u64,
    pub deployed: [u64; 25],
    pub rewards_sol: u64,
    pub rewards_ore: u64,
    pub refined_ore: u64,
}

