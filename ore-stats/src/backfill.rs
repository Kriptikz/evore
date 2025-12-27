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
    /// Only show rounds with no deployments
    pub missing_deployments_only: Option<bool>,
    /// Only show rounds where deployments_sum != total_deployed (data integrity issue)
    pub invalid_only: Option<bool>,
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
/// Fetch transactions for a round via Helius v2 API
pub async fn fetch_round_transactions(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<FetchTxnsResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Fetching transactions for round {}", round_id);
    
    // Use Helius to fetch transactions for the round
    let mut helius = state.helius.write().await;
    
    let result = helius.get_transactions_for_round(round_id, None).await;
    
    match result {
        Ok(page) => {
            let tx_count = page.transactions.len() as u32;
            
            // Store raw transactions to ClickHouse
            // TODO: Implement raw transaction storage
            
            // Update PostgreSQL status
            update_round_status_txns_fetched(&state.postgres, round_id, tx_count as i32).await;
            
            Ok(Json(FetchTxnsResponse {
                round_id,
                transactions_fetched: tx_count,
                status: "success".to_string(),
            }))
        }
        Err(e) => {
            tracing::error!("Failed to fetch transactions for round {}: {}", round_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse { error: format!("Helius error: {}", e) }),
            ))
        }
    }
}

/// POST /admin/reconstruct/{round_id}
/// Reconstruct deployments from stored transactions
pub async fn reconstruct_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<ReconstructResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Reconstructing deployments for round {}", round_id);
    
    // TODO: Implement reconstruction logic
    // 1. Load raw_transactions from ClickHouse WHERE round_id = X
    // 2. Parse and replay transactions
    // 3. Build deployment records
    
    // For now, return placeholder
    Ok(Json(ReconstructResponse {
        round_id,
        deployments_reconstructed: 0,
        status: "not_implemented".to_string(),
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
/// - `missing_deployments_only`: Only show rounds with no deployments
/// - `invalid_only`: Only show rounds where deployments don't match total_deployed
pub async fn get_rounds_with_data(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsWithDataQuery>,
) -> Result<Json<RoundsWithDataResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).min(200);
    let missing_only = params.missing_deployments_only.unwrap_or(false);
    let invalid_only = params.invalid_only.unwrap_or(false);
    
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
    
    // Get rounds from ClickHouse with pagination
    let (rounds, has_more) = state.clickhouse
        .get_rounds_paginated(before_round_id, offset, limit)
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
        
        // Filter if missing_deployments_only is set
        if missing_only && deployment_count > 0 {
            continue;
        }
        
        // Filter if invalid_only is set (only show rounds with mismatched totals)
        if invalid_only && (deployment_count == 0 || is_valid) {
            continue;
        }
        
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
    
    // Get next cursor (last round_id in results before filtering)
    // Note: We use the last enriched round since filtering may change what's available
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

