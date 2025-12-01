//! RoundTracker - Websocket subscription to Round PDA for real-time deployment data
//!
//! Provides:
//! - `deployed[25]`: Amount deployed per square
//! - `total_deployed`: Total amount deployed in round
//! - `motherlode`: ORE in the motherlode
//!
//! Switches subscription when round_id changes.

use evore::ore_api::{round_pda, Round};
use solana_account_decoder::UiAccountEncoding;
use solana_client::pubsub_client::PubsubClient;
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use steel::AccountDeserialize;
use std::sync::{Arc, RwLock};

/// Tracks Round account state via websocket subscription
pub struct RoundTracker {
    ws_url: String,
    round: Arc<RwLock<Option<Round>>>,
    current_round_id: Arc<RwLock<u64>>,
    /// Signal to stop the current subscription thread
    stop_signal: Arc<RwLock<bool>>,
}

impl RoundTracker {
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            round: Arc::new(RwLock::new(None)),
            current_round_id: Arc::new(RwLock::new(0)),
            stop_signal: Arc::new(RwLock::new(false)),
        }
    }

    /// Get current round state (None if not yet received)
    pub fn get_round(&self) -> Option<Round> {
        *self.round.read().unwrap()
    }

    /// Get deployed amounts per square
    pub fn get_deployed(&self) -> [u64; 25] {
        self.get_round().map(|r| r.deployed).unwrap_or([0; 25])
    }

    /// Get total deployed in current round
    pub fn get_total_deployed(&self) -> u64 {
        self.get_round().map(|r| r.total_deployed).unwrap_or(0)
    }

    /// Get motherlode amount
    pub fn get_motherlode(&self) -> u64 {
        self.get_round().map(|r| r.motherlode).unwrap_or(0)
    }

    /// Get current tracked round ID
    pub fn get_tracked_round_id(&self) -> u64 {
        *self.current_round_id.read().unwrap()
    }

    /// Switch to tracking a new round
    /// Returns true if switched, false if already tracking this round
    pub fn switch_round(&self, new_round_id: u64) -> bool {
        let current = *self.current_round_id.read().unwrap();
        if new_round_id == current && current != 0 {
            return false;
        }

        // Signal current subscription to stop
        {
            let mut stop = self.stop_signal.write().unwrap();
            *stop = true;
        }

        // Update round ID
        {
            let mut id = self.current_round_id.write().unwrap();
            *id = new_round_id;
        }

        // Clear old round data
        {
            let mut round = self.round.write().unwrap();
            *round = None;
        }

        // Start new subscription
        self.start_subscription_for_round(new_round_id);

        true
    }

    /// Start websocket subscription for a specific round
    fn start_subscription_for_round(&self, round_id: u64) {
        let round = Arc::clone(&self.round);
        let ws_url = self.ws_url.clone();
        let stop_signal = Arc::clone(&self.stop_signal);
        let round_address = round_pda(round_id).0;

        // Reset stop signal for new subscription
        {
            let mut stop = stop_signal.write().unwrap();
            *stop = false;
        }

        std::thread::spawn(move || {
            loop {
                // Check if we should stop
                if *stop_signal.read().unwrap() {
                    break;
                }

                let config = RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    data_slice: None,
                    min_context_slot: None,
                };

                match PubsubClient::account_subscribe(&ws_url, &round_address, Some(config)) {
                    Ok((_subscription, receiver)) => {
                        for response in receiver {
                            // Check stop signal between messages
                            if *stop_signal.read().unwrap() {
                                break;
                            }

                            if let Some(account) = response.value.data.decode() {
                                match Round::try_from_bytes(&account) {
                                    Ok(r) => {
                                        let mut round_lock = round.write().unwrap();
                                        *round_lock = Some(*r);
                                    }
                                    Err(e) => {
                                        eprintln!("RoundTracker: Failed to parse Round: {}", e);
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        // Check stop signal before reconnecting
                        if *stop_signal.read().unwrap() {
                            break;
                        }
                        eprintln!("RoundTracker: Subscription error: {}, reconnecting...", e);
                        std::thread::sleep(std::time::Duration::from_secs(1));
                    }
                }
            }
        });
    }

    /// Start initial subscription (call switch_round after getting first board data)
    pub fn start_subscription(&self, initial_round_id: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.switch_round(initial_round_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_tracker_new() {
        let tracker = RoundTracker::new("wss://example.com");
        assert!(tracker.get_round().is_none());
        assert_eq!(tracker.get_total_deployed(), 0);
        assert_eq!(tracker.get_tracked_round_id(), 0);
    }

    #[test]
    fn test_switch_round_returns_false_for_same_round() {
        let tracker = RoundTracker::new("wss://example.com");
        // First switch should work
        // Note: can't actually test subscription without real WS
        // Just testing the logic
        {
            let mut id = tracker.current_round_id.write().unwrap();
            *id = 5;
        }
        // Same round should return false
        assert!(!tracker.switch_round(5));
    }
}
