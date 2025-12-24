//! Miner account cache to reduce RPC calls
//!
//! Caches ORE miner account data in RAM, refreshing only after deployments
//! or when a new round is detected.

use evore::ore_api::{miner_pda, Miner};
use evore::state::managed_miner_auth_pda;
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use steel::AccountDeserialize;
use tracing::{debug, info, warn};

use crate::config::DeployerInfo;
use crate::crank::CrankError;

/// Cached miner data
#[derive(Debug, Clone)]
pub struct CachedMiner {
    /// The ORE miner PDA address
    pub miner_address: Pubkey,
    /// The managed_miner_auth PDA (authority of the miner)
    pub authority: Pubkey,
    /// The deployer address this miner belongs to
    pub deployer_address: Pubkey,
    /// Manager address
    pub manager_address: Pubkey,
    /// Last checkpointed round
    pub checkpoint_id: u64,
    /// Round the miner last deployed in
    pub round_id: u64,
    /// Whether miner has deployed in round_id (sum of deployed > 0)
    pub has_deployed: bool,
    /// Balance of the managed_miner_auth PDA
    pub auth_balance: u64,
    /// SOL rewards available in the ORE miner account
    pub rewards_sol: u64,
    /// Whether the miner account exists
    pub exists: bool,
}

/// Miner cache for reducing RPC calls
pub struct MinerCache {
    /// Cached miner data keyed by miner PDA address
    miners: HashMap<Pubkey, CachedMiner>,
    /// The round_id when we last refreshed
    last_refresh_round: Option<u64>,
    /// Whether we need to refresh all balances
    needs_balance_refresh: bool,
}

impl MinerCache {
    pub fn new() -> Self {
        Self {
            miners: HashMap::new(),
            last_refresh_round: None,
            needs_balance_refresh: true,
        }
    }

    /// Get cached miner data
    pub fn get(&self, miner_address: &Pubkey) -> Option<&CachedMiner> {
        self.miners.get(miner_address)
    }

    /// Get cached miner by deployer address
    pub fn get_by_deployer(&self, deployer_address: &Pubkey) -> Option<&CachedMiner> {
        self.miners.values().find(|m| &m.deployer_address == deployer_address)
    }

    /// Check if a miner has already deployed in the given round
    pub fn has_deployed_in_round(&self, miner_address: &Pubkey, round_id: u64) -> bool {
        self.miners.get(miner_address)
            .map(|m| m.exists && m.round_id == round_id && m.has_deployed)
            .unwrap_or(false)
    }

    /// Check if miner needs checkpoint (checkpoint_id < round_id)
    pub fn needs_checkpoint(&self, miner_address: &Pubkey) -> Option<u64> {
        self.miners.get(miner_address).and_then(|m| {
            if m.exists && m.checkpoint_id < m.round_id {
                Some(m.round_id)
            } else {
                None
            }
        })
    }

    /// Get cached balance for a miner's auth PDA
    pub fn get_balance(&self, miner_address: &Pubkey) -> Option<u64> {
        self.miners.get(miner_address).map(|m| m.auth_balance)
    }

    /// Check if miner has SOL rewards to recycle
    pub fn has_sol_to_recycle(&self, miner_address: &Pubkey) -> bool {
        self.miners.get(miner_address)
            .map(|m| m.exists && m.rewards_sol > 0)
            .unwrap_or(false)
    }

    /// Mark that balances need refreshing (call after deployment)
    pub fn invalidate_balances(&mut self) {
        self.needs_balance_refresh = true;
    }

    /// Mark specific miners as deployed (after successful deploy)
    pub fn mark_deployed(&mut self, miner_addresses: &[Pubkey], round_id: u64) {
        for addr in miner_addresses {
            if let Some(miner) = self.miners.get_mut(addr) {
                miner.round_id = round_id;
                miner.has_deployed = true;
            }
        }
        // Balance will have changed after deploy
        self.needs_balance_refresh = true;
    }

    /// Refresh cache using batch RPC calls
    /// Returns the number of miners fetched
    pub fn refresh(
        &mut self,
        rpc_client: &RpcClient,
        deployers: &[DeployerInfo],
        auth_id: u64,
        current_round_id: u64,
    ) -> Result<usize, CrankError> {
        let is_new_round = self.last_refresh_round.map_or(true, |r| r != current_round_id);
        
        // Build list of addresses to fetch
        let mut miner_addresses: Vec<Pubkey> = Vec::new();
        let mut auth_addresses: Vec<Pubkey> = Vec::new();
        let mut deployer_map: HashMap<Pubkey, &DeployerInfo> = HashMap::new();
        
        for deployer in deployers {
            let (auth_pda, _) = managed_miner_auth_pda(deployer.manager_address, auth_id);
            let (miner_addr, _) = miner_pda(auth_pda);
            
            miner_addresses.push(miner_addr);
            auth_addresses.push(auth_pda);
            deployer_map.insert(miner_addr, deployer);
        }

        // Only refresh if new round or balances invalidated
        if !is_new_round && !self.needs_balance_refresh && !self.miners.is_empty() {
            debug!("Cache still valid, skipping refresh");
            return Ok(self.miners.len());
        }

        info!(
            "Refreshing miner cache: {} miners, new_round={}, balance_refresh={}",
            deployers.len(), is_new_round, self.needs_balance_refresh
        );

        // Batch fetch all miner accounts (up to 100 per call)
        let mut fetched_count = 0;
        
        for (chunk_idx, (miner_chunk, auth_chunk)) in miner_addresses
            .chunks(100)
            .zip(auth_addresses.chunks(100))
            .enumerate()
        {
            // Fetch miner accounts
            let miner_accounts = rpc_client
                .get_multiple_accounts(miner_chunk)
                .map_err(|e| CrankError::Rpc(format!("Failed to fetch miners: {}", e)))?;

            // Fetch auth PDA accounts to get lamport balances (batch)
            let auth_accounts = rpc_client
                .get_multiple_accounts(auth_chunk)
                .map_err(|e| CrankError::Rpc(format!("Failed to fetch auth accounts: {}", e)))?;
            
            // Extract lamport balances from auth accounts
            let auth_balances: Vec<u64> = auth_accounts
                .iter()
                .map(|acc| acc.as_ref().map(|a| a.lamports).unwrap_or(0))
                .collect();

            // Process results
            for (i, (miner_account, balance)) in miner_accounts.iter().zip(auth_balances.iter()).enumerate() {
                let global_idx = chunk_idx * 100 + i;
                let miner_address = miner_addresses[global_idx];
                let auth_address = auth_addresses[global_idx];
                let deployer = deployer_map.get(&miner_address).unwrap();

                let cached = if let Some(account) = miner_account {
                    // Parse miner data
                    match Miner::try_from_bytes(&account.data) {
                        Ok(miner) => {
                            let has_deployed = miner.deployed.iter().any(|&d| d > 0);
                            CachedMiner {
                                miner_address,
                                authority: auth_address,
                                deployer_address: deployer.deployer_address,
                                manager_address: deployer.manager_address,
                                checkpoint_id: miner.checkpoint_id,
                                round_id: miner.round_id,
                                has_deployed,
                                auth_balance: *balance,
                                rewards_sol: miner.rewards_sol,
                                exists: true,
                            }
                        }
                        Err(e) => {
                            warn!("Failed to parse miner {}: {:?}", miner_address, e);
                            CachedMiner {
                                miner_address,
                                authority: auth_address,
                                deployer_address: deployer.deployer_address,
                                manager_address: deployer.manager_address,
                                checkpoint_id: 0,
                                round_id: 0,
                                has_deployed: false,
                                auth_balance: *balance,
                                rewards_sol: 0,
                                exists: false,
                            }
                        }
                    }
                } else {
                    // Miner doesn't exist yet
                    CachedMiner {
                        miner_address,
                        authority: auth_address,
                        deployer_address: deployer.deployer_address,
                        manager_address: deployer.manager_address,
                        checkpoint_id: 0,
                        round_id: 0,
                        has_deployed: false,
                        auth_balance: *balance,
                        rewards_sol: 0,
                        exists: false,
                    }
                };

                self.miners.insert(miner_address, cached);
                fetched_count += 1;
            }
        }

        self.last_refresh_round = Some(current_round_id);
        self.needs_balance_refresh = false;

        info!("Miner cache refreshed: {} miners", fetched_count);
        Ok(fetched_count)
    }

    /// Refresh only balances (lighter weight than full refresh)
    pub fn refresh_balances(
        &mut self,
        rpc_client: &RpcClient,
    ) -> Result<(), CrankError> {
        if !self.needs_balance_refresh {
            return Ok(());
        }

        let auth_addresses: Vec<Pubkey> = self.miners.values()
            .map(|m| m.authority)
            .collect();

        if auth_addresses.is_empty() {
            return Ok(());
        }

        info!("Refreshing {} auth balances", auth_addresses.len());

        // Batch fetch accounts to get lamport balances
        for chunk in auth_addresses.chunks(100) {
            let accounts = rpc_client
                .get_multiple_accounts(chunk)
                .map_err(|e| CrankError::Rpc(format!("Failed to fetch auth accounts: {}", e)))?;
            
            for (addr, account) in chunk.iter().zip(accounts.iter()) {
                let balance = account.as_ref().map(|a| a.lamports).unwrap_or(0);
                // Find and update the miner with this auth
                for miner in self.miners.values_mut() {
                    if miner.authority == *addr {
                        miner.auth_balance = balance;
                        break;
                    }
                }
            }
        }

        self.needs_balance_refresh = false;
        Ok(())
    }

    /// Get all cached miners
    pub fn all_miners(&self) -> impl Iterator<Item = &CachedMiner> {
        self.miners.values()
    }

    /// Get miner address for a deployer
    pub fn get_miner_address_for_deployer(&self, deployer_address: &Pubkey) -> Option<Pubkey> {
        self.miners.values()
            .find(|m| &m.deployer_address == deployer_address)
            .map(|m| m.miner_address)
    }

    /// Refresh a single miner's data (for error recovery)
    /// Returns the updated cached miner if found
    pub fn refresh_single(
        &mut self,
        rpc_client: &RpcClient,
        miner_address: &Pubkey,
    ) -> Result<Option<CachedMiner>, CrankError> {
        // Get existing cached miner to know the auth address
        let cached = match self.miners.get(miner_address) {
            Some(m) => m.clone(),
            None => {
                warn!("Cannot refresh unknown miner: {}", miner_address);
                return Ok(None);
            }
        };

        info!("[MinerCache] Refreshing single miner: {} (auth: {})", miner_address, cached.authority);

        // Fetch both miner account and auth account
        let accounts = rpc_client
            .get_multiple_accounts(&[*miner_address, cached.authority])
            .map_err(|e| CrankError::Rpc(format!("Failed to fetch miner accounts: {}", e)))?;

        let miner_account = accounts.get(0).and_then(|a| a.as_ref());
        let auth_account = accounts.get(1).and_then(|a| a.as_ref());
        let auth_balance = auth_account.map(|a| a.lamports).unwrap_or(0);

        let updated = if let Some(account) = miner_account {
            match Miner::try_from_bytes(&account.data) {
                Ok(miner) => {
                    let has_deployed = miner.deployed.iter().any(|&d| d > 0);
                    CachedMiner {
                        miner_address: *miner_address,
                        authority: cached.authority,
                        deployer_address: cached.deployer_address,
                        manager_address: cached.manager_address,
                        checkpoint_id: miner.checkpoint_id,
                        round_id: miner.round_id,
                        has_deployed,
                        auth_balance,
                        rewards_sol: miner.rewards_sol,
                        exists: true,
                    }
                }
                Err(e) => {
                    warn!("Failed to parse miner {}: {:?}", miner_address, e);
                    CachedMiner {
                        auth_balance,
                        exists: false,
                        ..cached
                    }
                }
            }
        } else {
            // Miner doesn't exist (anymore?)
            CachedMiner {
                auth_balance,
                exists: false,
                ..cached
            }
        };

        info!(
            "[MinerCache] Refreshed miner {} | balance: {} | round_id: {} | deployed: {} | checkpoint_id: {}",
            miner_address, updated.auth_balance, updated.round_id, updated.has_deployed, updated.checkpoint_id
        );

        self.miners.insert(*miner_address, updated.clone());
        Ok(Some(updated))
    }
}
