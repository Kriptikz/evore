//! Drop-in replacement for solana RpcClient that tracks raw response sizes.
//!
//! Same method names, same parameters, same return structure.
//! Only difference: response has `.response_size` with the raw encoded response bytes.

use anyhow::Result;
use base64::Engine;
use serde::Deserialize;
use solana_client::rpc_config::{RpcAccountInfoConfig, RpcProgramAccountsConfig};
use solana_client::rpc_client::GetConfirmedSignaturesForAddress2Config;
use solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature;
use solana_sdk::{account::Account, pubkey::Pubkey, signature::Signature};

/// Response wrapper - same as solana's Response but with response_size added
#[derive(Debug)]
pub struct Response<T> {
    pub context: RpcContext,
    pub value: T,
    pub response_size: usize,
}

#[derive(Debug, Clone)]
pub struct RpcContext {
    pub slot: u64,
}

/// Drop-in replacement for solana_client::nonblocking::rpc_client::RpcClient
pub struct CustomRpcClient {
    url: String,
    client: reqwest::Client,
}

impl CustomRpcClient {
    pub fn new(url: &str) -> Self {
        let normalized_url = if url.starts_with("http") {
            url.to_string()
        } else {
            format!("https://{}", url)
        };
        
        Self {
            url: normalized_url,
            client: reqwest::Client::new(),
        }
    }
    
    /// Make a raw JSON-RPC request, return parsed result and raw response size
    async fn request_raw<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<(T, usize)> {
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });
        
        let resp = self.client
            .post(&self.url)
            .json(&body)
            .send()
            .await?;
        
        let bytes = resp.bytes().await?;
        let response_size = bytes.len();
        
        let json: JsonRpcResponse<T> = serde_json::from_slice(&bytes)?;
        
        if let Some(error) = json.error {
            anyhow::bail!("RPC error {}: {}", error.code, error.message);
        }
        
        let result = json.result.ok_or_else(|| anyhow::anyhow!("No result in response"))?;
        Ok((result, response_size))
    }
    
    // ==================== Account Methods ====================
    
    /// Same signature as RpcClient::get_account_with_config
    pub async fn get_account_with_config(
        &self,
        pubkey: &Pubkey,
        config: RpcAccountInfoConfig,
    ) -> Result<Response<Option<Account>>> {
        let encoding = config.encoding
            .map(|e| encoding_to_str(e))
            .unwrap_or("base64");
        let commitment = config.commitment
            .map(|c| c.commitment.to_string())
            .unwrap_or_else(|| "confirmed".to_string());
        
        let params = serde_json::json!([
            pubkey.to_string(),
            {
                "encoding": encoding,
                "commitment": commitment
            }
        ]);
        
        let (raw_resp, response_size): (RawContextValue<Option<RawUiAccount>>, _) = 
            self.request_raw("getAccountInfo", params).await?;
        
        let account = raw_resp.value
            .map(|ui| decode_ui_account(&ui, encoding))
            .transpose()?;
        
        Ok(Response {
            context: RpcContext { slot: raw_resp.context.slot },
            value: account,
            response_size,
        })
    }
    
    /// Same signature as RpcClient::get_multiple_accounts_with_config
    pub async fn get_multiple_accounts_with_config(
        &self,
        pubkeys: &[Pubkey],
        config: RpcAccountInfoConfig,
    ) -> Result<Response<Vec<Option<Account>>>> {
        let encoding = config.encoding
            .map(|e| encoding_to_str(e))
            .unwrap_or("base64");
        let commitment = config.commitment
            .map(|c| c.commitment.to_string())
            .unwrap_or_else(|| "confirmed".to_string());
        
        let keys: Vec<String> = pubkeys.iter().map(|p| p.to_string()).collect();
        
        let params = serde_json::json!([
            keys,
            {
                "encoding": encoding,
                "commitment": commitment
            }
        ]);
        
        let (raw_resp, response_size): (RawContextValue<Vec<Option<RawUiAccount>>>, _) = 
            self.request_raw("getMultipleAccounts", params).await?;
        
        let accounts: Vec<Option<Account>> = raw_resp.value
            .into_iter()
            .map(|opt| opt.map(|ui| decode_ui_account(&ui, encoding)).transpose())
            .collect::<Result<Vec<_>>>()?;
        
        Ok(Response {
            context: RpcContext { slot: raw_resp.context.slot },
            value: accounts,
            response_size,
        })
    }
    
    /// Same signature as RpcClient::get_program_accounts_with_config
    pub async fn get_program_accounts_with_config(
        &self,
        program_id: &Pubkey,
        config: RpcProgramAccountsConfig,
    ) -> Result<(Vec<(Pubkey, Account)>, usize)> {
        let encoding = config.account_config.encoding
            .map(|e| encoding_to_str(e))
            .unwrap_or("base64");
        let commitment = config.account_config.commitment
            .map(|c| c.commitment.to_string())
            .unwrap_or_else(|| "confirmed".to_string());
        
        let mut rpc_config = serde_json::json!({
            "encoding": encoding,
            "commitment": commitment
        });
        
        if let Some(filters) = &config.filters {
            let filter_json: Vec<serde_json::Value> = filters.iter().map(|f| {
                match f {
                    solana_client::rpc_filter::RpcFilterType::DataSize(size) => {
                        serde_json::json!({ "dataSize": size })
                    }
                    solana_client::rpc_filter::RpcFilterType::Memcmp(m) => {
                        let bytes_str = m.bytes()
                            .map(|b| bs58::encode(&*b).into_string())
                            .unwrap_or_default();
                        serde_json::json!({
                            "memcmp": {
                                "offset": m.offset(),
                                "bytes": bytes_str
                            }
                        })
                    }
                    _ => serde_json::json!(null),
                }
            }).collect();
            rpc_config["filters"] = serde_json::Value::Array(filter_json);
        }
        
        let params = serde_json::json!([
            program_id.to_string(),
            rpc_config
        ]);
        
        let (raw_accounts, response_size): (Vec<RawProgramAccount>, _) = 
            self.request_raw("getProgramAccounts", params).await?;
        
        let accounts: Vec<(Pubkey, Account)> = raw_accounts
            .into_iter()
            .map(|pa| {
                let pubkey = pa.pubkey.parse::<Pubkey>()?;
                let account = decode_ui_account(&pa.account, encoding)?;
                Ok((pubkey, account))
            })
            .collect::<Result<Vec<_>>>()?;
        
        // Return tuple (accounts, size) - caller extracts size for logging
        Ok((accounts, response_size))
    }
    
    // ==================== Signature Methods ====================
    
    /// Same signature as RpcClient::get_signatures_for_address_with_config
    pub async fn get_signatures_for_address_with_config(
        &self,
        address: &Pubkey,
        config: GetConfirmedSignaturesForAddress2Config,
    ) -> Result<(Vec<RpcConfirmedTransactionStatusWithSignature>, usize)> {
        let commitment = config.commitment
            .map(|c| c.commitment.to_string())
            .unwrap_or_else(|| "confirmed".to_string());
        
        let mut rpc_config = serde_json::json!({
            "commitment": commitment
        });
        
        if let Some(before) = config.before {
            rpc_config["before"] = serde_json::Value::String(before.to_string());
        }
        if let Some(until) = config.until {
            rpc_config["until"] = serde_json::Value::String(until.to_string());
        }
        if let Some(limit) = config.limit {
            rpc_config["limit"] = serde_json::Value::Number(limit.into());
        }
        
        let params = serde_json::json!([
            address.to_string(),
            rpc_config
        ]);
        
        let (sigs, response_size): (Vec<RpcConfirmedTransactionStatusWithSignature>, _) = 
            self.request_raw("getSignaturesForAddress", params).await?;
        
        Ok((sigs, response_size))
    }
    
    /// Simple get_signatures_for_address (no config)
    pub async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
    ) -> Result<(Vec<RpcConfirmedTransactionStatusWithSignature>, usize)> {
        let params = serde_json::json!([
            address.to_string(),
            { "commitment": "confirmed" }
        ]);
        
        let (sigs, response_size): (Vec<RpcConfirmedTransactionStatusWithSignature>, _) = 
            self.request_raw("getSignaturesForAddress", params).await?;
        
        Ok((sigs, response_size))
    }
    
    // ==================== Transaction Methods ====================
    
    /// Get transaction by signature - returns raw JSON
    pub async fn get_transaction(
        &self,
        signature: &Signature,
        commitment: &str,
    ) -> Result<(Option<serde_json::Value>, usize)> {
        let params = serde_json::json!([
            signature.to_string(),
            {
                "encoding": "json",
                "commitment": commitment,
                "maxSupportedTransactionVersion": 0
            }
        ]);
        
        let (result, response_size): (Option<serde_json::Value>, _) = 
            self.request_raw("getTransaction", params).await?;
        
        Ok((result, response_size))
    }
    
    // ==================== Other Methods ====================
    
    /// Get SOL balance
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        let params = serde_json::json!([
            pubkey.to_string(),
            { "commitment": "confirmed" }
        ]);
        
        let (raw_resp, _): (RawContextValue<u64>, _) = 
            self.request_raw("getBalance", params).await?;
        
        Ok(raw_resp.value)
    }
    
    /// Get current slot
    pub async fn get_slot(&self) -> Result<u64> {
        let params = serde_json::json!([{ "commitment": "confirmed" }]);
        
        let (slot, _): (u64, _) = self.request_raw("getSlot", params).await?;
        
        Ok(slot)
    }
    
    /// Generic send method for any RPC request
    pub async fn send<T: for<'de> Deserialize<'de>>(
        &self,
        request: solana_client::rpc_request::RpcRequest,
        params: serde_json::Value,
    ) -> Result<T> {
        let method = request.to_string();
        let (result, _) = self.request_raw(&method, params).await?;
        Ok(result)
    }
}

// ==================== Internal Types ====================

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct RawContextValue<T> {
    context: RawContext,
    value: T,
}

#[derive(Debug, Deserialize)]
struct RawContext {
    slot: u64,
}

#[derive(Debug, Deserialize)]
struct RawUiAccount {
    data: serde_json::Value,
    lamports: u64,
    owner: String,
    executable: bool,
    #[serde(rename = "rentEpoch")]
    rent_epoch: u64,
}

#[derive(Debug, Deserialize)]
struct RawProgramAccount {
    pubkey: String,
    account: RawUiAccount,
}


// ==================== Helpers ====================

fn encoding_to_str(encoding: solana_account_decoder_client_types::UiAccountEncoding) -> &'static str {
    use solana_account_decoder_client_types::UiAccountEncoding;
    match encoding {
        UiAccountEncoding::Base58 => "base58",
        UiAccountEncoding::Base64 => "base64",
        UiAccountEncoding::Base64Zstd => "base64+zstd",
        UiAccountEncoding::JsonParsed => "jsonParsed",
        _ => "base64",
    }
}

fn decode_ui_account(ui: &RawUiAccount, encoding: &str) -> Result<Account> {
    // Data comes as array: [encoded_string, encoding] or just string
    let encoded = match &ui.data {
        serde_json::Value::Array(arr) if !arr.is_empty() => {
            arr[0].as_str().ok_or_else(|| anyhow::anyhow!("Invalid data format"))?
        }
        serde_json::Value::String(s) => s.as_str(),
        _ => anyhow::bail!("Unexpected data format: {:?}", ui.data),
    };
    
    let data = match encoding {
        "base64" => base64::engine::general_purpose::STANDARD.decode(encoded)?,
        "base64+zstd" => {
            let compressed = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            zstd::decode_all(compressed.as_slice())?
        }
        "base58" => bs58::decode(encoded).into_vec()?,
        _ => anyhow::bail!("Unsupported encoding: {}", encoding),
    };
    
    Ok(Account {
        lamports: ui.lamports,
        data,
        owner: ui.owner.parse()?,
        executable: ui.executable,
        rent_epoch: ui.rent_epoch,
    })
}
