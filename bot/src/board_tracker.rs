//! BoardTracker - Websocket subscription to Board PDA for real-time updates
//!
//! Provides:
//! - `round_id`: Current round number
//! - `start_slot`: Round start slot
//! - `end_slot`: Round end slot
//!
//! Detects round changes and provides shared access via Arc.

use evore::ore_api::{board_pda, Board};
use solana_account_decoder::UiAccountEncoding;
use solana_client::pubsub_client::PubsubClient;
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_sdk::commitment_config::CommitmentConfig;
use steel::AccountDeserialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

/// Tracks Board account state via websocket subscription
pub struct BoardTracker {
    pub ws_url: String,
    board: Arc<RwLock<Option<Board>>>,
    last_round_id: Arc<RwLock<u64>>,
    /// True if WS subscription is currently connected
    pub ws_connected: Arc<AtomicBool>,
}

impl BoardTracker {
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
            board: Arc::new(RwLock::new(None)),
            last_round_id: Arc::new(RwLock::new(0)),
            ws_connected: Arc::new(AtomicBool::new(false)),
        }
    }
    
    /// Check if WS is connected
    pub fn is_connected(&self) -> bool {
        self.ws_connected.load(Ordering::Relaxed)
    }

    /// Get current board state (None if not yet received)
    pub fn get_board(&self) -> Option<Board> {
        *self.board.read().unwrap()
    }

    /// Get current round ID
    pub fn get_round_id(&self) -> u64 {
        self.get_board().map(|b| b.round_id).unwrap_or(0)
    }

    /// Get current end slot
    pub fn get_end_slot(&self) -> u64 {
        self.get_board().map(|b| b.end_slot).unwrap_or(u64::MAX)
    }

    /// Get current start slot
    pub fn get_start_slot(&self) -> u64 {
        self.get_board().map(|b| b.start_slot).unwrap_or(0)
    }

    /// Check if a new round started (round_id changed since last check)
    pub fn check_new_round(&self) -> Option<u64> {
        let current = self.get_round_id();
        let mut last = self.last_round_id.write().unwrap();
        if current > *last && current > 0 {
            *last = current;
            Some(current)
        } else {
            None
        }
    }

    /// Start websocket subscription to Board account (runs in background thread)
    /// Quietly reconnects on error with exponential backoff (max 30s)
    pub fn start_subscription(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let board = Arc::clone(&self.board);
        let ws_connected = Arc::clone(&self.ws_connected);
        let ws_url = self.ws_url.clone();
        let board_address = board_pda().0;

        std::thread::spawn(move || {
            let mut retry_delay_secs = 1u64;
            const MAX_RETRY_DELAY: u64 = 30;
            
            loop {
                let config = RpcAccountInfoConfig {
                    encoding: Some(UiAccountEncoding::Base64),
                    commitment: Some(CommitmentConfig::confirmed()),
                    data_slice: None,
                    min_context_slot: None,
                };

                match PubsubClient::account_subscribe(&ws_url, &board_address, Some(config)) {
                    Ok((_subscription, receiver)) => {
                        // Reset backoff on successful connection
                        retry_delay_secs = 1;
                        ws_connected.store(true, Ordering::Relaxed);
                        
                        for response in receiver {
                            if let Some(account) = response.value.data.decode() {
                                // Parse Board from account data - silently skip invalid data
                                if let Ok(b) = Board::try_from_bytes(&account) {
                                    let mut board_lock = board.write().unwrap();
                                    *board_lock = Some(*b);
                                }
                            }
                        }
                        // Receiver closed, mark as disconnected
                        ws_connected.store(false, Ordering::Relaxed);
                    }
                    Err(_) => {
                        // Mark as disconnected, quiet retry with exponential backoff
                        ws_connected.store(false, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_secs(retry_delay_secs));
                        retry_delay_secs = (retry_delay_secs * 2).min(MAX_RETRY_DELAY);
                    }
                }
            }
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_tracker_new() {
        let tracker = BoardTracker::new("wss://example.com");
        assert!(tracker.get_board().is_none());
        assert_eq!(tracker.get_round_id(), 0);
        assert_eq!(tracker.get_end_slot(), u64::MAX);
    }
}
