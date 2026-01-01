//! Transaction Sender System
//!
//! Sends signed transactions via RPC.
//! Does not wait for confirmation - that's handled by the confirmation system.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

use crate::sender::TxSender;

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::SignedTx;

/// Minimum delay between transaction sends to avoid rate limiting
const SEND_DELAY: Duration = Duration::from_millis(400);

/// Run the transaction sender system
pub async fn run(
    _shared: Arc<SharedState>,
    _senders: ChannelSenders,
    mut rx: mpsc::Receiver<SignedTx>,
    rpc_url: String,
) {
    info!("[TxSender] Starting...");

    let sender = TxSender::new(rpc_url);
    let mut sent_count = 0u64;
    let mut failed_count = 0u64;

    while let Some(signed_tx) = rx.recv().await {
        let tx_type = signed_tx.tx_type;
        let signature = signed_tx.signature;
        let batch_size = signed_tx.miners.len();

        debug!(
            "[TxSender] Sending {} txn: {} ({} miners)",
            tx_type, signature, batch_size
        );

        // Send the transaction (don't wait for confirmation)
        match sender.send_versioned_rpc(&signed_tx.tx).await {
            Ok(sig) => {
                info!(
                    "[TxSender] Sent {} txn: {}",
                    tx_type, sig
                );
                sent_count += 1;
            }
            Err(e) => {
                error!(
                    "[TxSender] Failed to send {} txn {}: {}",
                    tx_type, signature, e
                );
                failed_count += 1;

                // The confirmation system will handle timeouts and retries
            }
        }

        // Log progress periodically
        if (sent_count + failed_count) % 10 == 0 {
            info!(
                "[TxSender] Sent: {}, Failed: {}",
                sent_count, failed_count
            );
        }

        // Rate limit: wait before sending next transaction
        sleep(SEND_DELAY).await;
    }

    info!(
        "[TxSender] Shutting down. Sent: {}, Failed: {}",
        sent_count, failed_count
    );
}

