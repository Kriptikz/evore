use std::{mem, sync::Arc, time::Duration};

use base64::Engine as _;
use tracing;
use evore::ore_api::{self, Automate, Deploy, OreInstruction, AutomationStrategy};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{bs58, keccak::hashv};
use steel::{Pod, ProgramError, Pubkey, Zeroable};
use thiserror::Error;
use tokio::time::Instant;

use crate::app_state::{AutomationCache, ReconstructedAutomation};
use crate::clickhouse::{ClickHouseClient, RpcRequestInsert};

#[derive(Debug, Error)]
pub enum HeliusError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("rpc error: {0}")]
    Rpc(String),

    #[error("invalid rpc response: {0}")]
    InvalidResponse(String),

    #[error("program error: {0}")]
    Program(#[from] ProgramError),

    #[error("decode error: {0}")]
    Decode(String),
}


/// Wrapper for talking to the Helius RPC endpoint.
#[derive(Clone)]
pub struct HeliusApi {
    rpc_url: String,
    client: Client,
    last_request_at: Instant,
    
    // Metrics tracking
    clickhouse: Option<Arc<ClickHouseClient>>,
    provider_name: String,
    api_key_id: String,
}

impl HeliusApi {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self::with_clickhouse(rpc_url, None)
    }
    
    pub fn with_clickhouse(rpc_url: impl Into<String>, clickhouse: Option<Arc<ClickHouseClient>>) -> Self {
        let url = rpc_url.into();
        let full_url = if url.starts_with("http") {
            url
        } else {
            format!("https://{}", url)
        };
        
        // Extract provider name from URL for metrics
        let provider_name = extract_provider_name(&full_url);
        
        // Extract API key ID if present (for Helius URLs like xxx?api-key=abc)
        let api_key_id = extract_api_key_id(&full_url);
        
        Self {
            rpc_url: full_url,
            client: Client::new(),
            last_request_at: Instant::now(),
            clickhouse,
            provider_name,
            api_key_id,
        }
    }
    
    /// Log successful RPC call to ClickHouse
    fn log_success(&self, method: &str, target_type: &str, target_address: &str, duration_ms: u32, result_count: u32, response_size: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                "ore-stats",
                &self.provider_name,
                &self.api_key_id,
                method,
                target_type,
            )
            .with_target(target_address)
            .success(duration_ms, result_count, response_size);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log HeliusApi RPC metrics: {}", e);
                }
            });
        }
    }
    
    /// Log paginated RPC call to ClickHouse
    fn log_paginated_success(&self, method: &str, target_type: &str, target_address: &str, page_num: u16, cursor: &str, duration_ms: u32, result_count: u32, response_size: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                "ore-stats",
                &self.provider_name,
                &self.api_key_id,
                method,
                target_type,
            )
            .with_target(target_address)
            .with_pagination(page_num, cursor)
            .success(duration_ms, result_count, response_size);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log HeliusApi RPC metrics: {}", e);
                }
            });
        }
    }
    
    /// Log paginated RPC call with filters to ClickHouse
    fn log_filtered_success(&self, method: &str, target_type: &str, target_address: &str, page_num: u16, cursor: &str, filters_json: &str, duration_ms: u32, result_count: u32, response_size: u32) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                "ore-stats",
                &self.provider_name,
                &self.api_key_id,
                method,
                target_type,
            )
            .with_target(target_address)
            .with_pagination(page_num, cursor)
            .with_filters(filters_json)
            .success(duration_ms, result_count, response_size);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log HeliusApi RPC metrics: {}", e);
                }
            });
        }
    }
    
    /// Log error RPC call to ClickHouse
    fn log_error(&self, method: &str, target_type: &str, target_address: &str, duration_ms: u32, error: &str) {
        if let Some(ref ch) = self.clickhouse {
            let insert = RpcRequestInsert::new(
                "ore-stats",
                &self.provider_name,
                &self.api_key_id,
                method,
                target_type,
            )
            .with_target(target_address)
            .error(duration_ms, "", error);
            
            let ch = ch.clone();
            tokio::spawn(async move {
                if let Err(e) = ch.insert_rpc_metric(insert).await {
                    tracing::warn!("Failed to log HeliusApi RPC metrics: {}", e);
                }
            });
        }
    }

    /// Fetch a single *page* of transactions for a given round.
    ///
    /// - `round_id` – the ORE round id (u64)
    /// - `pagination_token` – token returned by the previous page (if any)
    /// - `limit` – max number of entries to fetch (Helius caps this; 100 is typical for full txs)
    ///
    /// Returns:
    /// - `transactions`: a Vec of full transaction JSONs
    /// - `pagination_token`: Some(token) if there is a next page, None otherwise
    pub async fn get_transactions_for_round(
        &mut self,
        round_id: u64,
        pagination_token: Option<String>,
    ) -> Result<RoundTransactionsPage, HeliusError> {
        // Derive the round PDA from the round id using ore-api.
        let (round_pda, _bump) = ore_api::round_pda(round_id);

        let page = self
            .get_transactions_for_address(&round_pda, pagination_token.clone(), Some(100), None, None, None)
            .await?;

        Ok(RoundTransactionsPage {
            transactions: page.transactions,
            pagination_token: page.pagination_token,
        })
    }

        /// Wrapper around Helius `getTransactionsForAddress` with:
    /// - transactionDetails: "full"
    /// - encoding: "json"
    /// - configurable sortOrder ("asc" or "desc")
    /// - filters.status = "succeeded"
    /// - optional slot range (gte / lte)
    pub async fn get_transactions_for_address(
        &mut self,
        address: &Pubkey,
        pagination_token: Option<String>,
        limit: Option<u32>,
        sort_order: Option<&str>,   // "asc" or "desc"
        slot_gte: Option<u64>,      // optional slot >=
        slot_lte: Option<u64>,      // optional slot <=
    ) -> Result<AddressTransactionsPage, HeliusError> {
        if self.last_request_at.elapsed().as_millis() < 200 {
            tokio::time::sleep(Duration::from_millis((200 - self.last_request_at.elapsed().as_millis()) as u64)).await;
        }
        self.last_request_at = Instant::now();
        // Build filters object
        let mut slot_filter = serde_json::Map::new();
        if let Some(gte) = slot_gte {
            slot_filter.insert("gte".to_string(), json!(gte));
        }
        if let Some(lte) = slot_lte {
            slot_filter.insert("lte".to_string(), json!(lte));
        }

        let mut filters_obj = serde_json::Map::new();

        if !slot_filter.is_empty() {
            filters_obj.insert("slot".to_string(), Value::Object(slot_filter));
        }

        // Always request only successful txns for our use case
        filters_obj.insert("status".to_string(), json!("succeeded"));

        let filters_value = if filters_obj.is_empty() {
            None
        } else {
            Some(Value::Object(filters_obj))
        };

        // Save cursor string before moving pagination_token
        let cursor_str = pagination_token.as_deref().unwrap_or("").to_string();
        
        // Request options for getTransactionsForAddress
        let opts = GetTransactionsOptions {
            transaction_details: "full".to_string(),
            encoding: Some("json".to_string()),
            sort_order: Some(sort_order.unwrap_or("asc").to_string()),
            limit: limit.unwrap_or(100),
            pagination_token,
            commitment: Some("finalized".to_string()),
            filters: filters_value,
        };

        // JSON-RPC body
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTransactionsForAddress",
            "params": [
                address.to_string(),
                opts
            ]
        });

        let start = Instant::now();
        let request_size = body.to_string().len() as u32;
        
        let response = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        
        let response_bytes = response.bytes().await?;
        let duration_ms = start.elapsed().as_millis() as u32;
        let response_size = response_bytes.len() as u32;
        
        let resp: RpcResponse<GetTransactionsResult> = serde_json::from_slice(&response_bytes)
            .map_err(|e| HeliusError::InvalidResponse(format!("JSON parse error: {}", e)))?;

        if let Some(err) = resp.error {
            self.log_error("getTransactionsForAddress", "address", &address.to_string(), duration_ms, &err.message);
            return Err(HeliusError::InvalidResponse(format!(
                "code: {}, message: {}, data: {:?}",
                err.code, err.message, err.data
            )));
        }

        let result = resp.result.ok_or_else(|| {
            self.log_error("getTransactionsForAddress", "address", &address.to_string(), duration_ms, "null result");
            HeliusError::InvalidResponse("result is null with no error".to_string())
        })?;

        let tx_count = result.data.len() as u32;
        self.log_paginated_success("getTransactionsForAddress", "address", &address.to_string(), 0, &cursor_str, duration_ms, tx_count, response_size);
        
        Ok(AddressTransactionsPage {
            transactions: result.data,
            pagination_token: result.pagination_token,
        })
    }

    /// For a given miner *authority* and a cutoff slot, compute automation state as of that slot.
    ///
    /// Uses an AutomationCache to avoid rescanning history:
    /// - If prev_cache is None or prev_cache.last_updated_slot == 0:
    ///     heavy DESC search (slot <= cutoff_slot), stop at first Automate ix.
    /// - Else:
    ///     light ASC search from (last_updated_slot + 1) up to cutoff_slot,
    ///     applying any Automate ix in chronological order.
    ///
    /// Returns:
    /// - (None, cache)  -> automation OFF as of cutoff_slot.
    /// - (Some(auto), cache) -> automation ON as of cutoff_slot.
    pub async fn get_latest_automate_for_authority_up_to_slot(
        &mut self,
        authority: &Pubkey,
        cutoff_slot: u64,
        prev_cache: Option<AutomationCache>,
    ) -> Result<(Option<ReconstructedAutomation>, AutomationCache), HeliusError> {
        // Early out if cutoff is nonsense
        if cutoff_slot == 0 {
            let cache = prev_cache.unwrap_or_else(|| AutomationCache::new(*authority));
            // Whatever cache says, we can't advance; just interpret its state
            if !cache.active {
                return Ok((None, cache));
            }
            let strat_enum = AutomationStrategy::from_u64(cache.strategy);
            let auto = ReconstructedAutomation {
                amount: cache.amount,
                authority: cache.authority,
                executor: cache.executor,
                fee: cache.fee,
                strategy: strat_enum,
                mask: cache.mask,
            };
            return Ok((Some(auto), cache));
        }

        let mut cache = prev_cache.unwrap_or_else(|| AutomationCache::new(*authority));

        // Automation PDA derived from authority
        let (automation_pda, _bump) = ore_api::automation_pda(*authority);

        // Helper: decode an Automate ix and return (Automate struct, executor, is_close)
        fn decode_automate_from_ix(
            ix: &Value,
            account_keys: &[Pubkey],
            expected_automation_pda: &Pubkey,
            expected_authority: &Pubkey,
        ) -> Result<Option<(Automate, Pubkey, bool)>, HeliusError> {
            // programIdIndex → ore program check
            let program_id_index = ix
                .get("programIdIndex")
                .and_then(Value::as_u64)
                .ok_or_else(|| HeliusError::Decode("missing programIdIndex".into()))? as usize;

            let program_id = account_keys
                .get(program_id_index)
                .ok_or_else(|| HeliusError::Decode("programIdIndex out of range".into()))?;

            if *program_id != ore_api::PROGRAM_ID {
                return Ok(None);
            }

            let data_str = ix
                .get("data")
                .and_then(Value::as_str)
                .ok_or_else(|| HeliusError::Decode("missing data".into()))?;

            let data = match bs58::decode(data_str).into_vec() {
                Ok(d) => d,
                Err(_) => return Ok(None),
            };

            if data.is_empty() {
                return Ok(None);
            }

            let tag = data[0];
            let ore_tag = match OreInstruction::try_from(tag) {
                Ok(t) => t,
                Err(_) => return Ok(None),
            };
            if ore_tag != OreInstruction::Automate {
                return Ok(None);
            }

            const AUTOMATE_BODY_SIZE: usize = core::mem::size_of::<Automate>();
            if data.len() < 1 + AUTOMATE_BODY_SIZE {
                return Ok(None);
            }

            let body = &data[1..1 + AUTOMATE_BODY_SIZE];
            let automate: &Automate = bytemuck::from_bytes(body);

            // Accounts: [signer, automation_info, executor_info, miner_info, system_program]
            let accounts = ix
                .get("accounts")
                .and_then(Value::as_array)
                .ok_or_else(|| HeliusError::Decode("missing accounts".into()))?;

            let get_key = |ix_index: usize| -> Result<Pubkey, HeliusError> {
                let acc_idx = accounts
                    .get(ix_index)
                    .and_then(Value::as_u64)
                    .ok_or_else(|| HeliusError::Decode("bad account index".into()))? as usize;

                let key = account_keys
                    .get(acc_idx)
                    .ok_or_else(|| HeliusError::Decode("account index out of range".into()))?;
                Ok(*key)
            };

            let signer = get_key(0)?;        // authority
            let autom_acc = get_key(1)?;     // automation_info
            let executor = get_key(2)?;      // executor_info

            // Must match the expected automation PDA & authority
            if autom_acc != *expected_automation_pda {
                return Ok(None);
            }
            if signer != *expected_authority {
                return Ok(None);
            }

            // Close when executor == Pubkey::default()
            let is_close = executor == Pubkey::default();

            Ok(Some((*automate, executor, is_close)))
        }

        // If cache already covers this cutoff, just interpret its state
        if cache.last_updated_slot >= cutoff_slot {
            if !cache.active {
                return Ok((None, cache));
            }
            let strat_enum = AutomationStrategy::from_u64(cache.strategy);
            let auto = ReconstructedAutomation {
                amount: cache.amount,
                authority: cache.authority,
                executor: cache.executor,
                fee: cache.fee,
                strategy: strat_enum,
                mask: cache.mask,
            };
            return Ok((Some(auto), cache));
        }

        // Decide mode:
        // - If we've never seen any automation history → heavy DESC search up to cutoff_slot.
        // - Else → ASC search from (last_updated_slot + 1) to cutoff_slot applying changes.
        let use_desc = cache.last_updated_slot == 0;

        if use_desc {
            // ─────────────────────────────────────────────
            // First-time heavy path: DESC, slot <= cutoff
            // ─────────────────────────────────────────────
            let mut pagination_token: Option<String> = None;

            loop {
                let page = self
                    .get_transactions_for_address(
                        &automation_pda,
                        pagination_token.clone(),
                        Some(100),
                        Some("desc"),
                        None,               // slot_gte
                        Some(cutoff_slot),  // slot_lte
                    )
                    .await?;

                if page.transactions.is_empty() {
                    // We scanned ≤ cutoff and found no Automate at all
                    cache.active = false;
                    cache.last_updated_slot = cutoff_slot;
                    return Ok((None, cache));
                }

                for tx in &page.transactions {
                    let slot = tx
                        .get("slot")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);

                    let message = tx
                        .get("transaction")
                        .and_then(|t| t.get("message"))
                        .ok_or_else(|| HeliusError::Decode("missing message".into()))?;

                    let account_keys_json = message
                        .get("accountKeys")
                        .and_then(Value::as_array)
                        .ok_or_else(|| HeliusError::Decode("missing accountKeys".into()))?;

                    let mut account_keys = Vec::with_capacity(account_keys_json.len());
                    for k in account_keys_json {
                        let s = k
                            .as_str()
                            .ok_or_else(|| HeliusError::Decode("account key not string".into()))?;
                        let pk = Pubkey::try_from(s)
                            .map_err(|_| HeliusError::Decode("invalid pubkey".into()))?;
                        account_keys.push(pk);
                    }

                    // OUTER instructions
                    if let Some(ixs) = message
                        .get("instructions")
                        .and_then(Value::as_array)
                    {
                        for ix in ixs {
                            if let Some((automate, executor, is_close)) =
                                decode_automate_from_ix(ix, &account_keys, &automation_pda, authority)?
                            {
                                cache.last_updated_slot = slot;

                                if is_close {
                                    cache.active = false;
                                    cache.mask = 0;
                                    cache.strategy = 0;
                                    cache.amount = 0;
                                    cache.fee = 0;
                                    cache.executor = Pubkey::default();
                                    return Ok((None, cache));
                                } else {
                                    cache.active = true;
                                    cache.mask = u64::from_le_bytes(automate.mask);
                                    cache.strategy = automate.strategy as u64;
                                    cache.amount = u64::from_le_bytes(automate.amount);
                                    cache.fee = u64::from_le_bytes(automate.fee);
                                    cache.executor = executor;

                                    let strat_enum =
                                        AutomationStrategy::from_u64(cache.strategy);
                                    let auto = ReconstructedAutomation {
                                        amount: cache.amount,
                                        authority: cache.authority,
                                        executor: cache.executor,
                                        fee: cache.fee,
                                        strategy: strat_enum,
                                        mask: cache.mask,
                                    };
                                    return Ok((Some(auto), cache));
                                }
                            }
                        }
                    }

                    // INNER instructions (if Automate ever appears there)
                    if let Some(inner_arr) = tx
                        .get("meta")
                        .and_then(|m| m.get("innerInstructions"))
                        .and_then(Value::as_array)
                    {
                        for inner in inner_arr {
                            if let Some(ixs) = inner
                                .get("instructions")
                                .and_then(Value::as_array)
                            {
                                for ix in ixs {
                                    if let Some((automate, executor, is_close)) =
                                        decode_automate_from_ix(
                                            ix,
                                            &account_keys,
                                            &automation_pda,
                                            authority,
                                        )?
                                    {
                                        cache.last_updated_slot = slot;

                                        if is_close {
                                            cache.active = false;
                                            cache.mask = 0;
                                            cache.strategy = 0;
                                            cache.amount = 0;
                                            cache.fee = 0;
                                            cache.executor = Pubkey::default();
                                            return Ok((None, cache));
                                        } else {
                                            cache.active = true;
                                            cache.mask = u64::from_le_bytes(automate.mask);
                                            cache.strategy = automate.strategy as u64;
                                            cache.amount = u64::from_le_bytes(automate.amount);
                                            cache.fee = u64::from_le_bytes(automate.fee);
                                            cache.executor = executor;

                                            let strat_enum =
                                                AutomationStrategy::from_u64(cache.strategy);
                                            let auto = ReconstructedAutomation {
                                                amount: cache.amount,
                                                authority: cache.authority,
                                                executor: cache.executor,
                                                fee: cache.fee,
                                                strategy: strat_enum,
                                                mask: cache.mask,
                                            };
                                            return Ok((Some(auto), cache));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                pagination_token = page.pagination_token;
                if pagination_token.is_none() {
                    // No Automate at all ≤ cutoff
                    cache.active = false;
                    cache.last_updated_slot = cutoff_slot;
                    return Ok((None, cache));
                }
            }
        } else {
            // ─────────────────────────────────────────────
            // Cached path: ASC from (last_updated_slot + 1) up to cutoff_slot
            // ─────────────────────────────────────────────
            let start_slot = cache.last_updated_slot.saturating_add(1);
            if start_slot > cutoff_slot {
                // Cache already covers this cutoff
                if !cache.active {
                    return Ok((None, cache));
                }
                let strat_enum = AutomationStrategy::from_u64(cache.strategy);
                let auto = ReconstructedAutomation {
                    amount: cache.amount,
                    authority: cache.authority,
                    executor: cache.executor,
                    fee: cache.fee,
                    strategy: strat_enum,
                    mask: cache.mask,
                };
                return Ok((Some(auto), cache));
            }

            let mut pagination_token: Option<String> = None;

            loop {
                let page = self
                    .get_transactions_for_address(
                        &automation_pda,
                        pagination_token.clone(),
                        Some(100),
                        Some("asc"),
                        Some(start_slot),  // slot_gte
                        Some(cutoff_slot), // slot_lte
                    )
                    .await?;

                if page.transactions.is_empty() {
                    break;
                }

                for tx in &page.transactions {
                    let slot = tx
                        .get("slot")
                        .and_then(Value::as_u64)
                        .unwrap_or(0);

                    let message = tx
                        .get("transaction")
                        .and_then(|t| t.get("message"))
                        .ok_or_else(|| HeliusError::Decode("missing message".into()))?;

                    let account_keys_json = message
                        .get("accountKeys")
                        .and_then(Value::as_array)
                        .ok_or_else(|| HeliusError::Decode("missing accountKeys".into()))?;

                    let mut account_keys = Vec::with_capacity(account_keys_json.len());
                    for k in account_keys_json {
                        let s = k
                            .as_str()
                            .ok_or_else(|| HeliusError::Decode("account key not string".into()))?;
                        let pk = Pubkey::try_from(s)
                            .map_err(|_| HeliusError::Decode("invalid pubkey".into()))?;
                        account_keys.push(pk);
                    }

                    // OUTER
                    if let Some(ixs) = message
                        .get("instructions")
                        .and_then(Value::as_array)
                    {
                        for ix in ixs {
                            if let Some((automate, executor, is_close)) =
                                decode_automate_from_ix(ix, &account_keys, &automation_pda, authority)?
                            {
                                cache.last_updated_slot = slot;

                                if is_close {
                                    cache.active = false;
                                    cache.mask = 0;
                                    cache.strategy = 0;
                                    cache.amount = 0;
                                    cache.fee = 0;
                                    cache.executor = Pubkey::default();
                                } else {
                                    cache.active = true;
                                    cache.mask = u64::from_le_bytes(automate.mask);
                                    cache.strategy = automate.strategy as u64;
                                    cache.amount = u64::from_le_bytes(automate.amount);
                                    cache.fee = u64::from_le_bytes(automate.fee);
                                    cache.executor = executor;
                                }
                            }
                        }
                    }

                    // INNER
                    if let Some(inner_arr) = tx
                        .get("meta")
                        .and_then(|m| m.get("innerInstructions"))
                        .and_then(Value::as_array)
                    {
                        for inner in inner_arr {
                            if let Some(ixs) = inner
                                .get("instructions")
                                .and_then(Value::as_array)
                            {
                                for ix in ixs {
                                    if let Some((automate, executor, is_close)) =
                                        decode_automate_from_ix(
                                            ix,
                                            &account_keys,
                                            &automation_pda,
                                            authority,
                                        )?
                                    {
                                        cache.last_updated_slot = slot;

                                        if is_close {
                                            cache.active = false;
                                            cache.mask = 0;
                                            cache.strategy = 0;
                                            cache.amount = 0;
                                            cache.fee = 0;
                                            cache.executor = Pubkey::default();
                                        } else {
                                            cache.active = true;
                                            cache.mask = u64::from_le_bytes(automate.mask);
                                            cache.strategy = automate.strategy as u64;
                                            cache.amount = u64::from_le_bytes(automate.amount);
                                            cache.fee = u64::from_le_bytes(automate.fee);
                                            cache.executor = executor;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                pagination_token = page.pagination_token;
                if pagination_token.is_none() {
                    break;
                }
            }

            // After extending cache up to cutoff_slot, just interpret cache
            if !cache.active {
                return Ok((None, cache));
            }
            let strat_enum = AutomationStrategy::from_u64(cache.strategy);
            let auto = ReconstructedAutomation {
                amount: cache.amount,
                authority: cache.authority,
                executor: cache.executor,
                fee: cache.fee,
                strategy: strat_enum,
                mask: cache.mask,
            };
            Ok((Some(auto), cache))
        }
    }

    /// Scan backwards from cutoff_slot looking for the latest Automate instruction.
    /// If stop_at_slot is provided, stop scanning when we reach that slot (early exit).
    /// Returns (None, cache) if no Automate found before reaching stop_at_slot.
    /// The caller should check if None is returned and use a fallback if available.
    pub async fn get_latest_automate_for_authority_up_to_slot_with_stop(
        &mut self,
        authority: &Pubkey,
        cutoff_slot: u64,
        stop_at_slot: Option<u64>,
    ) -> Result<(Option<ReconstructedAutomation>, AutomationCache), HeliusError> {
        let automation_pda = ore_api::automation_pda(*authority).0;
        let mut cache = AutomationCache::new(*authority);

        /// Decode an Automate instruction from an instruction JSON value.
        /// Returns Some((automate_data, executor, is_close)) if it's an Automate ix.
        fn decode_automate_from_ix(
            ix: &Value,
            account_keys: &[Pubkey],
            automation_pda: &Pubkey,
            authority: &Pubkey,
        ) -> Result<Option<(Automate, Pubkey, bool)>, HeliusError> {
            let prog_idx = ix
                .get("programIdIndex")
                .and_then(Value::as_u64)
                .unwrap_or(u64::MAX) as usize;
            let program_id = account_keys
                .get(prog_idx)
                .copied()
                .unwrap_or(Pubkey::default());

            if program_id != ore_api::PROGRAM_ID {
                return Ok(None);
            }

            let data = ix
                .get("data")
                .and_then(Value::as_str)
                .map(bs58::decode)
                .and_then(|d| d.into_vec().ok())
                .unwrap_or_default();

            if data.is_empty() {
                return Ok(None);
            }

            // Discriminator for Automate
            if data[0] != 7 {
                return Ok(None);
            }

            let accts_arr = ix
                .get("accounts")
                .and_then(Value::as_array)
                .ok_or_else(|| HeliusError::Decode("missing ix accounts".into()))?;

            // Automate ix has 4 accounts: [authority, automation, executor, system_program]
            if accts_arr.len() < 4 {
                return Ok(None);
            }

            let auth_idx = accts_arr[0].as_u64().unwrap_or(u64::MAX) as usize;
            let auto_idx = accts_arr[1].as_u64().unwrap_or(u64::MAX) as usize;
            let exec_idx = accts_arr[2].as_u64().unwrap_or(u64::MAX) as usize;

            let auth_pk = account_keys
                .get(auth_idx)
                .copied()
                .unwrap_or(Pubkey::default());
            let auto_pk = account_keys
                .get(auto_idx)
                .copied()
                .unwrap_or(Pubkey::default());
            let executor = account_keys
                .get(exec_idx)
                .copied()
                .unwrap_or(Pubkey::default());

            if auth_pk != *authority || auto_pk != *automation_pda {
                return Ok(None);
            }

            // Check for Close (len == 1) vs Open (len == 1 + 32)
            let is_close = data.len() == 1;
            if is_close {
                return Ok(Some((Automate::zeroed(), executor, true)));
            }

            if data.len() < 1 + mem::size_of::<Automate>() {
                return Err(HeliusError::Decode("automate data too short".into()));
            }

            let automate = bytemuck::try_from_bytes::<Automate>(&data[1..])
                .map_err(|_| HeliusError::Decode("bad automate cast".into()))?;

            Ok(Some((*automate, executor, is_close)))
        }

        if cutoff_slot == 0 {
            cache.active = false;
            return Ok((None, cache));
        }

        let mut pagination_token: Option<String> = None;

        loop {
            let page = self
                .get_transactions_for_address(
                    &automation_pda,
                    pagination_token.clone(),
                    Some(100),
                    Some("desc"),
                    None,               // slot_gte (no lower bound initially)
                    Some(cutoff_slot),  // slot_lte
                )
                .await?;

            if page.transactions.is_empty() {
                // We scanned all the way back and found no Automate
                cache.active = false;
                cache.last_updated_slot = cutoff_slot;
                return Ok((None, cache));
            }

            for tx in &page.transactions {
                let slot = tx
                    .get("slot")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);

                // Early stop check: if we've reached the stop_at_slot, return None
                // The caller should use the cached fallback state
                if let Some(stop) = stop_at_slot {
                    if slot <= stop {
                        tracing::debug!(
                            "Reached stop_at_slot {} (current tx slot {}), returning None for fallback",
                            stop, slot
                        );
                        cache.active = false;
                        cache.last_updated_slot = cutoff_slot;
                        return Ok((None, cache));
                    }
                }

                let message = tx
                    .get("transaction")
                    .and_then(|t| t.get("message"))
                    .ok_or_else(|| HeliusError::Decode("missing message".into()))?;

                let account_keys_json = message
                    .get("accountKeys")
                    .and_then(Value::as_array)
                    .ok_or_else(|| HeliusError::Decode("missing accountKeys".into()))?;

                let mut account_keys = Vec::with_capacity(account_keys_json.len());
                for k in account_keys_json {
                    let s = k
                        .as_str()
                        .ok_or_else(|| HeliusError::Decode("account key not string".into()))?;
                    let pk = Pubkey::try_from(s)
                        .map_err(|_| HeliusError::Decode("invalid pubkey".into()))?;
                    account_keys.push(pk);
                }

                let instructions = message
                    .get("instructions")
                    .and_then(Value::as_array)
                    .ok_or_else(|| HeliusError::Decode("missing instructions".into()))?;

                // Outer instructions
                for ix in instructions {
                    if let Some((automate, executor, is_close)) =
                        decode_automate_from_ix(
                            ix,
                            &account_keys,
                            &automation_pda,
                            authority,
                        )?
                    {
                        cache.last_updated_slot = slot;

                        if is_close {
                            cache.active = false;
                            cache.mask = 0;
                            cache.strategy = 0;
                            cache.amount = 0;
                            cache.fee = 0;
                            cache.executor = Pubkey::default();
                            return Ok((None, cache));
                        } else {
                            cache.active = true;
                            cache.mask = u64::from_le_bytes(automate.mask);
                            cache.strategy = automate.strategy as u64;
                            cache.amount = u64::from_le_bytes(automate.amount);
                            cache.fee = u64::from_le_bytes(automate.fee);
                            cache.executor = executor;

                            let strat_enum =
                                AutomationStrategy::from_u64(cache.strategy);
                            let auto = ReconstructedAutomation {
                                amount: cache.amount,
                                authority: cache.authority,
                                executor: cache.executor,
                                fee: cache.fee,
                                strategy: strat_enum,
                                mask: cache.mask,
                            };
                            return Ok((Some(auto), cache));
                        }
                    }
                }

                // Inner instructions
                if let Some(inner_arr) = tx
                    .get("meta")
                    .and_then(|m| m.get("innerInstructions"))
                    .and_then(Value::as_array)
                {
                    for inner in inner_arr {
                        if let Some(ixs) = inner
                            .get("instructions")
                            .and_then(Value::as_array)
                        {
                            for ix in ixs {
                                if let Some((automate, executor, is_close)) =
                                    decode_automate_from_ix(
                                        ix,
                                        &account_keys,
                                        &automation_pda,
                                        authority,
                                    )?
                                {
                                    cache.last_updated_slot = slot;

                                    if is_close {
                                        cache.active = false;
                                        cache.mask = 0;
                                        cache.strategy = 0;
                                        cache.amount = 0;
                                        cache.fee = 0;
                                        cache.executor = Pubkey::default();
                                        return Ok((None, cache));
                                    } else {
                                        cache.active = true;
                                        cache.mask = u64::from_le_bytes(automate.mask);
                                        cache.strategy = automate.strategy as u64;
                                        cache.amount = u64::from_le_bytes(automate.amount);
                                        cache.fee = u64::from_le_bytes(automate.fee);
                                        cache.executor = executor;

                                        let strat_enum =
                                            AutomationStrategy::from_u64(cache.strategy);
                                        let auto = ReconstructedAutomation {
                                            amount: cache.amount,
                                            authority: cache.authority,
                                            executor: cache.executor,
                                            fee: cache.fee,
                                            strategy: strat_enum,
                                            mask: cache.mask,
                                        };
                                        return Ok((Some(auto), cache));
                                    }
                                }
                            }
                        }
                    }
                }
            }

            pagination_token = page.pagination_token;
            if pagination_token.is_none() {
                // No Automate found at all
                cache.active = false;
                cache.last_updated_slot = cutoff_slot;
                return Ok((None, cache));
            }
        }
    }

    /// Parse all ORE Deploy instructions from a page of transactions,
    /// keeping only those whose round account matches `expected_round_pda`.
    ///
    /// Additionally, compute lamport deltas for signer, automation, and round
    /// from the tx's pre/post balances for sanity checking.
    pub fn parse_deployments_from_round_page(
        &self,
        &expected_round_pda: &Pubkey,
        txs: &[Value],
    ) -> Result<Vec<ParsedDeployment>, HeliusError> {
        let mut out = Vec::new();
        let mut skipped_no_deploy = 0;
        let mut skipped_wrong_round = 0;
        let mut deploy_found = 0;

        for tx in txs {
            // Skip failed transactions (we also filter status=succeeded in the RPC call,
            // but this is an extra safety check).
            let err = tx.get("meta").and_then(|m| m.get("err"));
            if !err.map_or(true, |e| e.is_null()) {
                continue;
            }

            let meta = match tx.get("meta") {
                Some(m) => m,
                None => {
                    tracing::warn!("Skipping tx: missing meta");
                    continue;
                }
            };

            // Slot
            let slot = match tx.get("slot").and_then(Value::as_u64) {
                Some(s) => s,
                None => {
                    tracing::warn!("Skipping tx: missing slot");
                    continue;
                }
            };

            // Signature (first one)
            let signature = tx
                .get("transaction")
                .and_then(|t| t.get("signatures"))
                .and_then(Value::as_array)
                .and_then(|sigs| sigs.get(0))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();

            // Message + account keys
            let message = match tx.get("transaction").and_then(|t| t.get("message")) {
                Some(m) => m,
                None => {
                    tracing::warn!("Skipping tx {}: missing message", signature);
                    continue;
                }
            };

            let account_keys_json = match message.get("accountKeys").and_then(Value::as_array) {
                Some(keys) => keys,
                None => {
                    tracing::warn!("Skipping tx {}: missing accountKeys", signature);
                    continue;
                }
            };

            let mut account_keys = Vec::with_capacity(account_keys_json.len());
            let mut skip_tx = false;
            for k in account_keys_json {
                match k.as_str().and_then(|s| Pubkey::try_from(s).ok()) {
                    Some(pk) => account_keys.push(pk),
                    None => {
                        tracing::warn!("Skipping tx {}: invalid account key", signature);
                        skip_tx = true;
                        break;
                    }
                }
            }
            if skip_tx {
                continue;
            }

            // Balances
            let pre_balances_json = match meta.get("preBalances").and_then(Value::as_array) {
                Some(b) => b,
                None => {
                    tracing::warn!("Skipping tx {}: missing preBalances", signature);
                    continue;
                }
            };

            let post_balances_json = match meta.get("postBalances").and_then(Value::as_array) {
                Some(b) => b,
                None => {
                    tracing::warn!("Skipping tx {}: missing postBalances", signature);
                    continue;
                }
            };

            if pre_balances_json.len() != post_balances_json.len()
                || pre_balances_json.len() != account_keys.len()
            {
                // Skip this transaction instead of failing - data may be corrupted or different format
                tracing::warn!(
                    "Skipping tx with balance mismatch: preBalances={}, postBalances={}, accountKeys={}",
                    pre_balances_json.len(), post_balances_json.len(), account_keys.len()
                );
                continue;
            }

            let mut pre_balances: Vec<u64> = Vec::with_capacity(pre_balances_json.len());
            let mut post_balances: Vec<u64> = Vec::with_capacity(post_balances_json.len());
            let mut balance_parse_error = false;

            for v in pre_balances_json {
                match v.as_u64() {
                    Some(n) => pre_balances.push(n),
                    None => {
                        tracing::warn!("Skipping tx {}: preBalance not u64", signature);
                        balance_parse_error = true;
                        break;
                    }
                }
            }
            if balance_parse_error {
                continue;
            }
            
            for v in post_balances_json {
                match v.as_u64() {
                    Some(n) => post_balances.push(n),
                    None => {
                        tracing::warn!("Skipping tx {}: postBalance not u64", signature);
                        balance_parse_error = true;
                        break;
                    }
                }
            }
            if balance_parse_error {
                continue;
            }

            // Helper to get lamport delta for an account index in the message
            let lamport_delta_for_index = |idx: usize| -> i64 {
                if idx >= pre_balances.len() || idx >= post_balances.len() {
                    0
                } else {
                    post_balances[idx] as i64 - pre_balances[idx] as i64
                }
            };

            // OUTER instructions
            let mut tx_has_deploy = false;
            if let Some(ixs) = message
                .get("instructions")
                .and_then(Value::as_array)
            {
                for ix in ixs {
                    if let Some(decoded) = decode_ore_deploy_ix(ix, &account_keys)? {
                        tx_has_deploy = true;
                        deploy_found += 1;
                        if decoded.round_pda != expected_round_pda {
                            skipped_wrong_round += 1;
                            continue;
                        }

                        // Accounts layout for Deploy:
                        // 0: signer
                        // 1: authority
                        // 2: automation_pda
                        // 3: board
                        // 4: miner_pda
                        // 5: round_pda
                        let accounts = ix
                            .get("accounts")
                            .and_then(Value::as_array)
                            .ok_or_else(|| HeliusError::Decode("missing accounts".into()))?;

                        let get_msg_index = |ix_index: usize| -> Result<usize, HeliusError> {
                            let acc_idx = accounts
                                .get(ix_index)
                                .and_then(Value::as_u64)
                                .ok_or_else(|| HeliusError::Decode("bad account index".into()))?
                                as usize;
                            Ok(acc_idx)
                        };

                        let signer_msg_idx = get_msg_index(0)?;
                        let automation_msg_idx = get_msg_index(2)?;
                        let round_msg_idx = get_msg_index(5)?;

                        let signer_lamports_delta =
                            lamport_delta_for_index(signer_msg_idx);
                        let automation_lamports_delta =
                            lamport_delta_for_index(automation_msg_idx);
                        let round_lamports_delta =
                            lamport_delta_for_index(round_msg_idx);

                        out.push(ParsedDeployment {
                            signer: decoded.signer,
                            authority: decoded.authority,
                            miner: decoded.miner_pda,
                            round: decoded.round_pda,
                            amount_per_square: decoded.ix_amount,
                            squares: decoded.ix_squares,
                            slot,
                            signature: signature.clone(),
                            signer_lamports_delta,
                            automation_lamports_delta,
                            round_lamports_delta,
                        });
                    }
                }
            }

            // INNER instructions (if Deploy ever appears there)
            if let Some(inner_arr) = meta
                .get("innerInstructions")
                .and_then(Value::as_array)
            {
                for inner in inner_arr {
                    if let Some(ixs) = inner
                        .get("instructions")
                        .and_then(Value::as_array)
                    {
                        for ix in ixs {
                            if let Some(decoded) =
                                decode_ore_deploy_ix(ix, &account_keys)?
                            {
                                if decoded.round_pda != expected_round_pda {
                                    continue;
                                }

                                let accounts = ix
                                    .get("accounts")
                                    .and_then(Value::as_array)
                                    .ok_or_else(|| HeliusError::Decode("missing accounts".into()))?;

                                let get_msg_index =
                                    |ix_index: usize| -> Result<usize, HeliusError> {
                                        let acc_idx = accounts
                                            .get(ix_index)
                                            .and_then(Value::as_u64)
                                            .ok_or_else(|| {
                                                HeliusError::Decode("bad account index".into())
                                            })? as usize;
                                        Ok(acc_idx)
                                    };

                                let signer_msg_idx = get_msg_index(0)?;
                                let automation_msg_idx = get_msg_index(2)?;
                                let round_msg_idx = get_msg_index(5)?;

                                let signer_lamports_delta =
                                    lamport_delta_for_index(signer_msg_idx);
                                let automation_lamports_delta =
                                    lamport_delta_for_index(automation_msg_idx);
                                let round_lamports_delta =
                                    lamport_delta_for_index(round_msg_idx);

                                out.push(ParsedDeployment {
                                    signer: decoded.signer,
                                    authority: decoded.authority,
                                    miner: decoded.miner_pda,
                                    round: decoded.round_pda,
                                    amount_per_square: decoded.ix_amount,
                                    squares: decoded.ix_squares,
                                    slot,
                                    signature: signature.clone(),
                                    signer_lamports_delta,
                                    automation_lamports_delta,
                                    round_lamports_delta,
                                });
                            }
                        }
                    }
                }
            }
            
            if !tx_has_deploy {
                skipped_no_deploy += 1;
            }
        }

        tracing::info!(
            "parse_deployments_from_round_page: txs={}, deploys_found={}, wrong_round={}, no_deploy={}, matched={}",
            txs.len(), deploy_found, skipped_wrong_round, skipped_no_deploy, out.len()
        );

        Ok(out)
    }

    /// Scan a page of transactions looking for Ore `Log` instructions that
    /// contain a `ResetEvent` for `round_id`.
    ///
    /// Returns the *latest* ResetEvent in this page (by slot), if any.
    pub fn parse_reset_event_from_round_page(
        &self,
        round_id: u64,
        txs: &[Value],
    ) -> Result<Option<(ResetEvent, u64)>, HeliusError> {
        let mut best: Option<(ResetEvent, u64)> = None;
        let event_size = mem::size_of::<ResetEvent>();

        for tx in txs {
            // Only consider succeeded txs (extra safety)
            let err = tx.get("meta").and_then(|m| m.get("err"));
            if !err.map_or(true, |e| e.is_null()) {
                continue;
            }

            let slot = match tx.get("slot").and_then(Value::as_u64) {
                Some(s) => s,
                None => continue,
            };

            let message = match tx
                .get("transaction")
                .and_then(|t| t.get("message"))
            {
                Some(m) => m,
                None => continue,
            };

            let account_keys_json = match message
                .get("accountKeys")
                .and_then(Value::as_array)
            {
                Some(a) => a,
                None => continue,
            };

            let mut account_keys = Vec::with_capacity(account_keys_json.len());
            for k in account_keys_json {
                let s = match k.as_str() {
                    Some(s) => s,
                    None => continue,
                };
                let pk = match Pubkey::try_from(s) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };
                account_keys.push(pk);
            }

            // Closure: try to parse a ResetEvent from a single ix
            let mut scan_ix = |ix: &Value| -> Result<Option<ResetEvent>, HeliusError> {
                let program_id_index = ix
                    .get("programIdIndex")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| HeliusError::Decode("missing programIdIndex".into()))?
                    as usize;

                let program_id = account_keys
                    .get(program_id_index)
                    .ok_or_else(|| HeliusError::Decode("programIdIndex out of range".into()))?;

                if *program_id != ore_api::PROGRAM_ID {
                    return Ok(None);
                }

                let data_str = ix
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| HeliusError::Decode("missing data".into()))?;

                let data = match bs58::decode(data_str).into_vec() {
                    Ok(d) => d,
                    Err(_) => return Ok(None),
                };

                if data.is_empty() {
                    return Ok(None);
                }

                // First byte: OreInstruction discriminator
                let tag = data[0];
                let ore_tag = match OreInstruction::try_from(tag) {
                    Ok(t) => t,
                    Err(_) => return Ok(None),
                };
                if ore_tag != OreInstruction::Log {
                    return Ok(None);
                }

                // Remaining bytes: ResetEvent payload
                if data.len() < 1 + event_size {
                    return Ok(None);
                }

                let payload = &data[1..1 + event_size];
                let ev: ResetEvent = bytemuck::pod_read_unaligned(payload);


                // Basic sanity: disc = 0, round_id matches the round we’re reconstructing
                if ev.disc != 0 {
                    return Ok(None);
                }
                if ev.round_id != round_id {
                    return Ok(None);
                }

                Ok(Some(ev))
            };

            // OUTER instructions
            if let Some(ixs) = message
                .get("instructions")
                .and_then(Value::as_array)
            {
                for ix in ixs {
                    if let Some(ev) = scan_ix(ix)? {
                        // Keep the latest event in this page
                        match best {
                            None => best = Some((ev, slot)),
                            Some((_, best_slot)) if slot > best_slot => {
                                best = Some((ev, slot));
                            }
                            _ => {}
                        }
                    }
                }
            }

            // INNER instructions: program_log is invoked via CPI, so the Log ix
            // will typically show up here.
            if let Some(inner_arr) = tx
                .get("meta")
                .and_then(|m| m.get("innerInstructions"))
                .and_then(Value::as_array)
            {
                for inner in inner_arr {
                    if let Some(ixs) = inner
                        .get("instructions")
                        .and_then(Value::as_array)
                    {
                        for ix in ixs {
                            if let Some(ev) = scan_ix(ix)? {
                                match best {
                                    None => best = Some((ev, slot)),
                                    Some((_, best_slot)) if slot > best_slot => {
                                        best = Some((ev, slot));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(best)
    }

    /// Parse all DeployEvents from a page of transactions.
    /// Returns a list of (DeployEvent, slot, signature, instruction_index).
    /// 
    /// DeployEvents are emitted after each Deploy instruction and contain
    /// all the information needed for reconstruction without automation states.
    pub fn parse_deploy_events_from_page(
        &self,
        txs: &[Value],
    ) -> Result<Vec<ParsedDeployEvent>, HeliusError> {
        let mut events = Vec::new();
        let event_size = mem::size_of::<DeployEvent>();

        for tx in txs {
            // Only consider succeeded txs
            let err = tx.get("meta").and_then(|m| m.get("err"));
            if !err.map_or(true, |e| e.is_null()) {
                continue;
            }

            let slot = match tx.get("slot").and_then(Value::as_u64) {
                Some(s) => s,
                None => continue,
            };

            let signature = tx
                .get("transaction")
                .and_then(|t| t.get("signatures"))
                .and_then(Value::as_array)
                .and_then(|arr| arr.first())
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();

            let message = match tx.get("transaction").and_then(|t| t.get("message")) {
                Some(m) => m,
                None => continue,
            };

            let account_keys_arr = match message.get("accountKeys").and_then(Value::as_array) {
                Some(a) => a,
                None => continue,
            };

            let mut account_keys = Vec::with_capacity(account_keys_arr.len());
            for k in account_keys_arr {
                let s = match k.as_str() {
                    Some(s) => s,
                    None => continue,
                };
                let pk = match Pubkey::try_from(s) {
                    Ok(pk) => pk,
                    Err(_) => continue,
                };
                account_keys.push(pk);
            }

            // Track instruction index for matching with Deploy instructions
            let mut outer_ix_index = 0u8;

            // Closure to parse a DeployEvent from a single ix
            let scan_ix = |ix: &Value, ix_index: u8| -> Result<Option<DeployEvent>, HeliusError> {
                let program_id_index = ix
                    .get("programIdIndex")
                    .and_then(Value::as_u64)
                    .ok_or_else(|| HeliusError::Decode("missing programIdIndex".into()))?
                    as usize;

                let program_id = account_keys
                    .get(program_id_index)
                    .ok_or_else(|| HeliusError::Decode("programIdIndex out of range".into()))?;

                // Must be ORE program
                if *program_id != ore_api::PROGRAM_ID {
                    return Ok(None);
                }

                let data_str = ix
                    .get("data")
                    .and_then(Value::as_str)
                    .ok_or_else(|| HeliusError::Decode("missing data".into()))?;

                let data = match bs58::decode(data_str).into_vec() {
                    Ok(d) => d,
                    Err(_) => return Ok(None),
                };

                if data.is_empty() {
                    return Ok(None);
                }

                // Log instruction discriminator is 1
                if data[0] != 1 {
                    return Ok(None);
                }

                // Check we have enough bytes for the event
                if data.len() < 1 + event_size {
                    return Ok(None);
                }

                let payload = &data[1..1 + event_size];
                let ev: DeployEvent = bytemuck::pod_read_unaligned(payload);

                // DeployEvent discriminator is 1 (ResetEvent is 0)
                if ev.disc != 1 {
                    return Ok(None);
                }

                Ok(Some(ev))
            };

            // OUTER instructions
            if let Some(ixs) = message.get("instructions").and_then(Value::as_array) {
                for ix in ixs {
                    if let Ok(Some(ev)) = scan_ix(ix, outer_ix_index) {
                        events.push(ParsedDeployEvent {
                            event: ev,
                            slot,
                            signature: signature.clone(),
                            instruction_index: outer_ix_index,
                        });
                    }
                    outer_ix_index += 1;
                }
            }

            // INNER instructions (DeployEvent is typically emitted via CPI)
            if let Some(inner_arr) = tx
                .get("meta")
                .and_then(|m| m.get("innerInstructions"))
                .and_then(Value::as_array)
            {
                for inner in inner_arr {
                    let parent_ix = inner
                        .get("index")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u8;
                    
                    if let Some(ixs) = inner.get("instructions").and_then(Value::as_array) {
                        for (inner_idx, ix) in ixs.iter().enumerate() {
                            if let Ok(Some(ev)) = scan_ix(ix, inner_idx as u8) {
                                events.push(ParsedDeployEvent {
                                    event: ev,
                                    slot,
                                    signature: signature.clone(),
                                    instruction_index: parent_ix,
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    /// Check if all Deploy instructions in the transactions have corresponding DeployEvents.
    /// Returns (has_all_events, deploy_count, event_count, deploys_without_events).
    pub fn check_deploy_events_coverage(
        &self,
        round_id: u64,
        txs: &[Value],
    ) -> Result<DeployEventCoverage, HeliusError> {
        // First, parse all DeployEvents
        let events = self.parse_deploy_events_from_page(txs)?;
        
        // Filter to only events for this round
        let round_events: Vec<_> = events
            .iter()
            .filter(|e| e.event.round_id == round_id)
            .collect();

        // Parse all Deploy instructions
        let deployments = self.parse_deployments_from_round_page(
            &ore_api::round_pda(round_id).0,
            txs,
        )?;

        // Build a set of (signature, authority) for events
        let event_keys: std::collections::HashSet<(String, String)> = round_events
            .iter()
            .map(|e| (e.signature.clone(), e.event.authority.to_string()))
            .collect();

        // Check which deployments have events
        let mut deploys_without_events = Vec::new();
        for deploy in &deployments {
            let key = (deploy.signature.clone(), deploy.authority.to_string());
            if !event_keys.contains(&key) {
                deploys_without_events.push(DeployWithoutEvent {
                    signature: deploy.signature.clone(),
                    authority: deploy.authority.to_string(),
                    slot: deploy.slot,
                });
            }
        }

        Ok(DeployEventCoverage {
            has_all_events: deploys_without_events.is_empty() && !deployments.is_empty(),
            deploy_count: deployments.len(),
            event_count: round_events.len(),
            deploys_without_events,
        })
    }

    /// Helius v2 getProgramAccounts with cursor-based pagination and filters.
    ///
    /// Benefits over standard getProgramAccounts:
    /// - Configurable limits: 1-10,000 accounts per request
    /// - Cursor-based pagination: prevents timeouts on large queries
    /// - changedSinceSlot: incremental updates for real-time sync
    ///
    /// Rate limit: 25 RPS (Developer plan)
    pub async fn get_program_accounts_v2(
        &mut self,
        program_id: &Pubkey,
        options: GetProgramAccountsV2Options,
    ) -> Result<GetProgramAccountsV2Page, HeliusError> {
        // Enforce minimum rate limiting (40ms between calls = 25 RPS max)
        if self.last_request_at.elapsed().as_millis() < 40 {
            tokio::time::sleep(Duration::from_millis(
                (40 - self.last_request_at.elapsed().as_millis()) as u64,
            ))
            .await;
        }
        self.last_request_at = Instant::now();

        // Build the options object for Helius v2
        let mut opts = serde_json::Map::new();

        // Encoding (default to base64)
        opts.insert(
            "encoding".to_string(),
            json!(options.encoding.as_deref().unwrap_or("base64")),
        );

        // Limit (1-10000)
        if let Some(limit) = options.limit {
            opts.insert("limit".to_string(), json!(limit.min(10000).max(1)));
        }

        // Cursor for pagination
        if let Some(cursor) = &options.cursor {
            opts.insert("cursor".to_string(), json!(cursor));
        }

        // changedSinceSlot for incremental updates
        if let Some(slot) = options.changed_since_slot {
            opts.insert("changedSinceSlot".to_string(), json!(slot));
        }

        // Filters (memcmp and/or dataSize)
        if !options.filters.is_empty() {
            opts.insert("filters".to_string(), json!(options.filters));
        }

        // Data slice for partial account data
        if let Some(slice) = &options.data_slice {
            opts.insert("dataSlice".to_string(), json!(slice));
        }

        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getProgramAccountsV2",
            "params": [
                program_id.to_string(),
                opts
            ]
        });

        tracing::debug!("getProgramAccountsV2 request to {}: {}", self.rpc_url, body);

        let start = Instant::now();
        let request_size = body.to_string().len() as u32;
        
        let response = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        
        let response_bytes = response.bytes().await?;
        let duration_ms = start.elapsed().as_millis() as u32;
        let response_size = response_bytes.len() as u32;

        let resp: RpcResponse<GetProgramAccountsV2Result> = serde_json::from_slice(&response_bytes)
            .map_err(|e| HeliusError::InvalidResponse(format!("JSON parse error: {}", e)))?;

        if let Some(err) = resp.error {
            self.log_error("getProgramAccountsV2", "program", &program_id.to_string(), duration_ms, &err.message);
            return Err(HeliusError::InvalidResponse(format!(
                "code: {}, message: {}, data: {:?}",
                err.code, err.message, err.data
            )));
        }

        let result = resp.result.ok_or_else(|| {
            self.log_error("getProgramAccountsV2", "program", &program_id.to_string(), duration_ms, "null result");
            HeliusError::InvalidResponse("result is null with no error".to_string())
        })?;

        let accounts_count = result.accounts.len() as u32;
        let cursor_str = options.cursor.as_deref().unwrap_or("");
        
        // Build filters JSON for logging
        let filters_json = if options.filters.is_empty() {
            String::new()
        } else {
            serde_json::to_string(&options.filters).unwrap_or_default()
        };
        
        self.log_filtered_success("getProgramAccountsV2", "program", &program_id.to_string(), 0, cursor_str, &filters_json, duration_ms, accounts_count, response_size);
        
        Ok(GetProgramAccountsV2Page {
            accounts: result.accounts,
            cursor: result.pagination_key,
        })
    }

    /// Fetch all ORE miner accounts using getProgramAccountsV2.
    /// Automatically paginates through all results.
    pub async fn get_all_ore_miners(
        &mut self,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self
                .get_program_accounts_v2(
                    &ore_api::PROGRAM_ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: None,
                        filters: vec![
                            // Filter by Miner account size (discriminator + data)
                            ProgramAccountFilter::DataSize(std::mem::size_of::<ore_api::Miner>() as u64 + 8),
                        ],
                        data_slice: None,
                    },
                )
                .await?;

            all_accounts.extend(page.accounts);

            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_accounts)
    }

    /// Fetch ORE miner accounts that changed since a given slot.
    /// Used for incremental cache updates.
    pub async fn get_ore_miners_changed_since(
        &mut self,
        since_slot: u64,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;

        loop {
            let page = self
                .get_program_accounts_v2(
                    &ore_api::PROGRAM_ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: Some(since_slot),
                        filters: vec![ProgramAccountFilter::DataSize(
                            std::mem::size_of::<ore_api::Miner>() as u64 + 8,
                        )],
                        data_slice: None,
                    },
                )
                .await?;

            all_accounts.extend(page.accounts);

            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_accounts)
    }

    /// Fetch all ORE token holder accounts using getProgramAccountsV2.
    /// Filters Token Program accounts by ORE mint address.
    ///
    /// Returns token accounts with owner and balance info.
    pub async fn get_all_ore_token_holders(
        &mut self,
        ore_mint: &Pubkey,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;

        // Token account layout: mint is at offset 0 (32 bytes)
        let mint_bytes = ore_mint.to_bytes();
        let mint_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &mint_bytes,
        );

        loop {
            let page = self
                .get_program_accounts_v2(
                    &spl_token::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: None,
                        filters: vec![
                            // Token account size: 165 bytes
                            ProgramAccountFilter::DataSize(165),
                            // Mint address at offset 0
                            ProgramAccountFilter::Memcmp(MemcmpFilter {
                                offset: 0,
                                bytes: mint_b64.clone(),
                                encoding: Some("base64".to_string()),
                            }),
                        ],
                        data_slice: None,
                    },
                )
                .await?;

            all_accounts.extend(page.accounts);

            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_accounts)
    }

    /// Fetch ORE token accounts that changed since a given slot.
    /// Used for incremental cache updates.
    pub async fn get_ore_token_holders_changed_since(
        &mut self,
        ore_mint: &Pubkey,
        since_slot: u64,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;

        let mint_bytes = ore_mint.to_bytes();
        let mint_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &mint_bytes,
        );

        loop {
            let page = self
                .get_program_accounts_v2(
                    &spl_token::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: Some(since_slot),
                        filters: vec![
                            ProgramAccountFilter::DataSize(165),
                            ProgramAccountFilter::Memcmp(MemcmpFilter {
                                offset: 0,
                                bytes: mint_b64.clone(),
                                encoding: Some("base64".to_string()),
                            }),
                        ],
                        data_slice: None,
                    },
                )
                .await?;

            all_accounts.extend(page.accounts);

            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_accounts)
    }

    /// Fetch all ORE token holders with optimized dataSlice.
    /// Only fetches owner (32 bytes) + amount (8 bytes) = 40 bytes per account.
    /// Token account layout: [mint:32][owner:32][amount:8][...]
    /// With dataSlice offset=32, length=40, we get owner+amount only.
    pub async fn get_ore_token_balances(
        &mut self,
        ore_mint: &Pubkey,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<TokenBalance>, HeliusError> {
        let mut all_balances = Vec::new();
        let mut cursor: Option<String> = None;
        let mut page_num = 0u32;

        let mint_bytes = ore_mint.to_bytes();
        let mint_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &mint_bytes,
        );

        loop {
            page_num += 1;
            let page = self
                .get_program_accounts_v2(
                    &spl_token::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: None,
                        filters: vec![
                            ProgramAccountFilter::DataSize(165),
                            ProgramAccountFilter::Memcmp(MemcmpFilter {
                                offset: 0,
                                bytes: mint_b64.clone(),
                                encoding: Some("base64".to_string()),
                            }),
                        ],
                        data_slice: None, // Helius v2 doesn't support dataSlice
                    },
                )
                .await?;

            let page_count = page.accounts.len();
            
            // Parse full token account data
            for acc in &page.accounts {
                if let Some(balance) = Self::parse_token_balance_from_full_account(acc) {
                    all_balances.push(balance);
                }
            }

            tracing::debug!(
                "ORE token holders page {}: {} accounts fetched, {} total so far, cursor: {}",
                page_num,
                page_count,
                all_balances.len(),
                page.cursor.is_some()
            );

            // Continue until cursor is null (end of pagination)
            if page.cursor.is_none() {
                tracing::info!("ORE token holders pagination complete: {} pages, {} holders", page_num, all_balances.len());
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_balances)
    }

    /// Fetch ORE token balances that changed since a given slot.
    /// Uses dataSlice for efficiency.
    pub async fn get_ore_token_balances_changed_since(
        &mut self,
        ore_mint: &Pubkey,
        since_slot: u64,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<TokenBalance>, HeliusError> {
        let mut all_balances = Vec::new();
        let mut cursor: Option<String> = None;

        let mint_bytes = ore_mint.to_bytes();
        let mint_b64 = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            &mint_bytes,
        );

        loop {
            let page = self
                .get_program_accounts_v2(
                    &spl_token::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(5000)),
                        cursor: cursor.clone(),
                        changed_since_slot: Some(since_slot),
                        filters: vec![
                            ProgramAccountFilter::DataSize(165),
                            ProgramAccountFilter::Memcmp(MemcmpFilter {
                                offset: 0,
                                bytes: mint_b64.clone(),
                                encoding: Some("base64".to_string()),
                            }),
                        ],
                        data_slice: None, // Helius v2 doesn't support dataSlice
                    },
                )
                .await?;

            for acc in &page.accounts {
                if let Some(balance) = Self::parse_token_balance_from_full_account(acc) {
                    all_balances.push(balance);
                }
            }

            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }

        Ok(all_balances)
    }

    /// Parse owner + amount from full token account data (165 bytes)
    /// Token account layout:
    /// - [0..32]: mint pubkey
    /// - [32..64]: owner pubkey  
    /// - [64..72]: amount (u64 little-endian)
    /// - [72..]: state, delegate, etc.
    fn parse_token_balance_from_full_account(acc: &ProgramAccountV2) -> Option<TokenBalance> {
        // Data format: [base64_data, "base64"]
        let data_b64 = acc.account.data.first()?;
        let data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            data_b64,
        ).ok()?;

        if data.len() < 72 {
            return None;
        }

        // Bytes 32-64: owner pubkey
        let owner_bytes: [u8; 32] = data[32..64].try_into().ok()?;
        let owner = Pubkey::from(owner_bytes);

        // Bytes 64-72: amount (little-endian u64)
        let amount_bytes: [u8; 8] = data[64..72].try_into().ok()?;
        let amount = u64::from_le_bytes(amount_bytes);

        // Token account pubkey
        let token_account = Pubkey::try_from(acc.pubkey.as_str()).ok()?;

        Some(TokenBalance {
            token_account,
            owner,
            amount,
        })
    }
    
    // ========================================================================
    // EVORE Account Fetching (Phase 1b)
    // ========================================================================
    
    /// Fetch all EVORE Manager accounts using getProgramAccountsV2.
    pub async fn get_all_evore_managers(
        &mut self,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        self.get_evore_managers_since(None, limit_per_page).await
    }
    
    /// Fetch EVORE Manager accounts changed since a slot (incremental updates).
    pub async fn get_evore_managers_changed_since(
        &mut self,
        since_slot: u64,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        self.get_evore_managers_since(Some(since_slot), limit_per_page).await
    }
    
    /// Internal: Fetch EVORE managers with optional changedSinceSlot.
    async fn get_evore_managers_since(
        &mut self,
        since_slot: Option<u64>,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;
        
        // Manager account size: discriminator (8) + Manager struct size
        let manager_size = std::mem::size_of::<evore::state::Manager>() as u64 + 8;
        
        loop {
            let page = self
                .get_program_accounts_v2(
                    &evore::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(1000)),
                        cursor: cursor.clone(),
                        changed_since_slot: since_slot,
                        filters: vec![
                            ProgramAccountFilter::DataSize(manager_size),
                        ],
                        data_slice: None,
                    },
                )
                .await?;
            
            all_accounts.extend(page.accounts);
            
            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }
        
        if since_slot.is_some() {
            tracing::debug!("Fetched {} EVORE managers changed since slot {}", all_accounts.len(), since_slot.unwrap());
        } else {
            tracing::info!("Fetched {} EVORE manager accounts (full)", all_accounts.len());
        }
        Ok(all_accounts)
    }
    
    /// Fetch all EVORE Deployer accounts using getProgramAccountsV2.
    pub async fn get_all_evore_deployers(
        &mut self,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        self.get_evore_deployers_since(None, limit_per_page).await
    }
    
    /// Fetch EVORE Deployer accounts changed since a slot (incremental updates).
    pub async fn get_evore_deployers_changed_since(
        &mut self,
        since_slot: u64,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        self.get_evore_deployers_since(Some(since_slot), limit_per_page).await
    }
    
    /// Internal: Fetch EVORE deployers with optional changedSinceSlot.
    async fn get_evore_deployers_since(
        &mut self,
        since_slot: Option<u64>,
        limit_per_page: Option<u32>,
    ) -> Result<Vec<ProgramAccountV2>, HeliusError> {
        let mut all_accounts = Vec::new();
        let mut cursor: Option<String> = None;
        
        // Deployer account size: discriminator (8) + Deployer struct size
        let deployer_size = std::mem::size_of::<evore::state::Deployer>() as u64 + 8;
        
        loop {
            let page = self
                .get_program_accounts_v2(
                    &evore::ID,
                    GetProgramAccountsV2Options {
                        encoding: Some("base64".to_string()),
                        limit: Some(limit_per_page.unwrap_or(1000)),
                        cursor: cursor.clone(),
                        changed_since_slot: since_slot,
                        filters: vec![
                            ProgramAccountFilter::DataSize(deployer_size),
                        ],
                        data_slice: None,
                    },
                )
                .await?;
            
            all_accounts.extend(page.accounts);
            
            if page.cursor.is_none() {
                break;
            }
            cursor = page.cursor;
        }
        
        if since_slot.is_some() {
            tracing::debug!("Fetched {} EVORE deployers changed since slot {}", all_accounts.len(), since_slot.unwrap());
        } else {
            tracing::info!("Fetched {} EVORE deployer accounts (full)", all_accounts.len());
        }
        Ok(all_accounts)
    }
    
    /// Fetch a single account's lamport balance (for ManagedMinerAuth PDAs)
    pub async fn get_account_balance(&mut self, address: &Pubkey) -> Result<Option<u64>, HeliusError> {
        // Use getAccountInfo to get just the lamports
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getAccountInfo",
            "params": [
                address.to_string(),
                { "encoding": "base64" }
            ]
        });
        
        let start = Instant::now();
        
        let response = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        
        let response_bytes = response.bytes().await?;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        let resp: serde_json::Value = serde_json::from_slice(&response_bytes)
            .map_err(|e| HeliusError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        if let Some(err) = resp.get("error") {
            self.log_error("getAccountInfo", "balance", &address.to_string(), duration_ms, &err.to_string());
            return Err(HeliusError::Rpc(err.to_string()));
        }
        
        let result = resp.get("result").and_then(|r| r.get("value"));
        
        if result.is_none() || result == Some(&serde_json::Value::Null) {
            // Account doesn't exist
            self.log_success("getAccountInfo", "balance", &address.to_string(), duration_ms, 0, response_bytes.len() as u32);
            return Ok(None);
        }
        
        let lamports = result
            .and_then(|v| v.get("lamports"))
            .and_then(|l| l.as_u64());
        
        self.log_success("getAccountInfo", "balance", &address.to_string(), duration_ms, 1, response_bytes.len() as u32);
        
        Ok(lamports)
    }
    
    /// Fetch multiple account balances in a batch
    pub async fn get_multiple_account_balances(&mut self, addresses: &[Pubkey]) -> Result<Vec<(Pubkey, Option<u64>)>, HeliusError> {
        if addresses.is_empty() {
            return Ok(Vec::new());
        }
        
        // Use getMultipleAccounts for efficiency
        let address_strings: Vec<String> = addresses.iter().map(|a| a.to_string()).collect();
        
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getMultipleAccounts",
            "params": [
                address_strings,
                { "encoding": "base64" }
            ]
        });
        
        let start = Instant::now();
        
        let response = self.client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?;
        
        let response_bytes = response.bytes().await?;
        let duration_ms = start.elapsed().as_millis() as u32;
        
        let resp: serde_json::Value = serde_json::from_slice(&response_bytes)
            .map_err(|e| HeliusError::InvalidResponse(format!("JSON parse error: {}", e)))?;
        
        if let Some(err) = resp.get("error") {
            self.log_error("getMultipleAccounts", "balance", "", duration_ms, &err.to_string());
            return Err(HeliusError::Rpc(err.to_string()));
        }
        
        let values = resp
            .get("result")
            .and_then(|r| r.get("value"))
            .and_then(|v| v.as_array())
            .ok_or_else(|| HeliusError::InvalidResponse("Missing result.value array".to_string()))?;
        
        let mut results = Vec::with_capacity(addresses.len());
        
        for (i, value) in values.iter().enumerate() {
            let lamports = if value.is_null() {
                None
            } else {
                value.get("lamports").and_then(|l| l.as_u64())
            };
            results.push((addresses[i], lamports));
        }
        
        self.log_success("getMultipleAccounts", "balance", "", duration_ms, results.len() as u32, response_bytes.len() as u32);
        
        Ok(results)
    }
}

/// Parsed token balance from sliced account data
#[derive(Debug, Clone)]
pub struct TokenBalance {
    /// The token account address
    pub token_account: Pubkey,
    /// The owner of this token account
    pub owner: Pubkey,
    /// The token amount (raw, not UI amount)
    pub amount: u64,
}

// ============================================================================
// Helius v2 Types
// ============================================================================

/// A parsed DeployEvent with context
#[derive(Debug, Clone, Serialize)]
pub struct ParsedDeployEvent {
    pub event: DeployEvent,
    pub slot: u64,
    pub signature: String,
    pub instruction_index: u8,
}

/// Result of checking DeployEvent coverage for a round
#[derive(Debug, Clone, Serialize)]
pub struct DeployEventCoverage {
    /// True if all Deploy instructions have corresponding DeployEvents
    pub has_all_events: bool,
    /// Number of Deploy instructions found
    pub deploy_count: usize,
    /// Number of DeployEvents found for this round
    pub event_count: usize,
    /// List of deployments that don't have events (need automation state lookup)
    pub deploys_without_events: Vec<DeployWithoutEvent>,
}

/// A deployment that doesn't have a corresponding DeployEvent
#[derive(Debug, Clone, Serialize)]
pub struct DeployWithoutEvent {
    pub signature: String,
    pub authority: String,
    pub slot: u64,
}

/// Options for getProgramAccountsV2
#[derive(Debug, Clone, Default)]
pub struct GetProgramAccountsV2Options {
    /// Encoding for account data: "base64", "base58", "jsonParsed"
    pub encoding: Option<String>,
    /// Number of accounts per page (1-10000)
    pub limit: Option<u32>,
    /// Cursor for pagination (from previous response)
    pub cursor: Option<String>,
    /// Only return accounts that changed since this slot
    pub changed_since_slot: Option<u64>,
    /// Filters to apply (memcmp, dataSize)
    pub filters: Vec<ProgramAccountFilter>,
    /// Data slice to fetch only part of account data
    pub data_slice: Option<DataSlice>,
}

/// Data slice for fetching partial account data
#[derive(Debug, Clone, Serialize)]
pub struct DataSlice {
    pub offset: u64,
    pub length: u64,
}

/// Filter types for getProgramAccountsV2
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ProgramAccountFilter {
    /// Filter by exact data size
    DataSize(u64),
    /// Filter by memory comparison at offset
    Memcmp(MemcmpFilter),
}

/// Memcmp filter details
#[derive(Debug, Clone, Serialize)]
pub struct MemcmpFilter {
    pub offset: u64,
    pub bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

/// A single account from getProgramAccountsV2 response
#[derive(Debug, Clone, Deserialize)]
pub struct ProgramAccountV2 {
    /// The account's public key
    pub pubkey: String,
    /// The account data
    pub account: AccountDataV2,
}

/// Account data from v2 response
#[derive(Debug, Clone, Deserialize)]
pub struct AccountDataV2 {
    /// Account data (format depends on encoding)
    pub data: Vec<String>, // [data, encoding] for base64
    /// Account owner program
    pub owner: String,
    /// Lamports balance
    pub lamports: u64,
    /// Is executable
    pub executable: bool,
    /// Rent epoch
    #[serde(rename = "rentEpoch")]
    pub rent_epoch: u64,
}

/// Page result from getProgramAccountsV2
#[derive(Debug)]
pub struct GetProgramAccountsV2Page {
    pub accounts: Vec<ProgramAccountV2>,
    pub cursor: Option<String>,
}

/// Internal response structure for v2
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GetProgramAccountsV2Result {
    #[serde(default)]
    accounts: Vec<ProgramAccountV2>,
    #[serde(default)]
    pagination_key: Option<String>,
    #[serde(default)]
    total_results: Option<u64>,
}

/// The page we return to the rest of the app.
#[derive(Debug)]
pub struct RoundTransactionsPage {
    pub transactions: Vec<Value>,        // full transactions as raw JSON for now
    pub pagination_token: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddressTransactionsPage {
    pub transactions: Vec<serde_json::Value>,
    pub pagination_token: Option<String>,
}

/// RPC error from JSON-RPC response
#[derive(Debug, Deserialize)]
struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<Value>,
}

/// JSON-RPC envelope
#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default)]
    pub result: Option<T>,
    #[serde(default)]
    pub error: Option<RpcError>,
}

/// Shape of the `result` for getTransactionsForAddress.
/// We keep it minimal for now: just `data` + `paginationToken`.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct GetTransactionsResult {
    pub data: Vec<Value>,                     // each entry contains the full transaction object
    #[serde(default)]
    pub pagination_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GetTransactionsOptions {
    #[serde(rename = "transactionDetails")]
    pub transaction_details: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,

    #[serde(rename = "sortOrder", skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<String>,

    pub limit: u32,

    #[serde(rename = "paginationToken", skip_serializing_if = "Option::is_none")]
    pub pagination_token: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub commitment: Option<String>,

    /// Helius advanced filters (slot / blockTime / status / signature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filters: Option<Value>,
}

pub struct ParsedDeployment {
    pub signer: Pubkey,
    pub authority: Pubkey,
    pub miner: Pubkey,
    pub round: Pubkey,
    pub amount_per_square: u64,
    pub squares: [bool; 25],
    pub slot: u64,
    pub signature: String,

    pub signer_lamports_delta: i64,
    pub automation_lamports_delta: i64,
    pub round_lamports_delta: i64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct ResetEvent {
    /// The event discriminator.
    pub disc: u64,

    /// The block that was opened for trading.
    pub round_id: u64,

    /// The start slot of the next block.
    pub start_slot: u64,

    /// The end slot of the next block.
    pub end_slot: u64,

    /// The winning square of the round.
    pub winning_square: u64,

    /// The top miner of the round.
    pub top_miner: Pubkey,

    /// The number of miners on the winning square.
    pub num_winners: u64,

    /// The amount of ORE payout for the motherlode.
    pub motherlode: u64,

    /// The total amount of SOL prospected in the round.
    pub total_deployed: u64,

    /// The total amount of SOL put in the ORE vault.
    pub total_vaulted: u64,

    /// The total amount of SOL won by miners for the round.
    pub total_winnings: u64,

    /// The total amount of ORE minted for the round.
    pub total_minted: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

/// Deploy event emitted by ORE program after each Deploy instruction.
/// This was added to enable easier historical reconstruction.
/// If present, we can skip automation state fetching entirely.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable, Serialize, Deserialize)]
pub struct DeployEvent {
    /// The event discriminator (1 for DeployEvent).
    pub disc: u64,

    /// The authority of the deployer.
    pub authority: Pubkey,

    /// The amount of SOL deployed per square.
    pub amount: u64,

    /// The mask of the squares deployed to.
    pub mask: u64,

    /// The round id.
    pub round_id: u64,

    /// The signer of the deployer.
    pub signer: Pubkey,

    /// The strategy used by the autominer (u64::MAX if manual).
    pub strategy: u64,

    /// The total number of squares deployed to.
    pub total_squares: u64,

    /// The timestamp of the event.
    pub ts: i64,
}

#[derive(Debug, Clone)]
struct DecodedDeployIx {
    pub authority: Pubkey,
    pub miner_pda: Pubkey,
    pub signer: Pubkey,
    pub round_pda: Pubkey,
    pub ix_amount: u64,
    pub ix_squares: [bool; 25],
}

/// Decode a single ORE Deploy instruction into a structured form.
/// Does NOT handle automation; that’s done at the app layer.
// ============================================================================
// Helper Functions
// ============================================================================

/// Extract provider name from RPC URL for metrics
fn extract_provider_name(url: &str) -> String {
    if url.contains("helius") {
        "helius".to_string()
    } else if url.contains("quicknode") {
        "quicknode".to_string()
    } else if url.contains("alchemy") {
        "alchemy".to_string()
    } else if url.contains("triton") {
        "triton".to_string()
    } else if url.contains("rpcpool") {
        "rpcpool".to_string()
    } else if url.contains("localhost") || url.contains("127.0.0.1") {
        "localhost".to_string()
    } else {
        "unknown".to_string()
    }
}

/// Extract API key ID from URL (e.g., for Helius ?api-key=xxx)
fn extract_api_key_id(url: &str) -> String {
    // Look for api-key or api_key parameter
    if let Some(idx) = url.find("api-key=").or_else(|| url.find("api_key=")) {
        let start = idx + 8;
        let end = url[start..].find('&').map(|i| start + i).unwrap_or(url.len());
        let key = &url[start..end];
        // Return first 8 chars as ID (don't log full key)
        if key.len() >= 8 {
            format!("{}...", &key[..8])
        } else {
            key.to_string()
        }
    } else {
        String::new()
    }
}

fn decode_ore_deploy_ix(
    ix: &Value,
    account_keys: &[Pubkey],
) -> Result<Option<DecodedDeployIx>, HeliusError> {
    // programIdIndex → ore program check
    let program_id_index = ix
        .get("programIdIndex")
        .and_then(Value::as_u64)
        .ok_or_else(|| HeliusError::Decode("missing programIdIndex".into()))? as usize;

    let program_id = account_keys
        .get(program_id_index)
        .ok_or_else(|| HeliusError::Decode("programIdIndex out of range".into()))?;

    if *program_id != ore_api::PROGRAM_ID {
        return Ok(None);
    }

    // Decode base58 data
    let data_str = ix
        .get("data")
        .and_then(Value::as_str)
        .ok_or_else(|| HeliusError::Decode("missing data".into()))?;

    let data = match bs58::decode(data_str).into_vec() {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };

    if data.is_empty() {
        return Ok(None);
    }

    // Tag must be Deploy
    let tag = data[0];
    let ore_tag = match OreInstruction::try_from(tag) {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    if ore_tag != OreInstruction::Deploy {
        return Ok(None);
    }

    // Decode Deploy body
    const DEPLOY_BODY_SIZE: usize = core::mem::size_of::<Deploy>();
    if data.len() < 1 + DEPLOY_BODY_SIZE {
        return Ok(None);
    }

    let body = &data[1..1 + DEPLOY_BODY_SIZE];
    let deploy: &Deploy = bytemuck::from_bytes(body);

    // Accounts layout from SDK deploy helper:
    // 0: signer
    // 1: authority
    // 2: automation_pda
    // 3: board
    // 4: miner_pda
    // 5: round_pda
    let accounts = ix
        .get("accounts")
        .and_then(Value::as_array)
        .ok_or_else(|| HeliusError::Decode("missing accounts".into()))?;

    let get_key = |ix_index: usize| -> Result<Pubkey, HeliusError> {
        let acc_idx = accounts
            .get(ix_index)
            .and_then(Value::as_u64)
            .ok_or_else(|| HeliusError::Decode("bad account index".into()))? as usize;

        let key = account_keys
            .get(acc_idx)
            .ok_or_else(|| HeliusError::Decode("account index out of range".into()))?;
        Ok(*key)
    };

    let signer = get_key(0)?;
    let authority = get_key(1)?;
    let miner_pda = get_key(4)?;
    let round_pda = get_key(5)?;

    let ix_amount = u64::from_le_bytes(deploy.amount);
    let mask_u32 = u32::from_le_bytes(deploy.squares);

    let mut ix_squares = [false; 25];
    for i in 0..25 {
        ix_squares[i] = (mask_u32 & (1 << i)) != 0;
    }

    Ok(Some(DecodedDeployIx {
        signer,
        authority,
        miner_pda,
        round_pda,
        ix_amount,
        ix_squares,
    }))
}

// ============================================================================
// Automation Balance Tracking Types
// ============================================================================

/// An event that affects automation balance (found during backward scan)
#[derive(Debug, Clone)]
pub enum AutomationBalanceEvent {
    /// Automate instruction (open or close)
    Automate {
        slot: u64,
        signature: String,
        ix_index: u8,
        is_close: bool,
        /// Initial deposit (only for open)
        deposit: u64,
        /// Amount per square
        amount: u64,
        /// Mask (for Fixed) or num_squares (for Random)
        mask: u64,
        /// Strategy: 0=Fixed, 1=Random
        strategy: u8,
        /// Fee per deploy
        fee: u64,
        /// Executor pubkey
        executor: Pubkey,
    },
    /// ReloadSOL instruction (adds SOL to balance)
    ReloadSOL {
        slot: u64,
        signature: String,
        ix_index: u8,
        /// Amount of SOL added (from lamport transfer)
        amount: u64,
    },
    /// Deploy instruction (consumes balance)
    Deploy {
        slot: u64,
        signature: String,
        ix_index: u8,
        /// Round ID of this deployment
        round_id: u64,
        /// Squares from instruction (may not be what actually deployed)
        ix_squares: u64,
    },
}

impl AutomationBalanceEvent {
    pub fn slot(&self) -> u64 {
        match self {
            Self::Automate { slot, .. } => *slot,
            Self::ReloadSOL { slot, .. } => *slot,
            Self::Deploy { slot, .. } => *slot,
        }
    }
    
    pub fn signature(&self) -> &str {
        match self {
            Self::Automate { signature, .. } => signature,
            Self::ReloadSOL { signature, .. } => signature,
            Self::Deploy { signature, .. } => signature,
        }
    }
}

/// Result of calculating actual squares deployed for a single deploy
#[derive(Debug, Clone)]
pub struct CalculatedDeployment {
    pub slot: u64,
    pub signature: String,
    pub ix_index: u8,
    pub round_id: u64,
    /// Balance BEFORE this deploy
    pub balance_before: u64,
    /// Balance AFTER this deploy
    pub balance_after: u64,
    /// Actual mask of squares deployed (calculated from strategy + balance)
    pub actual_mask: u64,
    /// Number of squares actually deployed
    pub actual_squares: u8,
    /// Total SOL spent (squares * amount + fee)
    pub total_spent: u64,
    /// Was this a partial deploy (ran out of balance)?
    pub is_partial: bool,
}

/// Generate random mask using same algorithm as ORE program
pub fn generate_random_mask(num_squares: u64, authority: &Pubkey, round_id: u64) -> u64 {
    use solana_sdk::keccak::hashv;
    
    let r = hashv(&[&authority.to_bytes(), &round_id.to_le_bytes()]).0;
    let mut mask = 0u64;
    let mut selected = 0u64;
    
    for i in 0..25usize {
        let rand_byte = r[i] as u64;
        let remaining_needed = num_squares.saturating_sub(selected);
        let remaining_positions = (25 - i) as u64;
        
        if remaining_needed > 0 && rand_byte * remaining_positions < remaining_needed * 256 {
            mask |= 1u64 << i;
            selected += 1;
        }
    }
    
    mask
}

/// Count bits set in a mask
pub fn count_squares(mask: u64) -> u8 {
    mask.count_ones() as u8
}

/// Calculate actual deployment given strategy, mask, balance, amount, fee
/// Returns (actual_mask, actual_squares, total_spent, is_partial)
pub fn calculate_actual_deployment(
    strategy: u8,
    mask: u64,
    authority: &Pubkey,
    round_id: u64,
    balance: u64,
    amount_per_square: u64,
    fee: u64,
) -> (u64, u8, u64, bool) {
    // Determine desired squares based on strategy
    let desired_mask = match strategy {
        1 => {
            // Random strategy: mask's lower 8 bits = number of squares
            let num_squares = (mask & 0xFF).min(25);
            generate_random_mask(num_squares, authority, round_id)
        }
        _ => mask, // Fixed strategy or other
    };
    
    // Now calculate how many squares we can actually afford
    // Deploy goes through squares 0-24 in order
    let mut actual_mask = 0u64;
    let mut actual_squares = 0u8;
    let mut total_spent = 0u64;
    
    for i in 0..25 {
        if desired_mask & (1u64 << i) != 0 {
            // This square is in the desired mask
            let cost = total_spent + fee + amount_per_square;
            if cost > balance {
                // Can't afford this square - partial deploy
                break;
            }
            actual_mask |= 1u64 << i;
            actual_squares += 1;
            total_spent += amount_per_square;
        }
    }
    
    // Add fee if we deployed any squares
    if actual_squares > 0 {
        total_spent += fee;
    }
    
    let is_partial = count_squares(desired_mask) > actual_squares;
    
    (actual_mask, actual_squares, total_spent, is_partial)
}

/// Result of scanning automation history with full balance tracking
#[derive(Debug, Clone)]
pub struct AutomationHistoryScan {
    /// All events found, sorted chronologically (oldest first)
    pub events: Vec<AutomationBalanceEvent>,
    /// The Automate Open event that started this automation session
    pub automate_open: Option<AutomateOpenInfo>,
    /// Calculated deployments with balance info
    pub calculated_deploys: Vec<CalculatedDeployment>,
    /// Stats
    pub txns_searched: u32,
    pub pages_fetched: u32,
}

/// Info about an Automate Open instruction
#[derive(Debug, Clone)]
pub struct AutomateOpenInfo {
    pub slot: u64,
    pub signature: String,
    pub ix_index: u8,
    pub deposit: u64,
    pub amount: u64,
    pub mask: u64,
    pub strategy: u8,
    pub fee: u64,
    pub executor: Pubkey,
}

impl HeliusApi {
    /// Scan backwards from target_slot collecting ALL automation-related events:
    /// - Automate (open/close)
    /// - ReloadSOL
    /// - Deploy
    /// 
    /// Returns events sorted chronologically and pre-calculated deployment info.
    pub async fn scan_automation_history_with_balance(
        &mut self,
        authority: &Pubkey,
        target_slot: u64,
        stop_at_slot: Option<u64>,
    ) -> Result<AutomationHistoryScan, HeliusError> {
        let automation_pda = ore_api::automation_pda(*authority).0;
        let mut events: Vec<AutomationBalanceEvent> = Vec::new();
        let mut txns_searched = 0u32;
        let mut pages_fetched = 0u32;
        let mut found_automate_open = false;
        
        let mut pagination_token: Option<String> = None;
        
        'outer: loop {
            let page = self
                .get_transactions_for_address(
                    &automation_pda,
                    pagination_token.clone(),
                    Some(100),
                    Some("desc"),  // Scan backwards
                    None,
                    Some(target_slot),
                )
                .await?;
            
            pages_fetched += 1;
            
            if page.transactions.is_empty() {
                break;
            }
            
            for tx in &page.transactions {
                txns_searched += 1;
                
                let slot = tx.get("slot").and_then(Value::as_u64).unwrap_or(0);
                
                // Early stop check
                if let Some(stop) = stop_at_slot {
                    if slot <= stop {
                        break 'outer;
                    }
                }
                
                let signature = tx
                    .get("transaction")
                    .and_then(|t| t.get("signatures"))
                    .and_then(Value::as_array)
                    .and_then(|arr| arr.first())
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                
                let message = match tx.get("transaction").and_then(|t| t.get("message")) {
                    Some(m) => m,
                    None => continue,
                };
                
                let account_keys_arr = match message.get("accountKeys").and_then(Value::as_array) {
                    Some(a) => a,
                    None => continue,
                };
                
                let mut account_keys = Vec::with_capacity(account_keys_arr.len());
                for k in account_keys_arr {
                    if let Some(s) = k.as_str() {
                        if let Ok(pk) = Pubkey::try_from(s) {
                            account_keys.push(pk);
                        }
                    }
                }
                
                // Parse pre/post balances for ReloadSOL detection
                let pre_balances: Vec<u64> = tx
                    .get("meta")
                    .and_then(|m| m.get("preBalances"))
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();
                
                let post_balances: Vec<u64> = tx
                    .get("meta")
                    .and_then(|m| m.get("postBalances"))
                    .and_then(Value::as_array)
                    .map(|arr| arr.iter().filter_map(|v| v.as_u64()).collect())
                    .unwrap_or_default();
                
                // Find automation PDA index for balance tracking
                let automation_idx = account_keys.iter().position(|k| *k == automation_pda);
                
                // Process outer instructions
                if let Some(ixs) = message.get("instructions").and_then(Value::as_array) {
                    for (ix_idx, ix) in ixs.iter().enumerate() {
                        if let Some(event) = self.parse_automation_event(
                            ix,
                            &account_keys,
                            authority,
                            &automation_pda,
                            slot,
                            &signature,
                            ix_idx as u8,
                            automation_idx,
                            &pre_balances,
                            &post_balances,
                        )? {
                            if let AutomationBalanceEvent::Automate { is_close: false, .. } = &event {
                                found_automate_open = true;
                            }
                            events.push(event);
                        }
                    }
                }
                
                // Process inner instructions
                if let Some(inner_arr) = tx
                    .get("meta")
                    .and_then(|m| m.get("innerInstructions"))
                    .and_then(Value::as_array)
                {
                    for inner in inner_arr {
                        let parent_ix = inner.get("index").and_then(Value::as_u64).unwrap_or(0) as u8;
                        if let Some(ixs) = inner.get("instructions").and_then(Value::as_array) {
                            for (inner_idx, ix) in ixs.iter().enumerate() {
                                if let Some(event) = self.parse_automation_event(
                                    ix,
                                    &account_keys,
                                    authority,
                                    &automation_pda,
                                    slot,
                                    &signature,
                                    parent_ix,  // Use parent ix index
                                    automation_idx,
                                    &pre_balances,
                                    &post_balances,
                                )? {
                                    if let AutomationBalanceEvent::Automate { is_close: false, .. } = &event {
                                        found_automate_open = true;
                                    }
                                    events.push(event);
                                }
                            }
                        }
                    }
                }
                
                // If we found an Automate Open, we have everything we need
                if found_automate_open {
                    break 'outer;
                }
            }
            
            pagination_token = page.pagination_token;
            if pagination_token.is_none() {
                break;
            }
        }
        
        // Sort events chronologically (oldest first)
        events.sort_by_key(|e| e.slot());
        
        // Extract Automate Open info
        let automate_open = events.iter().find_map(|e| {
            if let AutomationBalanceEvent::Automate {
                slot, signature, ix_index, is_close: false,
                deposit, amount, mask, strategy, fee, executor
            } = e {
                Some(AutomateOpenInfo {
                    slot: *slot,
                    signature: signature.clone(),
                    ix_index: *ix_index,
                    deposit: *deposit,
                    amount: *amount,
                    mask: *mask,
                    strategy: *strategy,
                    fee: *fee,
                    executor: *executor,
                })
            } else {
                None
            }
        });
        
        // Calculate deployments with balance tracking
        let calculated_deploys = if let Some(ref open) = automate_open {
            self.calculate_deployments_with_balance(authority, &events, open)
        } else {
            Vec::new()
        };
        
        Ok(AutomationHistoryScan {
            events,
            automate_open,
            calculated_deploys,
            txns_searched,
            pages_fetched,
        })
    }
    
    /// Parse a single instruction and return an automation event if relevant
    fn parse_automation_event(
        &self,
        ix: &Value,
        account_keys: &[Pubkey],
        authority: &Pubkey,
        automation_pda: &Pubkey,
        slot: u64,
        signature: &str,
        ix_index: u8,
        automation_idx: Option<usize>,
        pre_balances: &[u64],
        post_balances: &[u64],
    ) -> Result<Option<AutomationBalanceEvent>, HeliusError> {
        let prog_idx = ix
            .get("programIdIndex")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX) as usize;
        
        let program_id = account_keys.get(prog_idx).copied().unwrap_or_default();
        
        // Must be ORE program
        if program_id != ore_api::PROGRAM_ID {
            return Ok(None);
        }
        
        let data = ix
            .get("data")
            .and_then(Value::as_str)
            .map(bs58::decode)
            .and_then(|d| d.into_vec().ok())
            .unwrap_or_default();
        
        if data.is_empty() {
            return Ok(None);
        }
        
        let tag = data[0];
        let ore_tag = match OreInstruction::try_from(tag) {
            Ok(t) => t,
            Err(_) => return Ok(None),
        };
        
        match ore_tag {
            OreInstruction::Automate => {
                // Parse Automate instruction
                const AUTOMATE_BODY_SIZE: usize = core::mem::size_of::<Automate>();
                if data.len() < 1 + AUTOMATE_BODY_SIZE {
                    return Ok(None);
                }
                
                let body = &data[1..1 + AUTOMATE_BODY_SIZE];
                let automate: &Automate = bytemuck::from_bytes(body);
                
                let accounts = ix.get("accounts").and_then(Value::as_array);
                if accounts.is_none() {
                    return Ok(None);
                }
                let accounts = accounts.unwrap();
                
                // Accounts: [signer, automation_info, executor_info, miner_info, system_program]
                let get_key = |idx: usize| -> Option<Pubkey> {
                    let acc_idx = accounts.get(idx)?.as_u64()? as usize;
                    account_keys.get(acc_idx).copied()
                };
                
                let signer = match get_key(0) {
                    Some(k) => k,
                    None => return Ok(None),
                };
                let autom_acc = match get_key(1) {
                    Some(k) => k,
                    None => return Ok(None),
                };
                let executor = get_key(2).unwrap_or_default();
                
                // Must match expected authority and automation PDA
                if signer != *authority || autom_acc != *automation_pda {
                    return Ok(None);
                }
                
                let is_close = executor == Pubkey::default();
                
                Ok(Some(AutomationBalanceEvent::Automate {
                    slot,
                    signature: signature.to_string(),
                    ix_index,
                    is_close,
                    deposit: u64::from_le_bytes(automate.deposit),
                    amount: u64::from_le_bytes(automate.amount),
                    mask: u64::from_le_bytes(automate.mask),
                    strategy: automate.strategy,
                    fee: u64::from_le_bytes(automate.fee),
                    executor,
                }))
            }
            
            OreInstruction::ReloadSOL => {
                // ReloadSOL adds SOL from miner account to automation account
                // The amount is determined by the lamport delta on the automation account
                let reload_amount = if let Some(auto_idx) = automation_idx {
                    let pre = pre_balances.get(auto_idx).copied().unwrap_or(0);
                    let post = post_balances.get(auto_idx).copied().unwrap_or(0);
                    post.saturating_sub(pre)
                } else {
                    0
                };
                
                Ok(Some(AutomationBalanceEvent::ReloadSOL {
                    slot,
                    signature: signature.to_string(),
                    ix_index,
                    amount: reload_amount,
                }))
            }
            
            OreInstruction::Deploy => {
                // Parse Deploy instruction to get round_id and squares
                const DEPLOY_BODY_SIZE: usize = core::mem::size_of::<Deploy>();
                if data.len() < 1 + DEPLOY_BODY_SIZE {
                    return Ok(None);
                }
                
                let body = &data[1..1 + DEPLOY_BODY_SIZE];
                let deploy: &Deploy = bytemuck::from_bytes(body);
                
                // Get round account to determine round_id
                let accounts = ix.get("accounts").and_then(Value::as_array);
                if accounts.is_none() {
                    return Ok(None);
                }
                let accounts = accounts.unwrap();
                
                // Accounts: [signer, automation_info, miner_info, round_info, treasury, system_program]
                let get_key = |idx: usize| -> Option<Pubkey> {
                    let acc_idx = accounts.get(idx)?.as_u64()? as usize;
                    account_keys.get(acc_idx).copied()
                };
                
                let _signer = match get_key(0) {
                    Some(k) => k,
                    None => return Ok(None),
                };
                let autom_acc = get_key(1);
                
                // Check if this deploy uses automation (automation account matches)
                let uses_automation = autom_acc.map(|a| a == *automation_pda).unwrap_or(false);
                if !uses_automation {
                    return Ok(None);
                }
                
                // We need to determine round_id from the round account
                // For now, we'll store the instruction squares and determine round_id later
                // when we correlate with round data
                let ix_squares = u32::from_le_bytes(deploy.squares) as u64;
                
                Ok(Some(AutomationBalanceEvent::Deploy {
                    slot,
                    signature: signature.to_string(),
                    ix_index,
                    round_id: 0, // Will be filled in during balance calculation or from round data
                    ix_squares,
                }))
            }
            
            _ => Ok(None),
        }
    }
    
    /// Calculate balance at each deployment given the events and initial deposit
    fn calculate_deployments_with_balance(
        &self,
        authority: &Pubkey,
        events: &[AutomationBalanceEvent],
        automate_open: &AutomateOpenInfo,
    ) -> Vec<CalculatedDeployment> {
        let mut balance = automate_open.deposit;
        let mut results = Vec::new();
        
        // Current automation settings (from open, may be updated by subsequent Automate calls)
        let mut current_amount = automate_open.amount;
        let mut current_mask = automate_open.mask;
        let mut current_strategy = automate_open.strategy;
        let mut current_fee = automate_open.fee;
        let mut automation_active = true;
        
        for event in events {
            match event {
                AutomationBalanceEvent::Automate {
                    is_close, deposit, amount, mask, strategy, fee, ..
                } => {
                    if *is_close {
                        automation_active = false;
                    } else {
                        automation_active = true;
                        balance += *deposit;  // New deposit adds to balance
                        current_amount = *amount;
                        current_mask = *mask;
                        current_strategy = *strategy;
                        current_fee = *fee;
                    }
                }
                
                AutomationBalanceEvent::ReloadSOL { amount, .. } => {
                    if automation_active {
                        balance += *amount;
                    }
                }
                
                AutomationBalanceEvent::Deploy {
                    slot, signature, ix_index, round_id, ix_squares
                } => {
                    if !automation_active {
                        continue;
                    }
                    
                    let balance_before = balance;
                    
                    // Calculate actual deployment
                    let (actual_mask, actual_squares, total_spent, is_partial) =
                        calculate_actual_deployment(
                            current_strategy,
                            current_mask,
                            authority,
                            *round_id,
                            balance,
                            current_amount,
                            current_fee,
                        );
                    
                    balance = balance.saturating_sub(total_spent);
                    
                    results.push(CalculatedDeployment {
                        slot: *slot,
                        signature: signature.clone(),
                        ix_index: *ix_index,
                        round_id: *round_id,
                        balance_before,
                        balance_after: balance,
                        actual_mask,
                        actual_squares,
                        total_spent,
                        is_partial,
                    });
                }
            }
        }
        
        results
    }
}
