//! Transaction Pipeline - Async send and batch confirmation
//!
//! Components:
//! - TxSender: Reads from channel, sends instantly (no blocking)
//! - TxConfirmer: Batch getSignatureStatuses, returns results via oneshot
//!
//! This decouples transaction sending from confirmation checking.

use solana_client::rpc_client::RpcClient;
use solana_client::rpc_config::RpcSendTransactionConfig;
use solana_sdk::{signature::Signature, transaction::Transaction};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

/// Request to send a transaction
pub struct TxRequest {
    pub transaction: Transaction,
    pub response_tx: oneshot::Sender<TxResult>,
    /// Optional bot name for logging
    pub bot_name: Option<String>,
}

/// Result of a transaction send/confirm
#[derive(Debug, Clone)]
pub struct TxResult {
    pub signature: Signature,
    pub confirmed: bool,
    pub error: Option<String>,
    /// Slot the transaction landed in (if confirmed)
    pub slot_landed: Option<u64>,
}

/// Pending signature waiting for confirmation (internal)
pub(crate) struct PendingSig {
    signature: Signature,
    response_tx: oneshot::Sender<TxResult>,
    #[allow(dead_code)]
    bot_name: Option<String>,
}

/// Transaction sender task
/// 
/// Reads transactions from channel, sends immediately, queues for confirmation.
pub(crate) async fn tx_sender_task(
    rpc: Arc<RpcClient>,
    mut request_rx: mpsc::UnboundedReceiver<TxRequest>,
    pending_tx: mpsc::UnboundedSender<PendingSig>,
) {
    let config = RpcSendTransactionConfig {
        skip_preflight: true,
        max_retries: Some(0),
        ..Default::default()
    };

    while let Some(req) = request_rx.recv().await {
        match rpc.send_transaction_with_config(&req.transaction, config) {
            Ok(sig) => {
                // Queue for confirmation
                let _ = pending_tx.send(PendingSig {
                    signature: sig,
                    response_tx: req.response_tx,
                    bot_name: req.bot_name,
                });
            }
            Err(e) => {
                // Send immediate failure
                let _ = req.response_tx.send(TxResult {
                    signature: Signature::default(),
                    confirmed: false,
                    error: Some(format!("Send failed: {}", e)),
                    slot_landed: None,
                });
            }
        }
    }
}

/// Transaction confirmer task
/// 
/// Collects pending signatures, batch checks status, returns results.
pub(crate) async fn tx_confirmer_task(
    rpc: Arc<RpcClient>,
    mut pending_rx: mpsc::UnboundedReceiver<PendingSig>,
) {
    let mut pending: HashMap<Signature, PendingSig> = HashMap::new();
    
    loop {
        // Drain all pending signatures from channel (non-blocking)
        loop {
            match pending_rx.try_recv() {
                Ok(p) => {
                    pending.insert(p.signature, p);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => return,
            }
        }

        if pending.is_empty() {
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }

        // Batch check up to 256 signatures at a time
        let sigs: Vec<Signature> = pending.keys().copied().collect();
        let batch_size = 256.min(sigs.len());
        let batch: Vec<Signature> = sigs.into_iter().take(batch_size).collect();

        match rpc.get_signature_statuses(&batch) {
            Ok(response) => {
                let statuses = response.value;
                
                for (sig, status_opt) in batch.iter().zip(statuses.iter()) {
                    if let Some(status) = status_opt {
                        // Transaction has a status (confirmed or error)
                        if let Some(p) = pending.remove(sig) {
                            let has_error = status.err.is_some();
                            let error_msg = status.err.as_ref().map(|e| format!("{:?}", e));
                            
                            let _ = p.response_tx.send(TxResult {
                                signature: *sig,
                                confirmed: !has_error,
                                error: error_msg,
                                slot_landed: status.slot.into(),
                            });
                        }
                    }
                    // else: still pending, keep in map
                }
            }
            Err(e) => {
                eprintln!("TxConfirmer: status check error: {}", e);
            }
        }

        // Timeout old pending transactions (30 seconds)
        // Note: For simplicity, we're not tracking insert time here
        // In production, add timestamp tracking

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// Create the transaction pipeline channels and tasks
/// 
/// Returns the sender channel for submitting transactions.
pub fn create_tx_pipeline(rpc: Arc<RpcClient>) -> mpsc::UnboundedSender<TxRequest> {
    let (request_tx, request_rx) = mpsc::unbounded_channel::<TxRequest>();
    let (pending_tx, pending_rx) = mpsc::unbounded_channel::<PendingSig>();

    let rpc_sender = Arc::clone(&rpc);
    let rpc_confirmer = Arc::clone(&rpc);

    // Spawn sender task
    tokio::spawn(async move {
        tx_sender_task(rpc_sender, request_rx, pending_tx).await;
    });

    // Spawn confirmer task
    tokio::spawn(async move {
        tx_confirmer_task(rpc_confirmer, pending_rx).await;
    });

    request_tx
}

/// Helper to send a transaction and wait for confirmation
pub async fn send_and_confirm(
    tx_channel: &mpsc::UnboundedSender<TxRequest>,
    transaction: Transaction,
    bot_name: Option<String>,
) -> Result<TxResult, String> {
    let (response_tx, response_rx) = oneshot::channel();
    
    tx_channel
        .send(TxRequest {
            transaction,
            response_tx,
            bot_name,
        })
        .map_err(|_| "Channel closed".to_string())?;

    response_rx
        .await
        .map_err(|_| "Response channel closed".to_string())
}

/// Helper to send a transaction without waiting (fire and forget)
pub fn send_no_wait(
    tx_channel: &mpsc::UnboundedSender<TxRequest>,
    transaction: Transaction,
    bot_name: Option<String>,
) -> oneshot::Receiver<TxResult> {
    let (response_tx, response_rx) = oneshot::channel();
    
    let _ = tx_channel.send(TxRequest {
        transaction,
        response_tx,
        bot_name,
    });

    response_rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tx_result_default() {
        let result = TxResult {
            signature: Signature::default(),
            confirmed: false,
            error: None,
            slot_landed: None,
        };
        assert!(!result.confirmed);
    }
}
