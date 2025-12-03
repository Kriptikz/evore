//! Round Coordinator - Orchestrates multiple bots with shared services
//!
//! Responsibilities:
//! - Creates and manages shared services
//! - Spawns bot tasks from configuration
//! - Handles graceful shutdown
//! - Provides runtime config updates

use std::sync::Arc;

use solana_sdk::signature::{read_keypair_file, Keypair};
use solana_sdk::signer::Signer;
use tokio::sync::{mpsc, RwLock};
use tokio::task::JoinHandle;

use crate::bot_runner::{run_bot_with_services, BotRunConfig, SharedServices};
use crate::config::{BotConfig, Config, StrategyParams};
use crate::sender::PingStats;
use crate::tui::TuiUpdate;

/// Coordinator for running multiple bots
pub struct RoundCoordinator {
    services: Arc<SharedServices>,
    bot_handles: Vec<JoinHandle<()>>,
    /// Shared configs that can be updated at runtime
    bot_configs: Vec<Arc<RwLock<BotRunConfig>>>,
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
            bot_configs: Vec::new(),
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
        let run_config = Arc::new(RwLock::new(BotRunConfig {
            name: bot_config.name.clone(),
            bot_index,
            auth_id: bot_config.auth_id,
            manager: manager_pubkey,
            signer,
            slots_left: bot_config.slots_left,
            strategy: bot_config.strategy,
            strategy_params: bot_config.strategy_params.clone(),
            bankroll: bot_config.bankroll,
            attempts: bot_config.attempts,
            priority_fee: bot_config.priority_fee,
            jito_tip: bot_config.jito_tip,
        }));

        // Store config for runtime updates
        self.bot_configs.push(Arc::clone(&run_config));

        let services = Arc::clone(&self.services);
        let tui_tx = self.tui_tx.clone();

        let handle = tokio::spawn(async move {
            run_bot_with_services(run_config, services, tui_tx).await;
        });

        self.bot_handles.push(handle);
    }
    
    /// Update a bot's runtime config (called from TUI config reload)
    pub async fn update_bot_config(&self, bot_index: usize, new_config: &BotConfig) -> Result<(), String> {
        let config = self.bot_configs.get(bot_index)
            .ok_or_else(|| format!("Bot {} not found", bot_index))?;
        
        let mut cfg = config.write().await;
        cfg.bankroll = new_config.bankroll;
        cfg.slots_left = new_config.slots_left;
        cfg.priority_fee = new_config.priority_fee;
        cfg.jito_tip = new_config.jito_tip;
        cfg.attempts = new_config.attempts;
        cfg.strategy_params = new_config.strategy_params.clone();
        
        Ok(())
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
    
    /// Get ping stats for sender endpoints
    pub fn get_ping_stats(&self) -> Arc<PingStats> {
        Arc::clone(&self.services.ping_stats)
    }
    
    /// Check if slot WS is connected
    pub fn is_slot_ws_connected(&self) -> bool {
        self.services.slot_tracker.is_ws_connected()
    }
    
    /// Check if board WS is connected
    pub fn is_board_ws_connected(&self) -> bool {
        self.services.board_tracker.is_connected()
    }
    
    /// Check if round WS is connected
    pub fn is_round_ws_connected(&self) -> bool {
        self.services.round_tracker.is_connected()
    }
    
    /// Check if RPC is connected
    pub fn is_rpc_connected(&self) -> bool {
        self.services.slot_tracker.is_rpc_connected()
    }
    
    /// Get current RPC RPS (requests per second)
    pub fn get_rpc_rps(&self) -> u32 {
        self.services.client.rps_tracker.get_rps()
    }
    
    /// Get current Sender RPS (HTTP sends per second)
    pub fn get_sender_rps(&self) -> u32 {
        self.services.ping_stats.get_sender_rps()
    }
    
    /// Get total RPC requests made
    pub fn get_rpc_total(&self) -> u64 {
        self.services.client.rps_tracker.get_total()
    }
    
    /// Get total sender HTTP sends made
    pub fn get_sender_total(&self) -> u64 {
        self.services.ping_stats.get_total_sends()
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
        attempts: 4,
        priority_fee: 5000,  // Default priority fee
        jito_tip: 200_000,   // Default jito tip (0.0002 SOL)
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
