//! Treasury Tracker - Polls ORE Treasury account for network stats
//!
//! Provides periodic updates of:
//! - balance: SOL collected for buy-bury operations
//! - motherlode: ORE in the motherlode rewards pool
//! - total_staked: Total ORE staking deposits
//! - total_unclaimed: Total unclaimed ORE mining rewards
//! - total_refined: Total refined ORE mining rewards

use std::sync::Arc;
use std::time::Duration;

use evore::ore_api::Treasury;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::client::{EvoreClient, RpsTracker};
use crate::tui::TuiUpdate;

/// Treasury data for TUI display
#[derive(Clone, Debug, Default)]
pub struct TreasuryData {
    /// SOL balance for buy-bury operations (lamports)
    pub balance: u64,
    /// ORE in the motherlode rewards pool
    pub motherlode: u64,
    /// Total ORE staking deposits
    pub total_staked: u64,
    /// Total unclaimed ORE mining rewards
    pub total_unclaimed: u64,
    /// Total refined ORE mining rewards
    pub total_refined: u64,
}

impl From<&Treasury> for TreasuryData {
    fn from(treasury: &Treasury) -> Self {
        Self {
            balance: treasury.balance,
            motherlode: treasury.motherlode,
            total_staked: treasury.total_staked,
            total_unclaimed: treasury.total_unclaimed,
            total_refined: treasury.total_refined,
        }
    }
}

/// Treasury tracker that polls the ORE Treasury account
pub struct TreasuryTracker {
    rpc_url: String,
    rps_tracker: Arc<RpsTracker>,
    tui_tx: mpsc::UnboundedSender<TuiUpdate>,
    poll_interval: Duration,
}

impl TreasuryTracker {
    /// Create a new treasury tracker
    pub fn new(
        rpc_url: &str,
        rps_tracker: Arc<RpsTracker>,
        tui_tx: mpsc::UnboundedSender<TuiUpdate>,
    ) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rps_tracker,
            tui_tx,
            poll_interval: Duration::from_secs(2),
        }
    }

    /// Start the polling loop (spawns a tokio task)
    pub fn start(&self) {
        let rpc_url = self.rpc_url.clone();
        let rps_tracker = Arc::clone(&self.rps_tracker);
        let tui_tx = self.tui_tx.clone();
        let poll_interval = self.poll_interval;

        tokio::spawn(async move {
            Self::poll_loop(rpc_url, rps_tracker, tui_tx, poll_interval).await;
        });
    }

    /// Main polling loop
    async fn poll_loop(
        rpc_url: String,
        rps_tracker: Arc<RpsTracker>,
        tui_tx: mpsc::UnboundedSender<TuiUpdate>,
        poll_interval: Duration,
    ) {
        let client = EvoreClient::new_with_tracker(&rpc_url, rps_tracker);
        
        loop {
            // Poll treasury account
            match client.get_treasury() {
                Ok(treasury) => {
                    let data = TreasuryData::from(&treasury);
                    let _ = tui_tx.send(TuiUpdate::TreasuryUpdate(data));
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
    fn test_treasury_data_default() {
        let data = TreasuryData::default();
        assert_eq!(data.balance, 0);
        assert_eq!(data.motherlode, 0);
    }
}
