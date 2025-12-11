//! Transaction sender module
//!
//! Handles sending transactions via Helius and Jito endpoints

use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use std::str::FromStr;
use std::time::Duration;

// =============================================================================
// Jito Tip Accounts (mainnet-beta)
// =============================================================================

pub const JITO_TIP_ACCOUNTS: [&str; 10] = [
    "4ACfpUFoaSD9bfPdeu6DBt89gB6ENTeHBXCAi87NhDEE",
    "D2L6yPZ2FmmmTKPgzaMKdhu6EWZcTpLy1Vhx8uvZe7NZ",
    "9bnz4RShgq1hAnLnZbP8kbgBg1kEmcJBYQq3gQbmnSta",
    "5VY91ws6B2hMmBFRsXkoAAdsPHBJwRfBht4DXox3xkwn",
    "2nyhqdwKcJZR2vcqCyrYsaPVdAnFoJjiksCXJ7hfEYgD",
    "2q5pghRs6arqVjRvT5gfgWfWcHWmw1ZuCzphgd5KfWGJ",
    "wyvPkWjVZz1M8fHQnMMCDTQDbkManefNNhweYk5WkcF",
    "3KCKozbAaF75qEU33jtzozcJ29yJuaLJTy2jFdzUY8bT",
    "4vieeGHPYPG2MmyPRcYjdiDmmhN3ww7hsFNap8pVN3Ey",
    "4TQLFNWK8AovT1gFvda5jfw2oJeRMKEmw7aH6MGBJ3or",
];

/// Helius fast sender endpoint - East region (Newark)
pub const HELIUS_EAST_ENDPOINT: &str = "https://mainnet.helius-rpc.com/?api-key=";

/// Jito block engine endpoint - East region (New York)
pub const JITO_EAST_ENDPOINT: &str = "https://ny.mainnet.block-engine.jito.wtf/api/v1/transactions";

/// Jito block engine endpoint - West region (Salt Lake City)
pub const JITO_WEST_ENDPOINT: &str = "https://slc.mainnet.block-engine.jito.wtf/api/v1/transactions";

/// Get a random Jito tip account
pub fn get_random_jito_tip_account() -> Pubkey {
    let idx = rand::random_usize() % JITO_TIP_ACCOUNTS.len();
    Pubkey::from_str(JITO_TIP_ACCOUNTS[idx]).unwrap()
}

/// Transaction sender
pub struct TxSender {
    client: reqwest::Client,
    helius_api_key: Option<String>,
    use_jito: bool,
}

impl TxSender {
    pub fn new(helius_api_key: Option<String>, use_jito: bool) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();
        
        Self {
            client,
            helius_api_key,
            use_jito,
        }
    }
    
    /// Send a transaction via Helius
    pub async fn send_helius(&self, tx: &Transaction) -> Result<Signature, SendError> {
        let api_key = self.helius_api_key.as_ref()
            .ok_or(SendError::NoApiKey)?;
        
        let url = format!("{}{}", HELIUS_EAST_ENDPOINT, api_key);
        
        let tx_bytes = bincode::serialize(tx)
            .map_err(|e| SendError::Serialize(e.to_string()))?;
        let tx_base64 = base64::encode(&tx_bytes);
        
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
            .post(&url)
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
    
    /// Send a transaction via Jito
    pub async fn send_jito(&self, tx: &Transaction) -> Result<Signature, SendError> {
        if !self.use_jito {
            return Err(SendError::Disabled);
        }
        
        let tx_bytes = bincode::serialize(tx)
            .map_err(|e| SendError::Serialize(e.to_string()))?;
        let tx_base64 = base64::encode(&tx_bytes);
        
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "sendTransaction",
            "params": [
                tx_base64,
                {
                    "encoding": "base64"
                }
            ]
        });
        
        // Try East first, then West
        for endpoint in [JITO_EAST_ENDPOINT, JITO_WEST_ENDPOINT] {
            match self.send_jito_endpoint(endpoint, &body).await {
                Ok(sig) => return Ok(sig),
                Err(_) => continue,
            }
        }
        
        Err(SendError::AllEndpointsFailed)
    }
    
    async fn send_jito_endpoint(&self, endpoint: &str, body: &serde_json::Value) -> Result<Signature, SendError> {
        let response = self.client
            .post(endpoint)
            .json(body)
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
    
    /// Send transaction to multiple endpoints
    pub async fn send_all(&self, tx: &Transaction) -> Result<Signature, SendError> {
        // Get signature from transaction
        let signature = tx.signatures.first()
            .ok_or(SendError::NoSignature)?
            .clone();
        
        // Send to Helius (if configured)
        if self.helius_api_key.is_some() {
            let _ = self.send_helius(tx).await;
        }
        
        // Send to Jito (if enabled)
        if self.use_jito {
            let _ = self.send_jito(tx).await;
        }
        
        Ok(signature)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("No API key configured")]
    NoApiKey,
    #[error("Jito sending disabled")]
    Disabled,
    #[error("Transaction has no signature")]
    NoSignature,
    #[error("Serialization error: {0}")]
    Serialize(String),
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("RPC error: {0}")]
    RpcError(String),
    #[error("All endpoints failed")]
    AllEndpointsFailed,
}

// Simple random for tip account selection
mod rand {
    use std::time::{SystemTime, UNIX_EPOCH};
    
    pub fn random_usize() -> usize {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .subsec_nanos() as usize;
        nanos
    }
}
