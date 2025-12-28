//! Historical data backfill workflow
//!
//! Multi-phase admin workflow for reconstructing historical round data:
//! 1. Fetch round metadata from external API
//! 2. Fetch transactions via Helius
//! 3. Reconstruct deployments
//! 4. Verify and finalize

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::app_state::AppState;
use crate::clickhouse::RoundInsert;
use crate::external_api::get_ore_supply_rounds;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct BackfillRoundsResponse {
    pub rounds_fetched: u32,
    pub rounds_skipped: u32,
    pub rounds_missing_deployments: u32,
    pub stopped_at_round: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct BackfillRoundsQuery {
    pub stop_at_round: Option<u64>,
    pub max_pages: Option<u32>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RoundStatus {
    pub round_id: i64,
    pub meta_fetched: bool,
    pub transactions_fetched: bool,
    pub reconstructed: bool,
    pub verified: bool,
    pub finalized: bool,
    pub transaction_count: i32,
    pub deployment_count: i32,
    pub verification_notes: String,
}

#[derive(Debug, Serialize)]
pub struct PendingRoundsResponse {
    pub pending: Vec<RoundStatus>,
    pub total: u32,
}

#[derive(Debug, Serialize)]
pub struct FetchTxnsResponse {
    pub round_id: u64,
    pub transactions_fetched: u32,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ReconstructResponse {
    pub round_id: u64,
    pub deployments_reconstructed: u32,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRequest {
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyResponse {
    pub round_id: u64,
    pub verified: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct FinalizeResponse {
    pub round_id: u64,
    pub deployments_stored: u32,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteResponse {
    pub round_id: u64,
    pub round_deleted: bool,
    pub deployments_deleted: bool,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteQuery {
    pub delete_round: Option<bool>,
    pub delete_deployments: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct BulkDeleteRequest {
    pub round_ids: Vec<u64>,
    pub delete_rounds: bool,
    pub delete_deployments: bool,
}

#[derive(Debug, Serialize)]
pub struct BulkDeleteResponse {
    pub deleted_count: u32,
    pub failed_count: u32,
    pub message: String,
}

/// Request for adding rounds to the backfill workflow
#[derive(Debug, Deserialize)]
pub struct AddToBackfillRequest {
    pub round_ids: Vec<u64>,
}

#[derive(Debug, Serialize)]
pub struct AddToBackfillResponse {
    pub added: u32,
    pub already_pending: u32,
    pub message: String,
}

/// Response for a single round backfill operation (used by reconstruct endpoint)
#[derive(Debug, Serialize)]
pub struct BackfillDeploymentsResponse {
    pub round_id: u64,
    pub transactions_fetched: u32,
    pub deployments_found: u32,
    pub deployments_stored: u32,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoundDataStatus {
    pub round_id: u64,
    pub round_exists: bool,
    pub deployment_count: u64,
    pub deployments_sum: u64,
    pub total_deployed: u64,
    pub is_valid: bool,
    pub discrepancy: i64,
}

#[derive(Debug, Serialize)]
pub struct RoundWithData {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub total_deployed: u64,
    pub total_winnings: u64,
    pub unique_miners: u32,
    pub motherlode: u64,
    pub deployment_count: u64,
    pub source: String,
    /// Sum of all deployment amounts in the database
    pub deployments_sum: u64,
    /// true if deployments_sum matches total_deployed, false otherwise
    pub is_valid: bool,
    /// Difference between total_deployed and deployments_sum (positive = missing, negative = extra)
    pub discrepancy: i64,
}

#[derive(Debug, Serialize)]
pub struct RoundsWithDataResponse {
    pub rounds: Vec<RoundWithData>,
    pub total: u32,
    /// Whether there are more rounds available
    pub has_more: bool,
    /// Cursor for next page (use as `before` param)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<u64>,
    /// Current page number (if using page-based pagination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct RoundsWithDataQuery {
    /// Number of rounds per page (default 50, max 200)
    pub limit: Option<u32>,
    /// Page number (1-based, for offset pagination)
    pub page: Option<u32>,
    /// Cursor: get rounds before this round_id (for cursor-based pagination)
    pub before: Option<u64>,
    /// Filter: minimum round_id (inclusive)
    pub round_id_gte: Option<u64>,
    /// Filter: maximum round_id (inclusive)
    pub round_id_lte: Option<u64>,
    /// Filter mode: "all", "missing_deployments", "invalid_deployments", "missing_rounds"
    pub filter_mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MissingRoundsResponse {
    pub missing_round_ids: Vec<u64>,
    pub total: u32,
    pub has_more: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub min_stored_round: u64,
    pub max_stored_round: u64,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Handlers
// ============================================================================

/// POST /admin/backfill/rounds?stop_at_round={id}&max_pages={n}
/// Fetch round metadata from external API and store to ClickHouse
pub async fn backfill_rounds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<BackfillRoundsQuery>,
) -> Result<Json<BackfillRoundsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let stop_at_round = params.stop_at_round.unwrap_or(0);
    let max_pages = params.max_pages.unwrap_or(100);
    
    let mut rounds_fetched = 0u32;
    let mut rounds_skipped = 0u32;
    let mut rounds_missing_deployments = 0u32;
    let mut stopped_at: Option<u64> = None;
    
    tracing::info!("Starting backfill, stop_at_round={}, max_pages={}", stop_at_round, max_pages);
    
    for page in 0..max_pages {
        // Fetch from external API
        let rounds = get_ore_supply_rounds(page as u64).await;
        
        if rounds.is_empty() {
            tracing::info!("No more rounds at page {}", page);
            break;
        }
        
        let mut batch: Vec<RoundInsert> = Vec::new();
        let mut should_stop = false;
        
        for round in rounds {
            let round_id = round.round_id as u64;
            
            // Check if we should stop
            if round_id <= stop_at_round {
                stopped_at = Some(round_id);
                should_stop = true;
                break;
            }
            
            // Check if round already exists in ClickHouse
            let round_exists = check_round_exists(&state.clickhouse, round_id).await;
            
            // Also check if deployments exist
            let deployment_count = state.clickhouse.count_deployments_for_round(round_id).await.unwrap_or(0);
            
            if round_exists && deployment_count > 0 {
                // Fully complete - skip
                rounds_skipped += 1;
                continue;
            } else if round_exists && deployment_count == 0 {
                // Round exists but no deployments - mark for backfill
                rounds_missing_deployments += 1;
                // Update PostgreSQL to indicate needs deployment backfill
                update_round_status_meta_fetched(&state.postgres, round_id).await;
                continue;
            }
            
            // Round doesn't exist - create it
            let insert = RoundInsert::from_backfill(
                round_id,
                0, // start_slot - not available from external API
                round.created_at as u64, // Use timestamp as end_slot estimate
                round.winning_square as u8,
                round.top_miner.clone(),
                round.total_deployed as u64,
                round.total_vaulted as u64,
                round.total_winnings as u64,
                round.motherlode as u64,
                0, // unique_miners - will be updated after reconstruction
                round.created_at as u64,
            );
            
            batch.push(insert);
            
            // Update PostgreSQL status
            update_round_status_meta_fetched(&state.postgres, round_id).await;
        }
        
        // Insert batch to ClickHouse
        if !batch.is_empty() {
            let count = batch.len() as u32;
            if let Err(e) = state.clickhouse.insert_rounds(batch).await {
                tracing::error!("Failed to insert rounds batch: {}", e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: format!("ClickHouse insert failed: {}", e) }),
                ));
            }
            rounds_fetched += count;
            tracing::info!("Inserted {} rounds from page {}", count, page);
        }
        
        if should_stop {
            break;
        }
    }
    
    tracing::info!(
        "Backfill complete: {} fetched, {} skipped, {} missing deployments, stopped_at={:?}",
        rounds_fetched, rounds_skipped, rounds_missing_deployments, stopped_at
    );
    
    Ok(Json(BackfillRoundsResponse {
        rounds_fetched,
        rounds_skipped,
        rounds_missing_deployments,
        stopped_at_round: stopped_at,
    }))
}

/// GET /admin/rounds/pending?status={filter}
/// List rounds that need work (not finalized)
pub async fn get_pending_rounds(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PendingRoundsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pending = get_pending_rounds_from_db(&state.postgres).await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Database error: {}", e) }),
            )
        })?;
    
    let total = pending.len() as u32;
    
    Ok(Json(PendingRoundsResponse { pending, total }))
}

/// POST /admin/fetch-txns/{round_id}
/// Fetch transactions for a round via Helius v2 API and store to ClickHouse
pub async fn fetch_round_transactions(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<FetchTxnsResponse>, (StatusCode, Json<ErrorResponse>)> {
    use crate::clickhouse::RawTransaction;
    
    tracing::info!("Fetching transactions for round {}", round_id);
    
    // Fetch all pages of transactions
    let mut all_transactions = Vec::new();
    let mut pagination_token: Option<String> = None;
    let mut page_count = 0u32;
    
    loop {
        let mut helius = state.helius.write().await;
        let result = helius.get_transactions_for_round(round_id, pagination_token.clone()).await;
        
        match result {
            Ok(page) => {
                let tx_count = page.transactions.len();
                all_transactions.extend(page.transactions);
                page_count += 1;
                
                tracing::info!(
                    "Round {} fetch: page {} with {} transactions (total: {})",
                    round_id, page_count, tx_count, all_transactions.len()
                );
                
                if page.pagination_token.is_none() {
                    break;
                }
                pagination_token = page.pagination_token;
                
                // Safety limit
                if page_count > 100 {
                    tracing::warn!("Round {} fetch: hit page limit", round_id);
                    break;
                }
            }
            Err(e) => {
                tracing::error!("Failed to fetch transactions for round {}: {}", round_id, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse { error: format!("Helius error: {}", e) }),
                ));
            }
        }
    }
    
    // Convert to RawTransaction for storage
    let mut raw_txs: Vec<RawTransaction> = Vec::new();
    
    for tx in &all_transactions {
        let signature = tx
            .get("transaction")
            .and_then(|t| t.get("signatures"))
            .and_then(|s| s.as_array())
            .and_then(|sigs| sigs.get(0))
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string();
        
        let slot = tx.get("slot").and_then(|s| s.as_u64()).unwrap_or(0);
        let block_time = tx.get("blockTime").and_then(|t| t.as_i64()).unwrap_or(0);
        
        // Get signer from first account
        let signer = tx
            .get("transaction")
            .and_then(|t| t.get("message"))
            .and_then(|m| m.get("accountKeys"))
            .and_then(|a| a.as_array())
            .and_then(|keys| keys.get(0))
            .and_then(|k| k.as_str())
            .unwrap_or_default()
            .to_string();
        
        raw_txs.push(RawTransaction {
            signature,
            slot,
            block_time,
            round_id,
            tx_type: "deploy".to_string(),
            raw_json: tx.to_string(),
            signer,
            authority: String::new(), // Will be parsed during reconstruction
        });
    }
    
    let tx_count = raw_txs.len() as u32;
    
    // Store to ClickHouse
    if !raw_txs.is_empty() {
        if let Err(e) = state.clickhouse.insert_raw_transactions(raw_txs).await {
            tracing::error!("Failed to store raw transactions: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Failed to store transactions: {}", e) }),
            ));
        }
    }
    
    // Update PostgreSQL status
    update_round_status_txns_fetched(&state.postgres, round_id, tx_count as i32).await;
    
    tracing::info!("Round {} fetch complete: {} transactions stored", round_id, tx_count);
    
    Ok(Json(FetchTxnsResponse {
        round_id,
        transactions_fetched: tx_count,
        status: "success".to_string(),
    }))
}

/// POST /admin/reconstruct/{round_id}
/// Reconstruct deployments from stored transactions
pub async fn reconstruct_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<ReconstructResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Reconstructing deployments for round {}", round_id);
    
    // Call the actual backfill function
    let result = backfill_round_deployments(&state, round_id).await;
    
    match result {
        Ok(resp) => Ok(Json(ReconstructResponse {
            round_id,
            deployments_reconstructed: resp.deployments_stored,
            status: resp.status,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: e }),
        )),
    }
}

/// Backfill deployments for a single round
/// 
/// Flow:
/// 1. Get stored transactions from ClickHouse
/// 2. Parse deploy instructions from transactions
/// 3. Build and store deployment records to ClickHouse
pub async fn backfill_round_deployments(
    state: &AppState,
    round_id: u64,
) -> Result<BackfillDeploymentsResponse, String> {
    use crate::clickhouse::DeploymentInsert;
    use std::collections::HashMap;
    
    tracing::info!("Starting deployment backfill for round {}", round_id);
    
    // Get round info for validation
    let round_info = state.clickhouse.get_round_by_id(round_id).await
        .map_err(|e| format!("Failed to get round info: {}", e))?
        .ok_or_else(|| format!("Round {} not found in ClickHouse", round_id))?;
    
    let winning_square = round_info.winning_square;
    let top_miner = round_info.top_miner.clone();
    
    // Derive the round PDA
    let (round_pda, _) = evore::ore_api::round_pda(round_id);
    
    // Get stored transactions from ClickHouse
    let raw_transactions = state.clickhouse.get_raw_transactions_for_round(round_id).await
        .map_err(|e| format!("Failed to get stored transactions: {}", e))?;
    
    if raw_transactions.is_empty() {
        return Ok(BackfillDeploymentsResponse {
            round_id,
            transactions_fetched: 0,
            deployments_found: 0,
            deployments_stored: 0,
            status: "no_transactions_stored".to_string(),
            error: Some("No transactions stored. Run fetch-txns first.".to_string()),
        });
    }
    
    tracing::info!(
        "Round {} backfill: found {} stored transactions",
        round_id, raw_transactions.len()
    );
    
    // Log first transaction raw_json length to check if it's actually stored
    if let Some(first) = raw_transactions.first() {
        tracing::info!(
            "Round {} backfill: first tx sig={}, raw_json len={}",
            round_id, first.signature, first.raw_json.len()
        );
    }
    
    // Parse raw_json back to Value for processing
    let mut all_transactions: Vec<serde_json::Value> = Vec::new();
    let mut parse_errors = 0;
    for raw_tx in &raw_transactions {
        match serde_json::from_str(&raw_tx.raw_json) {
            Ok(tx) => all_transactions.push(tx),
            Err(e) => {
                parse_errors += 1;
                if parse_errors <= 3 {
                    tracing::warn!(
                        "Failed to parse stored transaction {}: {}",
                        raw_tx.signature, e
                    );
                }
            }
        }
    }
    
    tracing::info!(
        "Round {} backfill: parsed {}/{} transactions successfully (errors: {})",
        round_id, all_transactions.len(), raw_transactions.len(), parse_errors
    );
    
    if all_transactions.is_empty() {
        return Ok(BackfillDeploymentsResponse {
            round_id,
            transactions_fetched: raw_transactions.len() as u32,
            deployments_found: 0,
            deployments_stored: 0,
            status: "parse_error".to_string(),
            error: Some("Failed to parse any stored transactions".to_string()),
        });
    }
    
    // Parse deployments from transactions
    tracing::info!(
        "Round {} backfill: looking for deployments matching round PDA {}",
        round_id, round_pda
    );
    
    let helius = state.helius.read().await;
    let parsed_deployments = helius.parse_deployments_from_round_page(&round_pda, &all_transactions)
        .map_err(|e| format!("Failed to parse deployments: {}", e))?;
    
    tracing::info!(
        "Round {} backfill: parse_deployments_from_round_page returned {} deployments",
        round_id, parsed_deployments.len()
    );
    
    if parsed_deployments.is_empty() {
        return Ok(BackfillDeploymentsResponse {
            round_id,
            transactions_fetched: all_transactions.len() as u32,
            deployments_found: 0,
            deployments_stored: 0,
            status: "no_deployments_found".to_string(),
            error: None,
        });
    }
    
    tracing::info!(
        "Round {} backfill: parsed {} deployment instructions",
        round_id, parsed_deployments.len()
    );
    
    // Aggregate deployments per miner per square
    // We track (total_amount, earliest_slot) per (miner, square)
    let mut miner_squares: HashMap<(String, u8), (u64, u64)> = HashMap::new();
    
    for pd in &parsed_deployments {
        let miner_pubkey = pd.authority.to_string();
        
        for (square_idx, is_deployed) in pd.squares.iter().enumerate() {
            if *is_deployed {
                let square_id = square_idx as u8;
                let key = (miner_pubkey.clone(), square_id);
                
                miner_squares.entry(key)
                    .and_modify(|(amt, slot)| {
                        *amt += pd.amount_per_square;
                        // Keep earliest slot
                        if pd.slot < *slot {
                            *slot = pd.slot;
                        }
                    })
                    .or_insert((pd.amount_per_square, pd.slot));
            }
        }
    }
    
    // Build deployment inserts
    let mut deployments: Vec<DeploymentInsert> = Vec::new();
    
    for ((miner_pubkey, square_id), (amount, slot)) in miner_squares {
        let is_winner = square_id == winning_square;
        let is_top = miner_pubkey == top_miner;
        
        // For historical backfill, we don't have exact reward data
        // Set ore/sol earned to 0 - they can be recalculated if needed
        deployments.push(DeploymentInsert {
            round_id,
            miner_pubkey,
            square_id,
            amount,
            deployed_slot: slot,
            ore_earned: 0,
            sol_earned: 0,
            is_winner: if is_winner { 1 } else { 0 },
            is_top_miner: if is_top { 1 } else { 0 },
        });
    }
    
    let deployments_count = deployments.len() as u32;
    
    // Store to ClickHouse
    state.clickhouse.insert_deployments(deployments).await
        .map_err(|e| format!("Failed to insert deployments: {}", e))?;
    
    // Update PostgreSQL status
    update_round_status_reconstructed(&state.postgres, round_id, deployments_count as i32).await;
    update_round_status_finalized(&state.postgres, round_id).await;
    
    tracing::info!(
        "Round {} backfill complete: {} transactions -> {} deployments stored",
        round_id, all_transactions.len(), deployments_count
    );
    
    Ok(BackfillDeploymentsResponse {
        round_id,
        transactions_fetched: all_transactions.len() as u32,
        deployments_found: parsed_deployments.len() as u32,
        deployments_stored: deployments_count,
        status: "success".to_string(),
        error: None,
    })
}

/// POST /admin/backfill/deployments
/// Add rounds to the backfill workflow for step-by-step processing
/// (For rounds that already have metadata but are missing deployments)
pub async fn add_to_backfill_workflow(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddToBackfillRequest>,
) -> Result<Json<AddToBackfillResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Adding {} rounds to backfill workflow", req.round_ids.len());
    
    let mut added = 0u32;
    let mut already_pending = 0u32;
    
    for round_id in &req.round_ids {
        // Check if already in pending list
        let existing = get_round_status(&state.postgres, *round_id as i64).await;
        
        match existing {
            Ok(Some(_)) => {
                // Already exists in pending list
                already_pending += 1;
            }
            Ok(None) => {
                // Add to pending list with meta_fetched=true (since round already exists in ClickHouse)
                add_round_to_backfill_workflow(&state.postgres, *round_id).await;
                added += 1;
            }
            Err(e) => {
                tracing::error!("Error checking round {} status: {}", round_id, e);
            }
        }
    }
    
    let message = format!(
        "Added {} rounds to backfill workflow, {} were already pending",
        added, already_pending
    );
    
    tracing::info!("{}", message);
    
    Ok(Json(AddToBackfillResponse {
        added,
        already_pending,
        message,
    }))
}

/// GET /admin/verify/{round_id}
/// Get reconstructed data for verification
pub async fn get_round_for_verification(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<RoundStatus>, (StatusCode, Json<ErrorResponse>)> {
    let status = get_round_status(&state.postgres, round_id as i64).await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Database error: {}", e) }),
            )
        })?;
    
    match status {
        Some(s) => Ok(Json(s)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: format!("Round {} not found", round_id) }),
        )),
    }
}

/// POST /admin/verify/{round_id}
/// Mark round as verified
pub async fn verify_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
    Json(req): Json<VerifyRequest>,
) -> Result<Json<VerifyResponse>, (StatusCode, Json<ErrorResponse>)> {
    let notes = req.notes.unwrap_or_default();
    
    update_round_status_verified(&state.postgres, round_id, &notes).await;
    
    Ok(Json(VerifyResponse {
        round_id,
        verified: true,
        message: "Round marked as verified".to_string(),
    }))
}

/// POST /admin/finalize/{round_id}
/// Finalize round and store deployments to ClickHouse
pub async fn finalize_backfill_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<FinalizeResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Finalizing backfill round {}", round_id);
    
    // TODO: Implement finalization
    // 1. Load reconstructed deployments
    // 2. Store to ClickHouse deployments table
    // 3. Update status
    
    update_round_status_finalized(&state.postgres, round_id).await;
    
    Ok(Json(FinalizeResponse {
        round_id,
        deployments_stored: 0,
        message: "Round finalized".to_string(),
    }))
}

#[derive(Debug, Serialize)]
pub struct ResetTxnsResponse {
    pub round_id: u64,
    pub message: String,
}

/// POST /admin/reset-txns/{round_id}
/// Reset transaction fetch status so txns can be re-fetched
pub async fn reset_txns_status(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<ResetTxnsResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Resetting transaction status for round {}", round_id);
    
    // Also delete any existing raw transactions from ClickHouse
    if let Err(e) = state.clickhouse.delete_raw_transactions_for_round(round_id).await {
        tracing::warn!("Failed to delete raw transactions for round {}: {}", round_id, e);
    }
    
    // Reset PostgreSQL status - keeps meta but resets txns, reconstruct, etc.
    let _ = sqlx::query(
        r#"
        UPDATE round_reconstruction_status
        SET transactions_fetched = false,
            transactions_fetched_at = NULL,
            transaction_count = 0,
            reconstructed = false,
            reconstructed_at = NULL,
            deployment_count = 0,
            verified = false,
            verified_at = NULL,
            finalized = false,
            finalized_at = NULL,
            verification_notes = ''
        WHERE round_id = $1
        "#
    )
    .bind(round_id as i64)
    .execute(&state.postgres)
    .await;
    
    Ok(Json(ResetTxnsResponse {
        round_id,
        message: "Transaction status reset. You can now fetch transactions again.".to_string(),
    }))
}

/// DELETE /admin/rounds/{round_id}?delete_round=true&delete_deployments=true
/// Delete round and/or deployments for re-backfill
pub async fn delete_round_data(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
    Query(params): Query<DeleteQuery>,
) -> Result<Json<DeleteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let delete_round = params.delete_round.unwrap_or(false);
    let delete_deployments = params.delete_deployments.unwrap_or(true);
    
    tracing::info!(
        "Deleting round {} data: round={}, deployments={}",
        round_id, delete_round, delete_deployments
    );
    
    let mut round_deleted = false;
    let mut deployments_deleted = false;
    
    if delete_deployments {
        match state.clickhouse.delete_deployments_for_round(round_id).await {
            Ok(_) => {
                deployments_deleted = true;
                tracing::info!("Deleted deployments for round {}", round_id);
            }
            Err(e) => {
                tracing::error!("Failed to delete deployments for round {}: {}", round_id, e);
            }
        }
    }
    
    if delete_round {
        match state.clickhouse.delete_round(round_id).await {
            Ok(_) => {
                round_deleted = true;
                tracing::info!("Deleted round {}", round_id);
            }
            Err(e) => {
                tracing::error!("Failed to delete round {}: {}", round_id, e);
            }
        }
    }
    
    // Reset reconstruction status in PostgreSQL
    reset_round_reconstruction_status(&state.postgres, round_id, delete_round).await;
    
    Ok(Json(DeleteResponse {
        round_id,
        round_deleted,
        deployments_deleted,
        message: format!(
            "Deleted: round={}, deployments={}",
            round_deleted, deployments_deleted
        ),
    }))
}

/// POST /admin/rounds/bulk-delete
/// Delete multiple rounds and/or their deployments
pub async fn bulk_delete_rounds(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkDeleteRequest>,
) -> Result<Json<BulkDeleteResponse>, (StatusCode, Json<ErrorResponse>)> {
    let mut deleted_count = 0u32;
    let mut failed_count = 0u32;
    
    tracing::info!(
        "Bulk deleting {} rounds: rounds={}, deployments={}",
        req.round_ids.len(), req.delete_rounds, req.delete_deployments
    );
    
    for round_id in &req.round_ids {
        let mut success = true;
        
        if req.delete_deployments {
            if let Err(e) = state.clickhouse.delete_deployments_for_round(*round_id).await {
                tracing::error!("Failed to delete deployments for round {}: {}", round_id, e);
                success = false;
            }
        }
        
        if req.delete_rounds {
            if let Err(e) = state.clickhouse.delete_round(*round_id).await {
                tracing::error!("Failed to delete round {}: {}", round_id, e);
                success = false;
            }
        }
        
        // Reset PostgreSQL status
        reset_round_reconstruction_status(&state.postgres, *round_id, req.delete_rounds).await;
        
        if success {
            deleted_count += 1;
        } else {
            failed_count += 1;
        }
    }
    
    Ok(Json(BulkDeleteResponse {
        deleted_count,
        failed_count,
        message: format!(
            "Deleted {} rounds, {} failed",
            deleted_count, failed_count
        ),
    }))
}

/// GET /admin/rounds/{round_id}/status
/// Check if round exists and has valid deployment data
pub async fn get_round_data_status(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<RoundDataStatus>, (StatusCode, Json<ErrorResponse>)> {
    let round_exists = state.clickhouse.round_exists(round_id).await.unwrap_or(false);
    let (deployment_count, deployments_sum) = state.clickhouse
        .get_deployment_stats_for_round(round_id)
        .await
        .unwrap_or((0, 0));
    
    // Get round's total_deployed for validation
    let total_deployed = if round_exists {
        state.clickhouse.get_round_by_id(round_id).await
            .ok()
            .flatten()
            .map(|r| r.total_deployed)
            .unwrap_or(0)
    } else {
        0
    };
    
    let discrepancy = total_deployed as i64 - deployments_sum as i64;
    let is_valid = round_exists && deployment_count > 0 && discrepancy == 0;
    
    Ok(Json(RoundDataStatus {
        round_id,
        round_exists,
        deployment_count,
        deployments_sum,
        total_deployed,
        is_valid,
        discrepancy,
    }))
}

/// GET /admin/rounds/data - Get rounds with deployment counts for admin verification
/// 
/// Query params:
/// - `limit`: Number of rounds per page (default 50, max 200)
/// - `page`: Page number (1-based) for offset pagination
/// - `before`: Cursor - get rounds before this round_id (more efficient for deep pagination)
/// - `filter_mode`: "all" (default), "missing_deployments", "invalid_deployments"
pub async fn get_rounds_with_data(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsWithDataQuery>,
) -> Result<Json<RoundsWithDataResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).min(200);
    let filter_mode = params.filter_mode.as_deref().unwrap_or("all");
    
    // Round ID filters
    let round_id_gte = params.round_id_gte;
    let round_id_lte = params.round_id_lte;
    
    // Determine pagination mode
    let (before_round_id, offset) = if let Some(before) = params.before {
        // Cursor-based pagination
        (Some(before), None)
    } else if let Some(page) = params.page {
        // Offset-based pagination (page 1 = offset 0)
        let page = page.max(1);
        (None, Some((page - 1) * limit))
    } else {
        // No pagination, get latest
        (None, None)
    };
    
    // Handle different filter modes with server-side queries
    let (enriched, has_more) = match filter_mode {
        "missing_deployments" => {
            // Server-side query for rounds with no deployments
            let (rounds, has_more) = state.clickhouse
                .get_rounds_with_missing_deployments(round_id_gte, round_id_lte, before_round_id, offset, limit)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: format!("ClickHouse error: {}", e) }),
                    )
                })?;
            
            let enriched: Vec<RoundWithData> = rounds.into_iter().map(|r| {
                RoundWithData {
                    round_id: r.round_id,
                    start_slot: r.start_slot,
                    end_slot: r.end_slot,
                    winning_square: r.winning_square,
                    top_miner: r.top_miner,
                    total_deployed: r.total_deployed,
                    total_winnings: r.total_winnings,
                    unique_miners: r.unique_miners,
                    motherlode: r.motherlode,
                    deployment_count: 0,
                    source: r.source,
                    deployments_sum: 0,
                    is_valid: false,
                    discrepancy: r.total_deployed as i64,
                }
            }).collect();
            
            (enriched, has_more)
        },
        "invalid_deployments" => {
            // Server-side query for rounds with mismatched deployment totals
            let (rounds, has_more) = state.clickhouse
                .get_rounds_with_invalid_deployments(round_id_gte, round_id_lte, before_round_id, offset, limit)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: format!("ClickHouse error: {}", e) }),
                    )
                })?;
            
            let enriched: Vec<RoundWithData> = rounds.into_iter().map(|r| {
                let discrepancy = r.total_deployed as i64 - r.deployments_sum as i64;
                let is_valid = r.deployment_count > 0 && discrepancy == 0;
                RoundWithData {
                    round_id: r.round_id,
                    start_slot: r.start_slot,
                    end_slot: r.end_slot,
                    winning_square: r.winning_square,
                    top_miner: r.top_miner,
                    total_deployed: r.total_deployed,
                    total_winnings: r.total_winnings,
                    unique_miners: r.unique_miners,
                    motherlode: r.motherlode,
                    deployment_count: r.deployment_count,
                    source: r.source,
                    deployments_sum: r.deployments_sum,
                    is_valid,
                    discrepancy,
                }
            }).collect();
            
            (enriched, has_more)
        },
        _ => {
            // Default: get all rounds and enrich
            let (rounds, has_more) = state.clickhouse
                .get_rounds_filtered_for_admin(round_id_gte, round_id_lte, before_round_id, offset, limit)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse { error: format!("ClickHouse error: {}", e) }),
                    )
                })?;
            
            // Enrich with deployment counts and validation
            let mut enriched = Vec::new();
            for r in rounds {
                // Get both count and sum in one query for efficiency
                let (deployment_count, deployments_sum) = state.clickhouse
                    .get_deployment_stats_for_round(r.round_id)
                    .await
                    .unwrap_or((0, 0));
                
                // Calculate validation: deployments_sum should match total_deployed
                let discrepancy = r.total_deployed as i64 - deployments_sum as i64;
                let is_valid = deployment_count > 0 && discrepancy == 0;
                
                enriched.push(RoundWithData {
                    round_id: r.round_id,
                    start_slot: r.start_slot,
                    end_slot: r.end_slot,
                    winning_square: r.winning_square,
                    top_miner: r.top_miner,
                    total_deployed: r.total_deployed,
                    total_winnings: r.total_winnings,
                    unique_miners: r.unique_miners,
                    motherlode: r.motherlode,
                    deployment_count,
                    source: r.source,
                    deployments_sum,
                    is_valid,
                    discrepancy,
                });
            }
            
            (enriched, has_more)
        }
    };
    
    // Get next cursor (last round_id in results)
    let next_cursor = if has_more {
        enriched.last().map(|r| r.round_id)
    } else {
        None
    };
    
    let total = enriched.len() as u32;
    
    Ok(Json(RoundsWithDataResponse {
        rounds: enriched,
        total,
        has_more,
        next_cursor,
        page: params.page,
    }))
}

/// GET /admin/rounds/missing - Get missing round IDs (gaps in stored data)
pub async fn get_missing_rounds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsWithDataQuery>,
) -> Result<Json<MissingRoundsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).min(200);
    
    // Round ID filters
    let round_id_gte = params.round_id_gte;
    let round_id_lte = params.round_id_lte;
    
    // Determine pagination mode
    let offset = if let Some(page) = params.page {
        let page = page.max(1);
        Some((page - 1) * limit)
    } else {
        None
    };
    
    let (missing_ids, has_more, min_stored, max_stored) = state.clickhouse
        .get_missing_round_ids(round_id_gte, round_id_lte, offset, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    let total = missing_ids.len() as u32;
    
    Ok(Json(MissingRoundsResponse {
        missing_round_ids: missing_ids,
        total,
        has_more,
        next_cursor: None, // Missing rounds use offset pagination only
        page: params.page,
        min_stored_round: min_stored,
        max_stored_round: max_stored,
    }))
}

/// GET /admin/rounds/stats - Get counts for each filter category
#[derive(Debug, Serialize)]
pub struct RoundStatsResponse {
    pub total_rounds: u64,
    pub missing_deployments_count: u64,
    pub invalid_deployments_count: u64,
    pub missing_rounds_count: u64,
    pub min_stored_round: u64,
    pub max_stored_round: u64,
}

pub async fn get_round_stats(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsWithDataQuery>,
) -> Result<Json<RoundStatsResponse>, (StatusCode, Json<ErrorResponse>)> {
    let round_id_gte = params.round_id_gte;
    let round_id_lte = params.round_id_lte;
    
    // Get all counts
    let total_rounds = state.clickhouse
        .get_rounds_count_filtered(round_id_gte, round_id_lte)
        .await
        .unwrap_or(0);
    
    let missing_deployments_count = state.clickhouse
        .get_rounds_with_missing_deployments_count(round_id_gte, round_id_lte)
        .await
        .unwrap_or(0);
    
    let invalid_deployments_count = state.clickhouse
        .get_rounds_with_invalid_deployments_count(round_id_gte, round_id_lte)
        .await
        .unwrap_or(0);
    
    let missing_rounds_count = state.clickhouse
        .get_missing_round_ids_count(round_id_gte, round_id_lte)
        .await
        .unwrap_or(0);
    
    // Get min/max round IDs
    let (min_stored, max_stored) = state.clickhouse
        .get_missing_round_ids(None, None, None, 1) // Just to get the range
        .await
        .map(|(_, _, min, max)| (min, max))
        .unwrap_or((0, 0));
    
    Ok(Json(RoundStatsResponse {
        total_rounds,
        missing_deployments_count,
        invalid_deployments_count,
        missing_rounds_count,
        min_stored_round: min_stored,
        max_stored_round: max_stored,
    }))
}

// ============================================================================
// Database Helpers
// ============================================================================

async fn check_round_exists(
    clickhouse: &crate::clickhouse::ClickHouseClient,
    round_id: u64,
) -> bool {
    // Check if round exists in ClickHouse
    clickhouse.round_exists(round_id).await.unwrap_or(false)
}

async fn get_pending_rounds_from_db(pool: &PgPool) -> Result<Vec<RoundStatus>, sqlx::Error> {
    let rows = sqlx::query_as::<_, RoundStatus>(
        r#"
        SELECT 
            round_id,
            meta_fetched,
            transactions_fetched,
            reconstructed,
            verified,
            finalized,
            transaction_count,
            deployment_count,
            verification_notes
        FROM round_reconstruction_status
        WHERE finalized = false
        ORDER BY round_id DESC
        LIMIT 100
        "#
    )
    .fetch_all(pool)
    .await?;
    
    Ok(rows)
}

async fn get_round_status(pool: &PgPool, round_id: i64) -> Result<Option<RoundStatus>, sqlx::Error> {
    let row = sqlx::query_as::<_, RoundStatus>(
        r#"
        SELECT 
            round_id,
            meta_fetched,
            transactions_fetched,
            reconstructed,
            verified,
            finalized,
            transaction_count,
            deployment_count,
            verification_notes
        FROM round_reconstruction_status
        WHERE round_id = $1
        "#
    )
    .bind(round_id)
    .fetch_optional(pool)
    .await?;
    
    Ok(row)
}

async fn update_round_status_meta_fetched(pool: &PgPool, round_id: u64) {
    let _ = sqlx::query(
        r#"
        INSERT INTO round_reconstruction_status (round_id, meta_fetched, meta_fetched_at)
        VALUES ($1, true, NOW())
        ON CONFLICT (round_id) DO UPDATE SET 
            meta_fetched = true,
            meta_fetched_at = NOW()
        "#
    )
    .bind(round_id as i64)
    .execute(pool)
    .await;
}

async fn update_round_status_txns_fetched(pool: &PgPool, round_id: u64, tx_count: i32) {
    let _ = sqlx::query(
        r#"
        UPDATE round_reconstruction_status
        SET transactions_fetched = true,
            transactions_fetched_at = NOW(),
            transaction_count = $2
        WHERE round_id = $1
        "#
    )
    .bind(round_id as i64)
    .bind(tx_count)
    .execute(pool)
    .await;
}

async fn update_round_status_verified(pool: &PgPool, round_id: u64, notes: &str) {
    let _ = sqlx::query(
        r#"
        UPDATE round_reconstruction_status
        SET verified = true,
            verified_at = NOW(),
            verification_notes = $2
        WHERE round_id = $1
        "#
    )
    .bind(round_id as i64)
    .bind(notes)
    .execute(pool)
    .await;
}

async fn update_round_status_reconstructed(pool: &PgPool, round_id: u64, deployment_count: i32) {
    let _ = sqlx::query(
        r#"
        INSERT INTO round_reconstruction_status (round_id, meta_fetched, transactions_fetched, reconstructed, deployment_count, reconstructed_at)
        VALUES ($1, true, true, true, $2, NOW())
        ON CONFLICT (round_id) DO UPDATE SET 
            transactions_fetched = true,
            reconstructed = true,
            deployment_count = $2,
            reconstructed_at = NOW()
        "#
    )
    .bind(round_id as i64)
    .bind(deployment_count)
    .execute(pool)
    .await;
}

/// Add a round to the backfill workflow with meta_fetched=true
/// (Used for rounds that already have metadata from live capture)
async fn add_round_to_backfill_workflow(pool: &PgPool, round_id: u64) {
    let _ = sqlx::query(
        r#"
        INSERT INTO round_reconstruction_status (round_id, meta_fetched, meta_fetched_at)
        VALUES ($1, true, NOW())
        ON CONFLICT (round_id) DO NOTHING
        "#
    )
    .bind(round_id as i64)
    .execute(pool)
    .await;
}

async fn update_round_status_finalized(pool: &PgPool, round_id: u64) {
    let _ = sqlx::query(
        r#"
        UPDATE round_reconstruction_status
        SET finalized = true,
            finalized_at = NOW()
        WHERE round_id = $1
        "#
    )
    .bind(round_id as i64)
    .execute(pool)
    .await;
}

async fn reset_round_reconstruction_status(pool: &PgPool, round_id: u64, reset_meta: bool) {
    if reset_meta {
        // Delete the row entirely if resetting metadata too
        let _ = sqlx::query(
            r#"DELETE FROM round_reconstruction_status WHERE round_id = $1"#
        )
        .bind(round_id as i64)
        .execute(pool)
        .await;
    } else {
        // Just reset the reconstruction/verification/finalization flags
        let _ = sqlx::query(
            r#"
            UPDATE round_reconstruction_status
            SET transactions_fetched = false,
                transactions_fetched_at = NULL,
                reconstructed = false,
                reconstructed_at = NULL,
                verified = false,
                verified_at = NULL,
                finalized = false,
                finalized_at = NULL,
                transaction_count = 0,
                deployment_count = 0,
                verification_notes = ''
            WHERE round_id = $1
            "#
        )
        .bind(round_id as i64)
        .execute(pool)
        .await;
    }
}

