//! Round Coordinator - Orchestrates multiple bots with shared services
//!
//! Responsibilities:
//! - Creates and manages shared services
//! - Spawns bot tasks from configuration
//! - Handles graceful shutdown

use std::sync::Arc;

use solana_sdk::signature::{read_keypair_file, Keypair};
use solana_sdk::signer::Signer;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::bot_runner::{run_bot_with_services, BotRunConfig, SharedServices};
use crate::config::{BotConfig, Config, StrategyParams};
use crate::tui::TuiUpdate;

/// Coordinator for running multiple bots
pub struct RoundCoordinator {
    services: Arc<SharedServices>,
    bot_handles: Vec<JoinHandle<()>>,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
}

impl RoundCoordinator {
    /// Create coordinator with shared services
    pub fn new(
        rpc_url: &str,
        ws_url: &str,
        tui_tx: mpsc::UnboundedSender<TuiUpdate>,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let services = Arc::new(SharedServices::new(rpc_url, ws_url)?);
        
        Ok(Self {
            services,
            bot_handles: Vec::new(),
            tui_tx,
        })
    }

    /// Start all background services
    pub fn start_services(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.services.start()
    }

    /// Spawn a bot from configuration
    pub fn spawn_bot(
        &mut self,
        bot_config: BotConfig,
        bot_index: usize,
        signer: Arc<Keypair>,
        manager_pubkey: solana_sdk::pubkey::Pubkey,
    ) {
        let run_config = BotRunConfig {
            name: bot_config.name.clone(),
            bot_index,
            auth_id: bot_config.auth_id,
            manager: manager_pubkey,
            signer,
            slots_left: bot_config.slots_left,
            strategy_params: bot_config.strategy_params.clone(),
        };

        let services = Arc::clone(&self.services);
        let tui_tx = self.tui_tx.clone();

        let handle = tokio::spawn(async move {
            run_bot_with_services(run_config, services, tui_tx).await;
        });

        self.bot_handles.push(handle);
    }

    /// Spawn multiple bots from full config
    pub fn spawn_bots_from_config(
        &mut self,
        config: &Config,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for (index, bot_config) in config.bots.iter().enumerate() {
            // Load keypairs
            let signer_path = config.get_signer_path(bot_config);
            let manager_path = config.get_manager_path(bot_config);
            
            let signer = Arc::new(read_keypair_file(&signer_path).map_err(|e| {
                format!("Failed to load signer from {:?}: {}", signer_path, e)
            })?);
            
            let manager_keypair = read_keypair_file(&manager_path).map_err(|e| {
                format!("Failed to load manager from {:?}: {}", manager_path, e)
            })?;
            let manager_pubkey = manager_keypair.pubkey();

            self.spawn_bot(bot_config.clone(), index, signer, manager_pubkey);
        }

        Ok(())
    }

    /// Wait for all bots to complete (they run forever, so this blocks until shutdown)
    pub async fn wait_for_bots(&mut self) {
        for handle in self.bot_handles.drain(..) {
            let _ = handle.await;
        }
    }

    /// Abort all running bots
    pub fn abort_all(&self) {
        for handle in &self.bot_handles {
            handle.abort();
        }
    }

    /// Get number of running bots
    pub fn bot_count(&self) -> usize {
        self.bot_handles.len()
    }
}

/// Create a coordinator and run with a single bot (for CLI compatibility)
pub async fn run_single_bot(
    rpc_url: &str,
    ws_url: &str,
    signer: Arc<Keypair>,
    manager_pubkey: solana_sdk::pubkey::Pubkey,
    auth_id: u64,
    slots_left: u64,
    strategy_params: StrategyParams,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut coordinator = RoundCoordinator::new(rpc_url, ws_url, tui_tx)?;
    coordinator.start_services()?;

    let bot_config = BotConfig {
        name: "Bot1".to_string(),
        auth_id,
        strategy: crate::config::DeployStrategy::EV,
        slots_left,
        bankroll: 0, // Will be determined from account
        strategy_params,
        signer_path: None,
        manager_path: None,
    };

    coordinator.spawn_bot(bot_config, 0, signer, manager_pubkey);
    coordinator.wait_for_bots().await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coordinator_bot_count() {
        // Just test struct creation (can't test full functionality without RPC)
        // Coordinator needs actual RPC/WS URLs
    }
}
