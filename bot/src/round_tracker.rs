//! RoundTracker - RPC polling for Round PDA deployment data
//!
//! Provides:
//! - `deployed[25]`: Amount deployed per square
//! - `total_deployed`: Total amount deployed in round
//! - `motherlode`: ORE in the motherlode
//!
//! Uses RPC polling instead of WebSocket for stability.
//! Polls every 1 second. Switches to new round PDA when round_id changes.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::time::Duration;

use evore::ore_api::Round;

use crate::client::{EvoreClient, RpsTracker};

/// Tracks Round account state via RPC polling
pub struct RoundTracker {
    round: Arc<RwLock<Option<Round>>>,
    current_round_id: Arc<RwLock<u64>>,
    /// Signal to stop the polling thread
    stop_signal: Arc<AtomicBool>,
    /// True if polling is active and receiving data
    connected: Arc<AtomicBool>,
    /// Poll interval
    poll_interval: Duration,
    /// RPC URL
    rpc_url: String,
    /// Shared RPS tracker
    rps_tracker: Arc<RpsTracker>,
}

impl RoundTracker {
    /// Create a new round tracker with RPC polling
    pub fn new(rpc_url: &str, rps_tracker: Arc<RpsTracker>) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            rps_tracker,
            round: Arc::new(RwLock::new(None)),
            current_round_id: Arc::new(RwLock::new(0)),
            stop_signal: Arc::new(AtomicBool::new(false)),
            connected: Arc::new(AtomicBool::new(false)),
            poll_interval: Duration::from_millis(1000),
        }
    }
    
    /// Check if polling is connected (receiving data)
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    /// Get current round state (None if not yet received)
    pub fn get_round(&self) -> Option<Round> {
        *self.round.read().unwrap()
    }

    /// Switch to tracking a new round
    /// Returns true if switched, false if already tracking this round
    pub fn switch_round(&self, new_round_id: u64) -> bool {
        let current = *self.current_round_id.read().unwrap();
        if new_round_id == current && current != 0 {
            return false;
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

        true
    }

    /// Start the polling loop (spawns a thread)
    pub fn start(&self) {
        let round = Arc::clone(&self.round);
        let current_round_id = Arc::clone(&self.current_round_id);
        let stop_signal = Arc::clone(&self.stop_signal);
        let connected = Arc::clone(&self.connected);
        let poll_interval = self.poll_interval;
        let rpc_url = self.rpc_url.clone();
        let rps_tracker = Arc::clone(&self.rps_tracker);

        std::thread::spawn(move || {
            let client = EvoreClient::new_with_tracker(&rpc_url, rps_tracker);
            let mut consecutive_failures = 0u32;
            
            loop {
                // Check if we should stop
                if stop_signal.load(Ordering::Relaxed) {
                    connected.store(false, Ordering::Relaxed);
                    break;
                }

                // Get current round ID to poll
                let round_id = *current_round_id.read().unwrap();
                
                if round_id == 0 {
                    // No round to track yet
                    std::thread::sleep(poll_interval);
                    continue;
                }

                // Poll the round account
                match client.get_round(round_id) {
                    Ok(r) => {
                        let mut round_lock = round.write().unwrap();
                        *round_lock = Some(r);
                        connected.store(true, Ordering::Relaxed);
                        consecutive_failures = 0;
                    }
                    Err(_) => {
                        consecutive_failures += 1;
                        // After 5 consecutive failures, mark as disconnected
                        if consecutive_failures >= 5 {
                            connected.store(false, Ordering::Relaxed);
                        }
                    }
                }

                std::thread::sleep(poll_interval);
            }
        });
    }

    /// Stop the polling loop
    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop_signal.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_round_tracker_new() {
        let tracker = Arc::new(RpsTracker::new());
        let round_tracker = RoundTracker::new("https://example.com", tracker);
        assert!(round_tracker.get_round().is_none());
    }

    #[test]
    fn test_switch_round_returns_false_for_same_round() {
        let tracker = Arc::new(RpsTracker::new());
        let round_tracker = RoundTracker::new("https://example.com", tracker);
        // Set initial round
        {
            let mut id = round_tracker.current_round_id.write().unwrap();
            *id = 5;
        }
        // Same round should return false
        assert!(!round_tracker.switch_round(5));
        // Different round should return true
        assert!(round_tracker.switch_round(6));
    }
}
