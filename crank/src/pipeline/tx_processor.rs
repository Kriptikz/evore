//! Transaction Processor System
//!
//! Signs transactions and sends them to the sender system.
//! Also sends signature info to the confirmation system.

use std::sync::Arc;

use solana_sdk::signature::{Keypair, Signer};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use super::channels::ChannelSenders;
use super::shared_state::SharedState;
use super::types::{BatchedTx, PendingConfirmation, SignedTx};

/// Run the transaction processor system
pub async fn run(
    shared: Arc<SharedState>,
    senders: ChannelSenders,
    mut rx: mpsc::Receiver<BatchedTx>,
    deploy_authority: Arc<Keypair>,
) {
    info!("[TxProcessor] Starting...");

    let mut processed_count = 0u64;

    while let Some(batched_tx) = rx.recv().await {
        let tx_type = batched_tx.tx_type;
        let batch_size = batched_tx.batch_size();
        let round_id = batched_tx.round_id;

        debug!(
            "[TxProcessor] Processing {} txn with {} miners for round {}",
            tx_type, batch_size, round_id
        );

        // The transaction should already be signed by the batcher
        // Just extract the signature
        let signature = batched_tx.tx.signatures[0];

        info!(
            "[TxProcessor] Signed {} txn: {} ({} miners)",
            tx_type, signature, batch_size
        );

        // Create signed transaction
        let signed_tx = SignedTx::new(
            batched_tx.tx,
            signature,
            batched_tx.miners.clone(),
            tx_type,
            round_id,
        );

        // Send to sender
        if let Err(e) = senders.to_tx_sender.send(signed_tx).await {
            error!("[TxProcessor] Failed to send to tx sender: {}", e);
            continue;
        }

        // Create pending confirmation
        let pending = PendingConfirmation::new(
            signature,
            batched_tx.miners,
            tx_type,
            round_id,
        );

        // Send to confirmation system
        if let Err(e) = senders.to_confirmation.send(pending).await {
            error!("[TxProcessor] Failed to send to confirmation: {}", e);
        }

        processed_count += 1;

        // Log progress periodically
        if processed_count % 10 == 0 {
            info!(
                "[TxProcessor] Processed {} transactions",
                processed_count
            );
        }
    }

    info!(
        "[TxProcessor] Shutting down. Total processed: {}",
        processed_count
    );
}

