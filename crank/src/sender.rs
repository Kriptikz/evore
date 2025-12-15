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
    
    /// Check transaction signature status
    pub async fn get_signature_status(&self, signature: &Signature) -> Result<Option<bool>, SendError> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignatureStatuses",
            "params": [
                [signature.to_string()],
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
        
        // Check if status exists and has confirmation
        if let Some(value) = json["result"]["value"].get(0) {
            if value.is_null() {
                return Ok(None); // Not found
            }
            if let Some(err) = value.get("err") {
                if !err.is_null() {
                    return Ok(Some(false)); // Failed
                }
            }
            if let Some(status) = value.get("confirmationStatus") {
                let status_str = status.as_str().unwrap_or("");
                return Ok(Some(status_str == "confirmed" || status_str == "finalized"));
            }
        }
        
        Ok(None)
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
