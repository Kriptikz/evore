//! Miner Tracker - Polls miner accounts for per-bot deployment data
//!
//! Provides periodic updates of miner deployed[25] arrays for board display.
//! Uses getMultipleAccounts RPC call to efficiently poll all miners at once.

use std::sync::Arc;
use std::time::Duration;

use evore::ore_api::miner_pda;
use solana_sdk::pubkey::Pubkey;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::client::{EvoreClient, RpsTracker};
use crate::tui::TuiUpdate;

/// Configuration for tracking a bot's miner
#[derive(Clone)]
pub struct MinerTrackConfig {
    /// Bot index in the TUI
    pub bot_index: usize,
    /// The miner authority (managed_miner_auth PDA)
    pub miner_authority: Pubkey,
}

/// Miner tracker that polls miner accounts for all bots
pub struct MinerTracker {
    rpc_url: String,
    rps_tracker: Arc<RpsTracker>,
    miners: Vec<MinerTrackConfig>,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
    poll_interval: Duration,
}

impl MinerTracker {
    /// Create a new miner tracker
    pub fn new(
        rpc_url: &str,
        rps_tracker: Arc<RpsTracker>,
        tui_tx: mpsc::UnboundedSender<TuiUpdate>,
    ) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rps_tracker,
            miners: Vec::new(),
            tui_tx,
            poll_interval: Duration::from_millis(1000),
        }
    }

    /// Add a miner to track (pass the authority/managed_miner_auth PDA)
    pub fn add_miner(&mut self, bot_index: usize, miner_authority: Pubkey) {
        self.miners.push(MinerTrackConfig { bot_index, miner_authority });
    }

    /// Start the polling loop (spawns a tokio task)
    pub fn start(&self) {
        if self.miners.is_empty() {
            return;
        }

        let rpc_url = self.rpc_url.clone();
        let rps_tracker = Arc::clone(&self.rps_tracker);
        let miners = self.miners.clone();
        let tui_tx = self.tui_tx.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            Self::poll_loop(rpc_url, rps_tracker, miners, tui_tx, poll_interval).await;
        });
    }

    /// Main polling loop
    async fn poll_loop(
        rpc_url: String,
        rps_tracker: Arc<RpsTracker>,
        miners: Vec<MinerTrackConfig>,
        tui_tx: mpsc::UnboundedSender<TuiUpdate>,
        poll_interval: Duration,
    ) {
        let client = EvoreClient::new_with_tracker(&rpc_url, rps_tracker);
        let authorities: Vec<Pubkey> = miners.iter().map(|m| m.miner_authority).collect();

        loop {
            // Poll all miner accounts at once
            match client.get_miners(&authorities) {
                Ok(miner_opts) => {
                    for (i, miner_opt) in miner_opts.iter().enumerate() {
                        if let Some(miner) = miner_opt {
                            let config = &miners[i];
                            
                            // Send miner data update
                            let _ = tui_tx.send(TuiUpdate::MinerDataUpdate {
                                bot_index: config.bot_index,
                                deployed: miner.deployed,
                                round_id: miner.round_id,
                            });
                        }
                    }
                }
                Err(_) => {
                    // Silently ignore RPC errors - will retry on next interval
                }
            }

            sleep(poll_interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_miner_track_config() {
        let config = MinerTrackConfig {
            bot_index: 0,
            miner_authority: Pubkey::new_unique(),
        };
        assert_eq!(config.bot_index, 0);
    }
}
