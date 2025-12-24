use std::{collections::HashMap, mem, time::Duration};

use ore_api::{prelude::{Automate, Deploy, OreInstruction}, state::{self, AutomationStrategy}};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use solana_sdk::{bs58, keccak::hashv};
use steel::{Pod, ProgramError, Pubkey, Zeroable};
use thiserror::Error;
use tokio::time::Instant;

use crate::app_state::{AutomationCache, ReconstructedAutomation};

#[derive(Debug, Error)]
pub enum HeliusError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("rpc error: {0}")]
    Rpc(String),

    #[error("invalid rpc response")]
    InvalidResponse,

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
}

impl HeliusApi {
    pub fn new(rpc_url: impl Into<String>) -> Self {
        Self {
            rpc_url: rpc_url.into(),
            client: Client::new(),
            last_request_at: Instant::now(),
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
        let (round_pda, _bump) = state::round_pda(round_id);

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

        let resp: RpcResponse<GetTransactionsResult> = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let result = resp.result.ok_or(HeliusError::InvalidResponse)?;

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
        let (automation_pda, _bump) = ore_api::state::automation_pda(*authority);

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

            if *program_id != ore_api::ID {
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

        for tx in txs {
            // Skip failed transactions (we also filter status=succeeded in the RPC call,
            // but this is an extra safety check).
            let err = tx.get("meta").and_then(|m| m.get("err"));
            if !err.map_or(true, |e| e.is_null()) {
                continue;
            }

            let meta = tx
                .get("meta")
                .ok_or_else(|| HeliusError::Decode("missing meta".into()))?;

            // Slot
            let slot = tx
                .get("slot")
                .and_then(Value::as_u64)
                .ok_or_else(|| HeliusError::Decode("missing slot".into()))?;

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

            // Balances
            let pre_balances_json = meta
                .get("preBalances")
                .and_then(Value::as_array)
                .ok_or_else(|| HeliusError::Decode("missing preBalances".into()))?;

            let post_balances_json = meta
                .get("postBalances")
                .and_then(Value::as_array)
                .ok_or_else(|| HeliusError::Decode("missing postBalances".into()))?;

            if pre_balances_json.len() != post_balances_json.len()
                || pre_balances_json.len() != account_keys.len()
            {
                return Err(HeliusError::Decode(
                    "pre/post balances length mismatch with accountKeys".into(),
                ));
            }

            let mut pre_balances: Vec<u64> = Vec::with_capacity(pre_balances_json.len());
            let mut post_balances: Vec<u64> = Vec::with_capacity(post_balances_json.len());

            for v in pre_balances_json {
                let n = v
                    .as_u64()
                    .ok_or_else(|| HeliusError::Decode("preBalance not u64".into()))?;
                pre_balances.push(n);
            }
            for v in post_balances_json {
                let n = v
                    .as_u64()
                    .ok_or_else(|| HeliusError::Decode("postBalance not u64".into()))?;
                post_balances.push(n);
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
            if let Some(ixs) = message
                .get("instructions")
                .and_then(Value::as_array)
            {
                for ix in ixs {
                    if let Some(decoded) = decode_ore_deploy_ix(ix, &account_keys)? {
                        if decoded.round_pda != expected_round_pda {
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
        }

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

                if *program_id != ore_api::ID {
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

/// JSON-RPC envelope
#[derive(Debug, Deserialize)]
struct RpcResponse<T> {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(default)]
    pub result: Option<T>,
    // you can add `error` if you want to model RPC errors explicitly:
    // pub error: Option<RpcError>,
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

    if *program_id != ore_api::ID {
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
