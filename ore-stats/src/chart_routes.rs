//! Chart Data API Routes
//!
//! Endpoints for time series chart data. All queries against ClickHouse
//! pre-aggregated tables for fast response times.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::clickhouse::{
    CostPerOreDailyRow, CostPerOreDirectRow, InflationDailyRow, InflationDirectRow,
    InflationHourlyRow, MinerActivityDailyRow, MintDailyRow, MintDirectRow, MintHourlyRow,
    RoundDirectRow, RoundsDailyRow, RoundsHourlyRow, TreasuryDirectRow, TreasuryHourlyRow,
};

// ============================================================================
// Query Parameters
// ============================================================================

/// Time range query for hourly data.
#[derive(Debug, Deserialize)]
pub struct HourlyQuery {
    /// Number of hours to fetch (default: 24, max: 720 = 30 days).
    pub hours: Option<u32>,
}

/// Time range query for daily data.
#[derive(Debug, Deserialize)]
pub struct DailyQuery {
    /// Number of days to fetch (default: 30, max: 365).
    pub days: Option<u32>,
}

/// Round range query for direct data.
#[derive(Debug, Deserialize)]
pub struct DirectQuery {
    /// Start round ID (inclusive). If omitted, starts from latest - limit.
    pub start: Option<u64>,
    /// End round ID (inclusive). If omitted or "live", uses latest round.
    pub end: Option<String>,
    /// Maximum number of data points to return (default: 1000, max: 5000).
    pub limit: Option<u32>,
}

impl DirectQuery {
    /// Parse end parameter - returns None for "live" or missing.
    pub fn end_round(&self) -> Option<u64> {
        self.end.as_ref().and_then(|s| {
            if s == "live" {
                None
            } else {
                s.parse().ok()
            }
        })
    }
}

// ============================================================================
// Response Types (for JSON serialization to frontend)
// ============================================================================

/// Rounds hourly response with Unix timestamps.
#[derive(Debug, Serialize)]
pub struct RoundsHourlyResponse {
    pub hour: u32,
    pub rounds_count: u32,
    pub total_deployments: u64,
    pub unique_miners: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub motherlode_hits: u32,
    pub total_motherlode: u64,
}

impl From<RoundsHourlyRow> for RoundsHourlyResponse {
    fn from(row: RoundsHourlyRow) -> Self {
        Self {
            hour: row.hour,
            rounds_count: row.rounds_count,
            total_deployments: row.total_deployments,
            unique_miners: row.unique_miners,
            total_deployed: row.total_deployed,
            total_vaulted: row.total_vaulted,
            total_winnings: row.total_winnings,
            motherlode_hits: row.motherlode_hits,
            total_motherlode: row.total_motherlode,
        }
    }
}

/// Rounds daily response with Unix timestamp for the day start.
#[derive(Debug, Serialize)]
pub struct RoundsDailyResponse {
    /// Unix timestamp for midnight UTC of this day.
    pub day: u32,
    pub rounds_count: u32,
    pub total_deployments: u64,
    pub unique_miners: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub motherlode_hits: u32,
    pub total_motherlode: u64,
}

impl From<RoundsDailyRow> for RoundsDailyResponse {
    fn from(row: RoundsDailyRow) -> Self {
        // Convert days since epoch to Unix timestamp
        let day_timestamp = (row.day as u32) * 86400;
        Self {
            day: day_timestamp,
            rounds_count: row.rounds_count,
            total_deployments: row.total_deployments,
            unique_miners: row.unique_miners,
            total_deployed: row.total_deployed,
            total_vaulted: row.total_vaulted,
            total_winnings: row.total_winnings,
            motherlode_hits: row.motherlode_hits,
            total_motherlode: row.total_motherlode,
        }
    }
}

/// Treasury hourly response.
#[derive(Debug, Serialize)]
pub struct TreasuryHourlyResponse {
    pub hour: u32,
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
}

impl From<TreasuryHourlyRow> for TreasuryHourlyResponse {
    fn from(row: TreasuryHourlyRow) -> Self {
        Self {
            hour: row.hour,
            balance: row.balance,
            motherlode: row.motherlode,
            total_staked: row.total_staked,
            total_unclaimed: row.total_unclaimed,
            total_refined: row.total_refined,
        }
    }
}

/// Mint supply hourly response.
#[derive(Debug, Serialize)]
pub struct MintHourlyResponse {
    pub hour: u32,
    pub supply: u64,
    pub supply_change_total: i64,
    pub round_count: u32,
}

impl From<MintHourlyRow> for MintHourlyResponse {
    fn from(row: MintHourlyRow) -> Self {
        Self {
            hour: row.hour,
            supply: row.supply,
            supply_change_total: row.supply_change_total,
            round_count: row.round_count,
        }
    }
}

/// Mint supply daily response.
#[derive(Debug, Serialize)]
pub struct MintDailyResponse {
    pub day: u32,
    pub supply: u64,
    pub supply_start: u64,
    pub supply_change_total: i64,
    pub round_count: u32,
}

impl From<MintDailyRow> for MintDailyResponse {
    fn from(row: MintDailyRow) -> Self {
        let day_timestamp = (row.day as u32) * 86400;
        Self {
            day: day_timestamp,
            supply: row.supply,
            supply_start: row.supply_start,
            supply_change_total: row.supply_change_total,
            round_count: row.round_count,
        }
    }
}

/// Market inflation hourly response.
#[derive(Debug, Serialize)]
pub struct InflationHourlyResponse {
    pub hour: u32,
    pub supply_end: u64,
    pub supply_change_total: i64,
    pub unclaimed_end: u64,
    pub unclaimed_change_total: i64,
    pub circulating_end: u64,
    pub market_inflation_total: i64,
    pub rounds_count: u32,
}

impl From<InflationHourlyRow> for InflationHourlyResponse {
    fn from(row: InflationHourlyRow) -> Self {
        Self {
            hour: row.hour,
            supply_end: row.supply_end,
            supply_change_total: row.supply_change_total,
            unclaimed_end: row.unclaimed_end,
            unclaimed_change_total: row.unclaimed_change_total,
            circulating_end: row.circulating_end,
            market_inflation_total: row.market_inflation_total,
            rounds_count: row.rounds_count,
        }
    }
}

/// Market inflation daily response.
#[derive(Debug, Serialize)]
pub struct InflationDailyResponse {
    pub day: u32,
    pub supply_start: u64,
    pub supply_end: u64,
    pub supply_change_total: i64,
    pub circulating_start: u64,
    pub circulating_end: u64,
    pub market_inflation_total: i64,
    pub rounds_count: u32,
}

impl From<InflationDailyRow> for InflationDailyResponse {
    fn from(row: InflationDailyRow) -> Self {
        let day_timestamp = (row.day as u32) * 86400;
        Self {
            day: day_timestamp,
            supply_start: row.supply_start,
            supply_end: row.supply_end,
            supply_change_total: row.supply_change_total,
            circulating_start: row.circulating_start,
            circulating_end: row.circulating_end,
            market_inflation_total: row.market_inflation_total,
            rounds_count: row.rounds_count,
        }
    }
}

/// Cost per ORE daily response.
#[derive(Debug, Serialize)]
pub struct CostPerOreDailyResponse {
    pub day: u32,
    pub rounds_count: u32,
    pub total_vaulted: u64,
    pub ore_minted_total: u64,
    pub cost_per_ore_lamports: u64,
}

impl From<CostPerOreDailyRow> for CostPerOreDailyResponse {
    fn from(row: CostPerOreDailyRow) -> Self {
        let day_timestamp = (row.day as u32) * 86400;
        Self {
            day: day_timestamp,
            rounds_count: row.rounds_count,
            total_vaulted: row.total_vaulted,
            ore_minted_total: row.ore_minted_total,
            cost_per_ore_lamports: row.cost_per_ore_lamports,
        }
    }
}

/// Miner activity daily response.
#[derive(Debug, Serialize)]
pub struct MinerActivityDailyResponse {
    pub day: u32,
    pub active_miners: u64,
    pub total_deployments: u64,
    pub total_deployed: u64,
    pub total_won: u64,
}

impl From<MinerActivityDailyRow> for MinerActivityDailyResponse {
    fn from(row: MinerActivityDailyRow) -> Self {
        let day_timestamp = (row.day as u32) * 86400;
        Self {
            day: day_timestamp,
            active_miners: row.active_miners,
            total_deployments: row.total_deployments,
            total_deployed: row.total_deployed,
            total_won: row.total_won,
        }
    }
}

// ============================================================================
// Direct/Round-based Response Types
// ============================================================================

/// Direct round data (from rounds table).
#[derive(Debug, Serialize)]
pub struct RoundDirectResponse {
    pub round_id: u64,
    pub created_at: i64,  // Unix timestamp ms
    pub total_deployments: u32,
    pub unique_miners: u32,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub motherlode_hit: bool,
    pub motherlode: u64,
}

/// Direct treasury snapshot data.
#[derive(Debug, Serialize)]
pub struct TreasuryDirectResponse {
    pub round_id: u64,
    pub created_at: i64,  // Unix timestamp ms
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
}

/// Direct mint snapshot data.
#[derive(Debug, Serialize)]
pub struct MintDirectResponse {
    pub round_id: u64,
    pub created_at: i64,  // Unix timestamp ms
    pub supply: u64,
    pub supply_change: i64,
}

/// Direct inflation data (per round).
#[derive(Debug, Serialize)]
pub struct InflationDirectResponse {
    pub round_id: u64,
    pub created_at: i64,  // Unix timestamp ms
    pub supply: u64,
    pub supply_change: i64,
    pub unclaimed: u64,
    pub circulating: u64,
    pub market_inflation: i64,
}

/// Direct cost per ORE (calculated per round).
#[derive(Debug, Serialize)]
pub struct CostPerOreDirectResponse {
    pub round_id: u64,
    pub created_at: i64,  // Unix timestamp ms
    pub total_vaulted: u64,
    pub ore_minted: u64,  // 1 ORE base + motherlode
    pub cost_per_ore_lamports: u64,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /charts/rounds/hourly
/// Returns hourly round statistics.
pub async fn get_rounds_hourly(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HourlyQuery>,
) -> Result<Json<Vec<RoundsHourlyResponse>>, StatusCode> {
    let hours = query.hours.unwrap_or(24).min(720);

    match state.clickhouse.get_rounds_hourly(hours).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get rounds hourly: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/rounds/daily
/// Returns daily round statistics.
pub async fn get_rounds_daily(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DailyQuery>,
) -> Result<Json<Vec<RoundsDailyResponse>>, StatusCode> {
    let days = query.days.unwrap_or(30).min(365);

    match state.clickhouse.get_rounds_daily(days).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get rounds daily: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/treasury/hourly
/// Returns hourly treasury snapshots (latest state per hour).
pub async fn get_treasury_hourly(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HourlyQuery>,
) -> Result<Json<Vec<TreasuryHourlyResponse>>, StatusCode> {
    let hours = query.hours.unwrap_or(24).min(720);

    match state.clickhouse.get_treasury_hourly(hours).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get treasury hourly: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/mint/hourly
/// Returns hourly mint supply data.
pub async fn get_mint_hourly(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HourlyQuery>,
) -> Result<Json<Vec<MintHourlyResponse>>, StatusCode> {
    let hours = query.hours.unwrap_or(24).min(720);

    match state.clickhouse.get_mint_hourly(hours).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get mint hourly: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/mint/daily
/// Returns daily mint supply data.
pub async fn get_mint_daily(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DailyQuery>,
) -> Result<Json<Vec<MintDailyResponse>>, StatusCode> {
    let days = query.days.unwrap_or(30).min(365);

    match state.clickhouse.get_mint_daily(days).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get mint daily: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/inflation/hourly
/// Returns hourly market inflation data.
pub async fn get_inflation_hourly(
    State(state): State<Arc<AppState>>,
    Query(query): Query<HourlyQuery>,
) -> Result<Json<Vec<InflationHourlyResponse>>, StatusCode> {
    let hours = query.hours.unwrap_or(24).min(720);

    match state.clickhouse.get_inflation_hourly(hours).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get inflation hourly: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/inflation/daily
/// Returns daily market inflation data.
pub async fn get_inflation_daily(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DailyQuery>,
) -> Result<Json<Vec<InflationDailyResponse>>, StatusCode> {
    let days = query.days.unwrap_or(30).min(365);

    match state.clickhouse.get_inflation_daily(days).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get inflation daily: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/cost-per-ore/daily
/// Returns daily cost per ORE data.
pub async fn get_cost_per_ore_daily(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DailyQuery>,
) -> Result<Json<Vec<CostPerOreDailyResponse>>, StatusCode> {
    let days = query.days.unwrap_or(30).min(365);

    match state.clickhouse.get_cost_per_ore_daily(days).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get cost per ore daily: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/miners/daily
/// Returns daily miner activity data.
pub async fn get_miners_daily(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DailyQuery>,
) -> Result<Json<Vec<MinerActivityDailyResponse>>, StatusCode> {
    let days = query.days.unwrap_or(30).min(365);

    match state.clickhouse.get_miner_activity_daily(days).await {
        Ok(rows) => Ok(Json(rows.into_iter().map(Into::into).collect())),
        Err(e) => {
            tracing::error!("Failed to get miner activity daily: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Direct/Round-based Route Handlers
// ============================================================================

/// Metadata about the latest round (for "live" end value).
#[derive(Debug, Serialize)]
pub struct DirectMetadata {
    pub latest_round_id: u64,
}

/// Wrapper response for direct data with metadata.
#[derive(Debug, Serialize)]
pub struct DirectResponse<T> {
    pub meta: DirectMetadata,
    pub data: Vec<T>,
}

/// GET /charts/rounds/direct
/// Returns direct round data by round range.
pub async fn get_rounds_direct(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectQuery>,
) -> Result<Json<DirectResponse<RoundDirectResponse>>, StatusCode> {
    let limit = query.limit.unwrap_or(1000).min(5000);
    let end_round = query.end_round();
    
    // Get latest round ID for metadata
    let latest = state.clickhouse.get_latest_round_id().await
        .map_err(|e| {
            tracing::error!("Failed to get latest round: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(0);

    match state.clickhouse.get_rounds_direct(query.start, end_round, limit).await {
        Ok(rows) => {
            let data: Vec<RoundDirectResponse> = rows.into_iter().map(|r| RoundDirectResponse {
                round_id: r.round_id,
                created_at: r.created_at,
                total_deployments: r.total_deployments,
                unique_miners: r.unique_miners,
                total_deployed: r.total_deployed,
                total_vaulted: r.total_vaulted,
                total_winnings: r.total_winnings,
                motherlode_hit: r.motherlode_hit != 0,
                motherlode: r.motherlode,
            }).collect();
            Ok(Json(DirectResponse { meta: DirectMetadata { latest_round_id: latest }, data }))
        },
        Err(e) => {
            tracing::error!("Failed to get rounds direct: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/treasury/direct
/// Returns direct treasury snapshots by round range.
pub async fn get_treasury_direct(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectQuery>,
) -> Result<Json<DirectResponse<TreasuryDirectResponse>>, StatusCode> {
    let limit = query.limit.unwrap_or(1000).min(5000);
    let end_round = query.end_round();
    
    let latest = state.clickhouse.get_latest_round_id().await
        .map_err(|e| {
            tracing::error!("Failed to get latest round: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(0);

    match state.clickhouse.get_treasury_direct(query.start, end_round, limit).await {
        Ok(rows) => {
            let data: Vec<TreasuryDirectResponse> = rows.into_iter().map(|r| TreasuryDirectResponse {
                round_id: r.round_id,
                created_at: r.created_at,
                balance: r.balance,
                motherlode: r.motherlode,
                total_staked: r.total_staked,
                total_unclaimed: r.total_unclaimed,
                total_refined: r.total_refined,
            }).collect();
            Ok(Json(DirectResponse { meta: DirectMetadata { latest_round_id: latest }, data }))
        },
        Err(e) => {
            tracing::error!("Failed to get treasury direct: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/mint/direct
/// Returns direct mint snapshots by round range.
pub async fn get_mint_direct(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectQuery>,
) -> Result<Json<DirectResponse<MintDirectResponse>>, StatusCode> {
    let limit = query.limit.unwrap_or(1000).min(5000);
    let end_round = query.end_round();
    
    let latest = state.clickhouse.get_latest_round_id().await
        .map_err(|e| {
            tracing::error!("Failed to get latest round: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(0);

    match state.clickhouse.get_mint_direct(query.start, end_round, limit).await {
        Ok(rows) => {
            let data: Vec<MintDirectResponse> = rows.into_iter().map(|r| MintDirectResponse {
                round_id: r.round_id,
                created_at: r.created_at,
                supply: r.supply,
                supply_change: r.supply_change,
            }).collect();
            Ok(Json(DirectResponse { meta: DirectMetadata { latest_round_id: latest }, data }))
        },
        Err(e) => {
            tracing::error!("Failed to get mint direct: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/inflation/direct
/// Returns direct inflation data by round range.
pub async fn get_inflation_direct(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectQuery>,
) -> Result<Json<DirectResponse<InflationDirectResponse>>, StatusCode> {
    let limit = query.limit.unwrap_or(1000).min(5000);
    let end_round = query.end_round();
    
    let latest = state.clickhouse.get_latest_round_id().await
        .map_err(|e| {
            tracing::error!("Failed to get latest round: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(0);

    match state.clickhouse.get_inflation_direct(query.start, end_round, limit).await {
        Ok(rows) => {
            let data: Vec<InflationDirectResponse> = rows.into_iter().map(|r| InflationDirectResponse {
                round_id: r.round_id,
                created_at: r.created_at,
                supply: r.supply,
                supply_change: r.supply_change,
                unclaimed: r.unclaimed,
                circulating: r.circulating,
                market_inflation: r.market_inflation,
            }).collect();
            Ok(Json(DirectResponse { meta: DirectMetadata { latest_round_id: latest }, data }))
        },
        Err(e) => {
            tracing::error!("Failed to get inflation direct: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// GET /charts/cost-per-ore/direct
/// Returns direct cost per ORE by round range.
pub async fn get_cost_per_ore_direct(
    State(state): State<Arc<AppState>>,
    Query(query): Query<DirectQuery>,
) -> Result<Json<DirectResponse<CostPerOreDirectResponse>>, StatusCode> {
    let limit = query.limit.unwrap_or(1000).min(5000);
    let end_round = query.end_round();
    
    let latest = state.clickhouse.get_latest_round_id().await
        .map_err(|e| {
            tracing::error!("Failed to get latest round: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .unwrap_or(0);

    match state.clickhouse.get_cost_per_ore_direct(query.start, end_round, limit).await {
        Ok(rows) => {
            let data: Vec<CostPerOreDirectResponse> = rows.into_iter().map(|r| CostPerOreDirectResponse {
                round_id: r.round_id,
                created_at: r.created_at,
                total_vaulted: r.total_vaulted,
                ore_minted: r.ore_minted,
                cost_per_ore_lamports: r.cost_per_ore_lamports,
            }).collect();
            Ok(Json(DirectResponse { meta: DirectMetadata { latest_round_id: latest }, data }))
        },
        Err(e) => {
            tracing::error!("Failed to get cost per ore direct: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ============================================================================
// Router
// ============================================================================

/// Create the charts router with all endpoints.
pub fn chart_router(_state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Rounds - aggregate
        .route("/rounds/hourly", get(get_rounds_hourly))
        .route("/rounds/daily", get(get_rounds_daily))
        // Rounds - direct (by round_id)
        .route("/rounds/direct", get(get_rounds_direct))
        // Treasury - aggregate
        .route("/treasury/hourly", get(get_treasury_hourly))
        // Treasury - direct (by round_id)
        .route("/treasury/direct", get(get_treasury_direct))
        // Mint supply - aggregate
        .route("/mint/hourly", get(get_mint_hourly))
        .route("/mint/daily", get(get_mint_daily))
        // Mint supply - direct (by round_id)
        .route("/mint/direct", get(get_mint_direct))
        // Market inflation - aggregate
        .route("/inflation/hourly", get(get_inflation_hourly))
        .route("/inflation/daily", get(get_inflation_daily))
        // Market inflation - direct (by round_id)
        .route("/inflation/direct", get(get_inflation_direct))
        // Cost per ORE - aggregate
        .route("/cost-per-ore/daily", get(get_cost_per_ore_daily))
        // Cost per ORE - direct (by round_id)
        .route("/cost-per-ore/direct", get(get_cost_per_ore_direct))
        // Miner activity (aggregate only - no per-round view)
        .route("/miners/daily", get(get_miners_daily))
}
