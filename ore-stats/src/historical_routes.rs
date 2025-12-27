//! Historical Data API Routes (Phase 3)
//!
//! Public read endpoints for historical data analysis with extensive filtering,
//! ranges, and pagination. All queries against ClickHouse.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;

// ============================================================================
// Pagination Types
// ============================================================================

/// Cursor-based pagination for sequential browsing
#[derive(Debug, Deserialize)]
pub struct CursorPagination {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Offset-based pagination for random access (leaderboards)
#[derive(Debug, Deserialize)]
pub struct OffsetPagination {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

/// Cursor-based response wrapper
#[derive(Debug, Serialize)]
pub struct CursorResponse<T> {
    pub data: Vec<T>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

/// Offset-based response wrapper
#[derive(Debug, Serialize)]
pub struct OffsetResponse<T> {
    pub data: Vec<T>,
    pub page: u32,
    pub per_page: u32,
    pub total_count: u64,
    pub total_pages: u32,
}

// ============================================================================
// Query Parameters
// ============================================================================

/// Round filters
#[derive(Debug, Deserialize)]
pub struct RoundsQuery {
    // Pagination
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    // Range filters
    pub round_id_gte: Option<u64>,
    pub round_id_lte: Option<u64>,
    pub slot_gte: Option<u64>,
    pub slot_lte: Option<u64>,
    // Boolean filters
    pub motherlode_hit: Option<bool>,
    // Order
    pub order: Option<String>, // "asc" or "desc"
}

/// Deployment filters
#[derive(Debug, Deserialize)]
pub struct DeploymentsQuery {
    // Pagination
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    // Range filters
    pub round_id_gte: Option<u64>,
    pub round_id_lte: Option<u64>,
    // Miner filter
    pub miner: Option<String>,
    // Boolean filters
    pub winner_only: Option<bool>,
    // Amount filters
    pub min_sol_earned: Option<u64>,
    pub max_sol_earned: Option<u64>,
    pub min_ore_earned: Option<u64>,
    pub max_ore_earned: Option<u64>,
}

/// Miner history filters
#[derive(Debug, Deserialize)]
pub struct MinerHistoryQuery {
    // Pagination
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    // Range filters
    pub round_id_gte: Option<u64>,
    pub round_id_lte: Option<u64>,
    // Boolean filters
    pub winner_only: Option<bool>,
}

/// Leaderboard query
#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    // Which metric to rank by
    pub metric: Option<String>, // "net_sol", "sol_earned", "ore_earned", "rounds_won"
    // Time range
    pub round_range: Option<String>, // "all", "last_60", "last_100", "today"
    // Pagination
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

/// Treasury history filters
#[derive(Debug, Deserialize)]
pub struct TreasuryHistoryQuery {
    // Pagination
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    // Range filters
    pub round_id_gte: Option<u64>,
    pub round_id_lte: Option<u64>,
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct HistoricalRound {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub total_deployed: u64,
    pub total_winnings: u64,
    pub unique_miners: u32,
    pub motherlode: u64,
    pub motherlode_hit: bool,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct HistoricalDeployment {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub square_id: u8,
    pub amount: u64,
    pub deployed_slot: u64,
    pub sol_earned: u64,
    pub ore_earned: u64,
    pub is_winner: bool,
    pub is_top_miner: bool,
}

#[derive(Debug, Serialize)]
pub struct MinerStats {
    pub miner_pubkey: String,
    pub total_deployed: u64,
    pub total_sol_earned: u64,
    pub total_ore_earned: u64,
    pub net_sol_change: i64,
    pub rounds_played: u64,
    pub rounds_won: u64,
    pub win_rate: f64,
    pub avg_deployment: u64,
}

#[derive(Debug, Serialize)]
pub struct LeaderboardEntry {
    pub rank: u32,
    pub miner_pubkey: String,
    pub value: i64, // The metric value (could be negative for net_sol)
    pub rounds_played: u64,
}

#[derive(Debug, Serialize)]
pub struct TreasurySnapshot {
    pub round_id: u64,
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Router
// ============================================================================

pub fn historical_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Rounds
        .route("/rounds", get(get_rounds))
        .route("/rounds/{round_id}", get(get_round_detail))
        .route("/rounds/{round_id}/deployments", get(get_round_deployments))
        
        // Deployments (cross-round)
        .route("/deployments", get(get_deployments))
        
        // Miner history
        .route("/miner/{pubkey}/deployments", get(get_miner_deployments))
        .route("/miner/{pubkey}/stats", get(get_miner_stats))
        
        // Leaderboards
        .route("/leaderboard", get(get_leaderboard))
        .route("/leaderboard/sol", get(get_leaderboard_sol))
        .route("/leaderboard/ore", get(get_leaderboard_ore))
        .route("/leaderboard/winners", get(get_leaderboard_winners))
        
        // Treasury history
        .route("/treasury/history", get(get_treasury_history))
        
        .with_state(state)
}

// ============================================================================
// Rounds Handlers
// ============================================================================

/// GET /history/rounds - List rounds with filters
async fn get_rounds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsQuery>,
) -> Result<Json<CursorResponse<HistoricalRound>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).min(100);
    let order_desc = params.order.as_deref() != Some("asc");
    
    // Build query
    let rounds = state.clickhouse
        .get_rounds_filtered(
            params.round_id_gte,
            params.round_id_lte,
            params.slot_gte,
            params.slot_lte,
            params.motherlode_hit,
            params.cursor.as_deref(),
            limit,
            order_desc,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get rounds: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let has_more = rounds.len() as u32 == limit;
    let cursor = rounds.last().map(|r| r.round_id.to_string());
    
    let data: Vec<HistoricalRound> = rounds.into_iter().map(|r| HistoricalRound {
        round_id: r.round_id,
        start_slot: r.start_slot,
        end_slot: r.end_slot,
        winning_square: r.winning_square,
        top_miner: r.top_miner,
        total_deployed: r.total_deployed,
        total_winnings: r.total_winnings,
        unique_miners: r.unique_miners,
        motherlode: r.motherlode,
        motherlode_hit: r.motherlode_hit > 0,
        created_at: format_timestamp(r.created_at),
    }).collect();
    
    Ok(Json(CursorResponse {
        data,
        cursor,
        has_more,
    }))
}

/// GET /history/rounds/{round_id} - Single round details
async fn get_round_detail(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<HistoricalRound>, (StatusCode, Json<ErrorResponse>)> {
    let round = state.clickhouse
        .get_round_by_id(round_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get round {}: {}", round_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?
        .ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Round not found".to_string() }))
        })?;
    
    Ok(Json(HistoricalRound {
        round_id: round.round_id,
        start_slot: round.start_slot,
        end_slot: round.end_slot,
        winning_square: round.winning_square,
        top_miner: round.top_miner,
        total_deployed: round.total_deployed,
        total_winnings: round.total_winnings,
        unique_miners: round.unique_miners,
        motherlode: round.motherlode,
        motherlode_hit: round.motherlode_hit > 0,
        created_at: format_timestamp(round.created_at),
    }))
}

/// GET /history/rounds/{round_id}/deployments - Deployments for a round
async fn get_round_deployments(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
    Query(params): Query<DeploymentsQuery>,
) -> Result<Json<CursorResponse<HistoricalDeployment>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(100).min(500);
    
    let deployments = state.clickhouse
        .get_deployments_for_round_filtered(
            round_id,
            params.miner.as_deref(),
            params.winner_only,
            params.min_sol_earned,
            params.cursor.as_deref(),
            limit,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployments for round {}: {}", round_id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let has_more = deployments.len() as u32 == limit;
    let cursor = deployments.last().map(|d| format!("{}:{}", d.miner_pubkey, d.square_id));
    
    let data: Vec<HistoricalDeployment> = deployments.into_iter().map(|d| HistoricalDeployment {
        round_id: d.round_id,
        miner_pubkey: d.miner_pubkey,
        square_id: d.square_id,
        amount: d.amount,
        deployed_slot: d.deployed_slot,
        sol_earned: d.sol_earned,
        ore_earned: d.ore_earned,
        is_winner: d.is_winner > 0,
        is_top_miner: d.is_top_miner > 0,
    }).collect();
    
    Ok(Json(CursorResponse {
        data,
        cursor,
        has_more,
    }))
}

// ============================================================================
// Deployments Handlers
// ============================================================================

/// GET /history/deployments - Query deployments across rounds
async fn get_deployments(
    State(state): State<Arc<AppState>>,
    Query(params): Query<DeploymentsQuery>,
) -> Result<Json<CursorResponse<HistoricalDeployment>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(100).min(500);
    
    let deployments = state.clickhouse
        .get_deployments_filtered(
            params.round_id_gte,
            params.round_id_lte,
            params.miner.as_deref(),
            params.winner_only,
            params.min_sol_earned,
            params.max_sol_earned,
            params.min_ore_earned,
            params.max_ore_earned,
            params.cursor.as_deref(),
            limit,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get deployments: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let has_more = deployments.len() as u32 == limit;
    let cursor = deployments.last().map(|d| format!("{}:{}:{}", d.round_id, d.miner_pubkey, d.square_id));
    
    let data: Vec<HistoricalDeployment> = deployments.into_iter().map(|d| HistoricalDeployment {
        round_id: d.round_id,
        miner_pubkey: d.miner_pubkey,
        square_id: d.square_id,
        amount: d.amount,
        deployed_slot: d.deployed_slot,
        sol_earned: d.sol_earned,
        ore_earned: d.ore_earned,
        is_winner: d.is_winner > 0,
        is_top_miner: d.is_top_miner > 0,
    }).collect();
    
    Ok(Json(CursorResponse {
        data,
        cursor,
        has_more,
    }))
}

// ============================================================================
// Miner History Handlers
// ============================================================================

/// GET /history/miner/{pubkey}/deployments - Miner's deployment history
async fn get_miner_deployments(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
    Query(params): Query<MinerHistoryQuery>,
) -> Result<Json<CursorResponse<HistoricalDeployment>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(100).min(500);
    
    let deployments = state.clickhouse
        .get_miner_deployments(
            &pubkey,
            params.round_id_gte,
            params.round_id_lte,
            params.winner_only,
            params.cursor.as_deref(),
            limit,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get miner deployments for {}: {}", pubkey, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let has_more = deployments.len() as u32 == limit;
    let cursor = deployments.last().map(|d| format!("{}:{}", d.round_id, d.square_id));
    
    let data: Vec<HistoricalDeployment> = deployments.into_iter().map(|d| HistoricalDeployment {
        round_id: d.round_id,
        miner_pubkey: d.miner_pubkey,
        square_id: d.square_id,
        amount: d.amount,
        deployed_slot: d.deployed_slot,
        sol_earned: d.sol_earned,
        ore_earned: d.ore_earned,
        is_winner: d.is_winner > 0,
        is_top_miner: d.is_top_miner > 0,
    }).collect();
    
    Ok(Json(CursorResponse {
        data,
        cursor,
        has_more,
    }))
}

/// GET /history/miner/{pubkey}/stats - Aggregated miner statistics
async fn get_miner_stats(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<MinerStats>, (StatusCode, Json<ErrorResponse>)> {
    let stats = state.clickhouse
        .get_miner_stats(&pubkey)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get miner stats for {}: {}", pubkey, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?
        .ok_or_else(|| {
            (StatusCode::NOT_FOUND, Json(ErrorResponse { error: "Miner not found in historical data".to_string() }))
        })?;
    
    Ok(Json(stats))
}

// ============================================================================
// Leaderboard Handlers
// ============================================================================

/// GET /history/leaderboard - Default leaderboard (net SOL)
async fn get_leaderboard(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LeaderboardQuery>,
) -> Result<Json<OffsetResponse<LeaderboardEntry>>, (StatusCode, Json<ErrorResponse>)> {
    get_leaderboard_internal(state, params, "net_sol").await
}

/// GET /history/leaderboard/sol - Leaderboard by SOL earned
async fn get_leaderboard_sol(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LeaderboardQuery>,
) -> Result<Json<OffsetResponse<LeaderboardEntry>>, (StatusCode, Json<ErrorResponse>)> {
    get_leaderboard_internal(state, params, "sol_earned").await
}

/// GET /history/leaderboard/ore - Leaderboard by ORE earned
async fn get_leaderboard_ore(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LeaderboardQuery>,
) -> Result<Json<OffsetResponse<LeaderboardEntry>>, (StatusCode, Json<ErrorResponse>)> {
    get_leaderboard_internal(state, params, "ore_earned").await
}

/// GET /history/leaderboard/winners - Leaderboard by rounds won
async fn get_leaderboard_winners(
    State(state): State<Arc<AppState>>,
    Query(params): Query<LeaderboardQuery>,
) -> Result<Json<OffsetResponse<LeaderboardEntry>>, (StatusCode, Json<ErrorResponse>)> {
    get_leaderboard_internal(state, params, "rounds_won").await
}

async fn get_leaderboard_internal(
    state: Arc<AppState>,
    params: LeaderboardQuery,
    default_metric: &str,
) -> Result<Json<OffsetResponse<LeaderboardEntry>>, (StatusCode, Json<ErrorResponse>)> {
    let metric = params.metric.as_deref().unwrap_or(default_metric);
    let round_range = params.round_range.as_deref().unwrap_or("all");
    let page = params.page.unwrap_or(1).max(1);
    let limit = params.limit.unwrap_or(50).min(100);
    let offset = (page - 1) * limit;
    
    let (entries, total_count) = state.clickhouse
        .get_leaderboard(metric, round_range, offset, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get leaderboard: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let total_pages = ((total_count as f64) / (limit as f64)).ceil() as u32;
    
    Ok(Json(OffsetResponse {
        data: entries,
        page,
        per_page: limit,
        total_count,
        total_pages,
    }))
}

// ============================================================================
// Treasury History Handlers
// ============================================================================

/// GET /history/treasury/history - Treasury snapshots over time
async fn get_treasury_history(
    State(state): State<Arc<AppState>>,
    Query(params): Query<TreasuryHistoryQuery>,
) -> Result<Json<CursorResponse<TreasurySnapshot>>, (StatusCode, Json<ErrorResponse>)> {
    let limit = params.limit.unwrap_or(50).min(100);
    
    let snapshots = state.clickhouse
        .get_treasury_history(
            params.round_id_gte,
            params.round_id_lte,
            params.cursor.as_deref(),
            limit,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get treasury history: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse { error: "Database error".to_string() }))
        })?;
    
    let has_more = snapshots.len() as u32 == limit;
    let cursor = snapshots.last().map(|s| s.round_id.to_string());
    
    Ok(Json(CursorResponse {
        data: snapshots,
        cursor,
        has_more,
    }))
}

// ============================================================================
// Helpers
// ============================================================================

fn format_timestamp(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| ts.to_string())
}

