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
            let exists = check_round_exists(&state.clickhouse, round_id).await;
            if exists {
                rounds_skipped += 1;
                continue;
            }
            
            // Create RoundInsert from external API data
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
        "Backfill complete: {} fetched, {} skipped, stopped_at={:?}",
        rounds_fetched, rounds_skipped, stopped_at
    );
    
    Ok(Json(BackfillRoundsResponse {
        rounds_fetched,
        rounds_skipped,
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

