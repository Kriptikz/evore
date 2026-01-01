//! Transaction backfill and migration
//!
//! Background task to migrate existing transactions from `raw_transactions` (round_id-based)
//! to the new `signatures` and `raw_transactions_v2` tables (account-based).
//!
//! Features:
//! - Restart-safe: tracks progress via database state
//! - Idempotent: skips already-migrated transactions
//! - Parses all accounts including LUT-resolved addresses

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use tokio::task::JoinHandle;

use crate::app_state::AppState;
use crate::clickhouse::{RawTransactionV2, SignatureRow};

/// Spawn background task to migrate old transactions to new tables
pub fn spawn_txn_backfill_task(state: Arc<AppState>) -> JoinHandle<()> {
    tokio::spawn(async move {
        tracing::info!("Starting transaction migration backfill task...");
        
        // Wait for startup to complete
        tokio::time::sleep(Duration::from_secs(30)).await;
        
        loop {
            // Get next batch of unmigrated transactions
            match migrate_next_round(&state).await {
                Ok(Some(round_id)) => {
                    tracing::info!("Migrated transactions for round {}", round_id);
                }
                Ok(None) => {
                    tracing::info!("Transaction migration complete!");
                    break;
                }
                Err(e) => {
                    tracing::error!("Migration error: {}", e);
                    tokio::time::sleep(Duration::from_secs(5)).await;
                }
            }
            
            // Small delay between rounds to avoid overloading
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
}

/// Migrate transactions for the next unmigrated round
async fn migrate_next_round(state: &AppState) -> Result<Option<u64>> {
    // Find earliest round in raw_transactions not yet in raw_transactions_v2
    let round_id = match state.clickhouse.get_next_unmigrated_round().await? {
        Some(id) => id,
        None => return Ok(None),
    };
    
    // Get all transactions for this round
    let old_txns = state.clickhouse.get_raw_transactions_for_round(round_id).await?;
    
    tracing::info!("Migrating {} transactions for round {}", old_txns.len(), round_id);
    
    let mut migrated_count = 0;
    let mut sig_rows = Vec::new();
    let mut tx_rows = Vec::new();
    
    for old_tx in old_txns {
        // Check if already migrated
        if state.clickhouse.transaction_exists_v2(&old_tx.signature).await? {
            continue;
        }
        
        // Parse accounts from raw_json
        let accounts = match parse_transaction_accounts(&old_tx.raw_json) {
            Ok(acc) => acc,
            Err(e) => {
                tracing::warn!(
                    "Failed to parse accounts for tx {}: {}. Using fallback.",
                    old_tx.signature, e
                );
                // Fallback: use signer and authority if available
                let mut fallback = Vec::new();
                if !old_tx.signer.is_empty() {
                    fallback.push(old_tx.signer.clone());
                }
                if !old_tx.authority.is_empty() && old_tx.authority != old_tx.signer {
                    fallback.push(old_tx.authority.clone());
                }
                fallback
            }
        };
        
        // Prepare signature row
        sig_rows.push(SignatureRow {
            signature: old_tx.signature.clone(),
            slot: old_tx.slot,
            block_time: old_tx.block_time,
            accounts: accounts.clone(),
        });
        
        // Prepare transaction row
        tx_rows.push(RawTransactionV2 {
            signature: old_tx.signature,
            slot: old_tx.slot,
            block_time: old_tx.block_time,
            accounts,
            raw_json: old_tx.raw_json,
        });
        
        migrated_count += 1;
        
        // Batch insert every 100 transactions
        if sig_rows.len() >= 100 {
            state.clickhouse.insert_signatures(sig_rows.drain(..).collect()).await?;
            state.clickhouse.insert_raw_transactions_v2(tx_rows.drain(..).collect()).await?;
        }
    }
    
    // Insert remaining
    if !sig_rows.is_empty() {
        state.clickhouse.insert_signatures(sig_rows).await?;
        state.clickhouse.insert_raw_transactions_v2(tx_rows).await?;
    }
    
    tracing::info!("Round {} migration complete: {} transactions", round_id, migrated_count);
    
    Ok(Some(round_id))
}

/// Parse all accounts from a transaction JSON, including LUT-resolved addresses
/// 
/// Account sources:
/// 1. Static account keys from transaction.message.accountKeys
/// 2. LUT-resolved addresses from meta.loadedAddresses.writable
/// 3. LUT-resolved addresses from meta.loadedAddresses.readonly
/// 
/// Returns deduplicated list of all accounts. The signer is always accounts[0].
pub fn parse_transaction_accounts(raw_json: &str) -> Result<Vec<String>> {
    let tx: serde_json::Value = serde_json::from_str(raw_json)?;
    let mut accounts = Vec::new();
    
    // Get static account keys
    // Format can be either:
    // - Array of pubkey strings
    // - Array of objects with "pubkey" field
    if let Some(keys) = tx["transaction"]["message"]["accountKeys"].as_array() {
        for key in keys {
            if let Some(pubkey) = key["pubkey"].as_str() {
                // Object format: { "pubkey": "..." }
                accounts.push(pubkey.to_string());
            } else if let Some(pubkey) = key.as_str() {
                // String format: "..."
                accounts.push(pubkey.to_string());
            }
        }
    }
    
    // Get LUT-resolved addresses (if present)
    // These are addresses resolved from Address Lookup Tables (ALTs)
    if let Some(loaded) = tx["meta"]["loadedAddresses"].as_object() {
        // Writable addresses
        if let Some(writable) = loaded["writable"].as_array() {
            for addr in writable {
                if let Some(s) = addr.as_str() {
                    accounts.push(s.to_string());
                }
            }
        }
        // Readonly addresses
        if let Some(readonly) = loaded["readonly"].as_array() {
            for addr in readonly {
                if let Some(s) = addr.as_str() {
                    accounts.push(s.to_string());
                }
            }
        }
    }
    
    // Remove duplicates while preserving order (signer is first)
    let mut seen = std::collections::HashSet::new();
    accounts.retain(|acc| seen.insert(acc.clone()));
    
    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_transaction_accounts_string_format() {
        let json = r#"{
            "transaction": {
                "message": {
                    "accountKeys": [
                        "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy",
                        "oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ",
                        "11111111111111111111111111111111"
                    ]
                }
            },
            "meta": {}
        }"#;
        
        let accounts = parse_transaction_accounts(json).unwrap();
        assert_eq!(accounts.len(), 3);
        assert_eq!(accounts[0], "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy");
    }
    
    #[test]
    fn test_parse_transaction_accounts_object_format() {
        let json = r#"{
            "transaction": {
                "message": {
                    "accountKeys": [
                        {"pubkey": "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy", "signer": true},
                        {"pubkey": "oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ", "signer": false}
                    ]
                }
            },
            "meta": {}
        }"#;
        
        let accounts = parse_transaction_accounts(json).unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0], "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy");
    }
    
    #[test]
    fn test_parse_transaction_accounts_with_lut() {
        let json = r#"{
            "transaction": {
                "message": {
                    "accountKeys": [
                        "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy"
                    ]
                }
            },
            "meta": {
                "loadedAddresses": {
                    "writable": ["oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ"],
                    "readonly": ["11111111111111111111111111111111"]
                }
            }
        }"#;
        
        let accounts = parse_transaction_accounts(json).unwrap();
        assert_eq!(accounts.len(), 3);
        // Signer first, then LUT-resolved
        assert_eq!(accounts[0], "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy");
        assert!(accounts.contains(&"oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ".to_string()));
    }
    
    #[test]
    fn test_parse_transaction_accounts_dedup() {
        let json = r#"{
            "transaction": {
                "message": {
                    "accountKeys": [
                        "7cVfgArCheMR6Cs4t6vz5rfnqd56vZq4ndaBrY5xkxXy",
                        "oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ"
                    ]
                }
            },
            "meta": {
                "loadedAddresses": {
                    "writable": ["oreV2ZymfyeXgNgBdqMkumTqqAprVqgBWQfoYkrtKWQ"],
                    "readonly": []
                }
            }
        }"#;
        
        let accounts = parse_transaction_accounts(json).unwrap();
        assert_eq!(accounts.len(), 2); // Deduplicated
    }
}


