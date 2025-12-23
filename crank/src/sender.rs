//! Transaction sender module
//!
//! Handles sending transactions via standard RPC

use solana_sdk::{
    signature::Signature,
    transaction::{Transaction, VersionedTransaction},
};
use std::str::FromStr;
use std::time::Duration;
use tracing::info;

/// Transaction sender
pub struct TxSender {
    client: reqwest::Client,
    rpc_url: String,
}

impl TxSender {
    pub fn new(rpc_url: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        
        Self {
            client,
            rpc_url,
        }
    }
    
    /// Send a transaction via standard RPC (sendTransaction)
    pub async fn send_rpc(&self, tx: &Transaction) -> Result<Signature, SendError> {
        let tx_bytes = bincode::serialize(tx)
            .map_err(|e| SendError::Serialize(e.to_string()))?;
        let tx_base64 = base64::encode(&tx_bytes);
        
        info!("Sending tx: {} bytes (limit 1232)", tx_bytes.len());
        
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                    "maxRetries": 0
                }
            ]
        });
        
        let response = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SendError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| SendError::Parse(e.to_string()))?;
        
        if let Some(error) = json.get("error") {
            return Err(SendError::RpcError(error.to_string()));
        }
        
        let sig_str = json["result"].as_str()
            .ok_or(SendError::Parse("No result in response".to_string()))?;
        
        let signature = Signature::from_str(sig_str)
            .map_err(|e| SendError::Parse(e.to_string()))?;
        
        Ok(signature)
    }
    
    /// Check transaction signature status for a single signature
    pub async fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>, SendError> {
        let statuses = self.get_signature_statuses(&[*signature]).await?;
        Ok(statuses.into_iter().next().unwrap_or(None))
    }
    
    /// Maximum signatures per getSignatureStatuses RPC call (Solana limit is 256)
    const MAX_SIGNATURES_PER_BATCH: usize = 256;
    
    /// Check transaction signature statuses in batch
    /// Returns a Vec of Option<bool> where:
    /// - None = not found yet
    /// - Some(true) = confirmed/finalized
    /// - Some(false) = failed with error
    pub async fn get_signature_statuses(&self, signatures: &[Signature]) -> Result<Vec<Option<bool>>, SendError> {
        if signatures.is_empty() {
            return Ok(vec![]);
        }
        
        let mut all_statuses = Vec::with_capacity(signatures.len());
        
        // Process in batches of MAX_SIGNATURES_PER_BATCH
        for chunk in signatures.chunks(Self::MAX_SIGNATURES_PER_BATCH) {
            let sig_strings: Vec<String> = chunk.iter().map(|s| s.to_string()).collect();
            
            let body = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "getSignatureStatuses",
                "params": [
                    sig_strings,
                    { "searchTransactionHistory": false }
                ]
            });
            
            let response = self.client
                .post(&self.rpc_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| SendError::Network(e.to_string()))?;
            
            let json: serde_json::Value = response.json().await
                .map_err(|e| SendError::Parse(e.to_string()))?;
            
            if let Some(error) = json.get("error") {
                return Err(SendError::RpcError(error.to_string()));
            }
            
            // Parse each status in the batch
            let values = json["result"]["value"].as_array()
                .ok_or(SendError::Parse("Expected array in result.value".to_string()))?;
            
            for value in values {
                let status = if value.is_null() {
                    None // Not found
                } else if let Some(err) = value.get("err") {
                    if !err.is_null() {
                        Some(false) // Failed
                    } else if let Some(conf_status) = value.get("confirmationStatus") {
                        let status_str = conf_status.as_str().unwrap_or("");
                        Some(status_str == "confirmed" || status_str == "finalized")
                    } else {
                        None
                    }
                } else if let Some(conf_status) = value.get("confirmationStatus") {
                    let status_str = conf_status.as_str().unwrap_or("");
                    Some(status_str == "confirmed" || status_str == "finalized")
                } else {
                    None
                };
                all_statuses.push(status);
            }
        }
        
        Ok(all_statuses)
    }
    
    /// Send and confirm a transaction via standard RPC
    pub async fn send_and_confirm_rpc(&self, tx: &Transaction, max_retries: u32) -> Result<Signature, SendError> {
        let signature = self.send_rpc(tx).await?;
        
        // Poll for confirmation
        for i in 0..max_retries {
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            match self.get_signature_status(&signature).await {
                Ok(Some(true)) => {
                    return Ok(signature);
                }
                Ok(Some(false)) => {
                    return Err(SendError::TransactionFailed(signature.to_string()));
                }
                Ok(None) => {
                    // Not found yet, keep polling
                    if i % 10 == 0 {
                        // Re-send every 5 seconds
                        let _ = self.send_rpc(tx).await;
                    }
                }
                Err(e) => {
                    // Network error, keep trying
                    if i == max_retries - 1 {
                        return Err(e);
                    }
                }
            }
        }
        
        Err(SendError::Timeout(signature.to_string()))
    }
    
    /// Send a versioned transaction via standard RPC
    pub async fn send_versioned_rpc(&self, tx: &VersionedTransaction) -> Result<Signature, SendError> {
        let tx_bytes = bincode::serialize(tx)
            .map_err(|e| SendError::Serialize(e.to_string()))?;
        let tx_base64 = base64::encode(&tx_bytes);
        
        info!("Sending versioned tx: {} bytes (limit 1232)", tx_bytes.len());
        
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                tx_base64,
                {
                    "encoding": "base64",
                    "skipPreflight": true,
                    "maxRetries": 0
                }
            ]
        });
        
        let response = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .map_err(|e| SendError::Network(e.to_string()))?;
        
        let json: serde_json::Value = response.json().await
            .map_err(|e| SendError::Parse(e.to_string()))?;
        
        if let Some(error) = json.get("error") {
            return Err(SendError::RpcError(error.to_string()));
        }
        
        let sig_str = json["result"].as_str()
            .ok_or(SendError::Parse("No result in response".to_string()))?;
        
        let signature = Signature::from_str(sig_str)
            .map_err(|e| SendError::Parse(e.to_string()))?;
        
        Ok(signature)
    }
    
    /// Send and confirm a versioned transaction via standard RPC
    pub async fn send_and_confirm_versioned_rpc(&self, tx: &VersionedTransaction, max_retries: u32) -> Result<Signature, SendError> {
        let signature = self.send_versioned_rpc(tx).await?;
        
        // Poll for confirmation
        for i in 0..max_retries {
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            match self.get_signature_status(&signature).await {
                Ok(Some(true)) => {
                    return Ok(signature);
                }
                Ok(Some(false)) => {
                    return Err(SendError::TransactionFailed(signature.to_string()));
                }
                Ok(None) => {
                    // Not found yet, keep polling
                    if i % 10 == 0 {
                        // Re-send every 5 seconds
                        let _ = self.send_versioned_rpc(tx).await;
                    }
                }
                Err(e) => {
                    // Network error, keep trying
                    if i == max_retries - 1 {
                        return Err(e);
                    }
                }
            }
        }
        
        Err(SendError::Timeout(signature.to_string()))
    }
    
    /// Send multiple versioned transactions and confirm them in batch
    /// Returns results for each transaction in the same order
    pub async fn send_and_confirm_versioned_batch(
        &self,
        txs: &[VersionedTransaction],
        max_retries: u32,
    ) -> Vec<ConfirmationResult> {
        if txs.is_empty() {
            return vec![];
        }
        
        // Send all transactions first
        let mut signatures: Vec<Option<Signature>> = Vec::with_capacity(txs.len());
        for tx in txs {
            match self.send_versioned_rpc(tx).await {
                Ok(sig) => signatures.push(Some(sig)),
                Err(e) => {
                    info!("Failed to send tx: {}", e);
                    signatures.push(None);
                }
            }
        }
        
        // Track which transactions are still pending
        let mut results: Vec<Option<ConfirmationResult>> = vec![None; txs.len()];
        
        // Mark failed sends
        for (i, sig) in signatures.iter().enumerate() {
            if sig.is_none() {
                results[i] = Some(ConfirmationResult::Failed(
                    Signature::default(),
                    "Failed to send transaction".to_string(),
                ));
            }
        }
        
        // Poll for confirmations in batch
        for retry in 0..max_retries {
            tokio::time::sleep(Duration::from_millis(500)).await;
            
            // Collect pending signatures
            let pending: Vec<(usize, Signature)> = signatures
                .iter()
                .enumerate()
                .filter_map(|(i, sig)| {
                    if results[i].is_none() {
                        sig.map(|s| (i, s))
                    } else {
                        None
                    }
                })
                .collect();
            
            if pending.is_empty() {
                break; // All done
            }
            
            // Batch check statuses
            let pending_sigs: Vec<Signature> = pending.iter().map(|(_, s)| *s).collect();
            match self.get_signature_statuses(&pending_sigs).await {
                Ok(statuses) => {
                    for ((original_idx, sig), status) in pending.iter().zip(statuses.iter()) {
                        match status {
                            Some(true) => {
                                results[*original_idx] = Some(ConfirmationResult::Confirmed(*sig));
                            }
                            Some(false) => {
                                results[*original_idx] = Some(ConfirmationResult::Failed(
                                    *sig,
                                    "Transaction failed".to_string(),
                                ));
                            }
                            None => {
                                // Still pending, re-send periodically
                                if retry % 10 == 0 && retry > 0 {
                                    let _ = self.send_versioned_rpc(&txs[*original_idx]).await;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    info!("Error checking batch statuses: {}", e);
                    // Continue polling on error
                }
            }
        }
        
        // Mark remaining as timeout
        for (i, result) in results.iter_mut().enumerate() {
            if result.is_none() {
                if let Some(sig) = signatures[i] {
                    *result = Some(ConfirmationResult::Timeout(sig));
                }
            }
        }
        
        results.into_iter().filter_map(|r| r).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("Serialization error: {0}")]
    Serialize(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    #[error("Timeout waiting for confirmation: {0}")]
    Timeout(String),
}

/// Confirmation result for batch operations
#[derive(Debug, Clone)]
pub enum ConfirmationResult {
    /// Transaction confirmed successfully
    Confirmed(Signature),
    /// Transaction failed with error
    Failed(Signature, String),
    /// Transaction timed out waiting for confirmation
    Timeout(Signature),
}
