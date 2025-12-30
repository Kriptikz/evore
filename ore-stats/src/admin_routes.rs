//! Admin API routes
//!
//! All routes require authentication via `require_admin_auth` middleware.
//! Login endpoint is the exception - it creates new sessions.

use std::net::IpAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde::{Deserialize, Serialize};

use crate::admin_auth::{
    self, extract_bearer_token, extract_client_ip, is_ip_blacklisted,
    record_failed_attempt, verify_password, AuthError, BlacklistEntry, LoginResponse,
};
use crate::app_state::AppState;

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct AdminMetricsResponse {
    pub uptime_seconds: u64,
    pub current_slot: u64,
    pub miners_cached: usize,
    pub ore_holders_cached: usize,
    pub pending_round_id: u64,
    pub pending_deployments: usize,
}

#[derive(Debug, Serialize)]
pub struct BlacklistResponse {
    pub entries: Vec<BlacklistEntryResponse>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct BlacklistEntryResponse {
    pub ip_address: String,
    pub reason: String,
    pub failed_attempts: i32,
    pub blocked_at: String,
    pub expires_at: Option<String>,
    pub created_by: Option<String>,
}

impl From<BlacklistEntry> for BlacklistEntryResponse {
    fn from(e: BlacklistEntry) -> Self {
        Self {
            ip_address: e.ip_address.to_string(),
            reason: e.reason,
            failed_attempts: e.failed_attempts,
            blocked_at: e.blocked_at.to_rfc3339(),
            expires_at: e.expires_at.map(|t| t.to_rfc3339()),
            created_by: e.created_by,
        }
    }
}

// RPC Metrics Response Types
#[derive(Debug, Serialize)]
pub struct RpcSummaryResponse {
    pub hours: u32,
    pub data: Vec<crate::clickhouse::RpcSummaryRow>,
}

#[derive(Debug, Serialize)]
pub struct RpcProvidersResponse {
    pub hours: u32,
    pub providers: Vec<crate::clickhouse::RpcProviderRow>,
}

#[derive(Debug, Serialize)]
pub struct RpcErrorsResponse {
    pub hours: u32,
    pub limit: u32,
    pub errors: Vec<crate::clickhouse::RpcErrorRow>,
}

#[derive(Debug, Serialize)]
pub struct RpcTimeseriesResponse {
    pub hours: u32,
    pub timeseries: Vec<crate::clickhouse::RpcTimeseriesRow>,
}

#[derive(Debug, Serialize)]
pub struct RpcDailyResponse {
    pub days: u32,
    pub daily: Vec<crate::clickhouse::RpcDailyRow>,
}

// ============================================================================
// Request Types
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct BlacklistRequest {
    pub ip: String,
    pub reason: String,
    #[serde(default)]
    pub permanent: bool,
}

#[derive(Debug, Deserialize)]
pub struct RpcMetricsQuery {
    /// Number of hours to look back (default: 24)
    #[serde(default = "default_hours")]
    pub hours: u32,
    /// Limit for error results (default: 100)
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_hours() -> u32 { 24 }
fn default_limit() -> u32 { 100 }

#[derive(Debug, Deserialize)]
pub struct RpcDailyQuery {
    /// Number of days to look back (default: 7)
    #[serde(default = "default_days")]
    pub days: u32,
}

fn default_days() -> u32 { 7 }

// ============================================================================
// Route Handlers
// ============================================================================

/// POST /admin/login - Authenticate and create session
pub async fn login(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, (StatusCode, Json<AuthError>)> {
    let client_ip = extract_client_ip(&headers)
        .unwrap_or_else(|| "127.0.0.1".parse().unwrap());
    
    // Check if IP is blacklisted
    if is_ip_blacklisted(&state.postgres, client_ip).await.unwrap_or(false) {
        return Err((
            StatusCode::FORBIDDEN,
            Json(AuthError { error: "IP address is blacklisted".to_string() }),
        ));
    }
    
    // Verify password against hash stored in state (hashed at startup)
    if !verify_password(&req.password, &state.admin_password_hash) {
        // Record failed attempt
        let was_blacklisted = record_failed_attempt(&state.postgres, client_ip, "/admin/login")
            .await
            .unwrap_or(false);
        
        if was_blacklisted {
            tracing::warn!("IP {} blacklisted after too many failed login attempts", client_ip);
        }
        
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(AuthError { error: "Invalid password".to_string() }),
        ));
    }
    
    // Create session
    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok());
    
    let (token, expires_at) = admin_auth::create_session(&state.postgres, client_ip, user_agent)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to create session".to_string() }),
            )
        })?;
    
    tracing::info!("Admin login successful from {}", client_ip);
    
    Ok(Json(LoginResponse {
        token,
        expires_at: expires_at.to_rfc3339(),
    }))
}

/// POST /admin/logout - Revoke current session
pub async fn logout(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let token = extract_bearer_token(&headers).ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            Json(AuthError { error: "Missing token".to_string() }),
        )
    })?;
    
    admin_auth::revoke_session(&state.postgres, &token)
        .await
        .map_err(|e| {
            tracing::error!("Failed to revoke session: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to revoke session".to_string() }),
            )
        })?;
    
    Ok(Json(MessageResponse {
        message: "Logged out successfully".to_string(),
    }))
}

/// GET /admin/metrics - Detailed server metrics
pub async fn get_admin_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<AdminMetricsResponse> {
    let miners_cached = state.miners_cache.read().await.len();
    let ore_holders_cached = state.ore_holders_cache.read().await.len();
    let current_slot = *state.slot_cache.read().await;
    let pending_round_id = *state.pending_round_id.read().await;
    let pending_deployments = state.pending_deployments.read().await.len();
    
    Json(AdminMetricsResponse {
        uptime_seconds: state.uptime_seconds(),
        current_slot,
        miners_cached,
        ore_holders_cached,
        pending_round_id,
        pending_deployments,
    })
}

/// GET /admin/blacklist - View all blacklisted IPs
pub async fn get_blacklist(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BlacklistResponse>, (StatusCode, Json<AuthError>)> {
    let entries = admin_auth::get_blacklist(&state.postgres)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get blacklist: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get blacklist".to_string() }),
            )
        })?;
    
    let total = entries.len();
    let entries: Vec<BlacklistEntryResponse> = entries.into_iter().map(Into::into).collect();
    
    Ok(Json(BlacklistResponse { entries, total }))
}

/// POST /admin/blacklist - Add IP to blacklist
pub async fn add_to_blacklist(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BlacklistRequest>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let ip: IpAddr = req.ip.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(AuthError { error: "Invalid IP address".to_string() }),
        )
    })?;
    
    admin_auth::blacklist_ip(&state.postgres, ip, &req.reason, req.permanent)
        .await
        .map_err(|e| {
            tracing::error!("Failed to blacklist IP: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to blacklist IP".to_string() }),
            )
        })?;
    
    tracing::info!("Admin blacklisted IP {}: {}", ip, req.reason);
    
    Ok(Json(MessageResponse {
        message: format!("IP {} added to blacklist", ip),
    }))
}

/// DELETE /admin/blacklist/{ip} - Remove IP from blacklist
pub async fn remove_from_blacklist(
    State(state): State<Arc<AppState>>,
    Path(ip_str): Path<String>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let ip: IpAddr = ip_str.parse().map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(AuthError { error: "Invalid IP address".to_string() }),
        )
    })?;
    
    let removed = admin_auth::unblacklist_ip(&state.postgres, ip)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unblacklist IP: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to unblacklist IP".to_string() }),
            )
        })?;
    
    if removed {
        tracing::info!("Admin unblacklisted IP {}", ip);
        Ok(Json(MessageResponse {
            message: format!("IP {} removed from blacklist", ip),
        }))
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(AuthError { error: "IP not found in blacklist".to_string() }),
        ))
    }
}

/// POST /admin/sessions/cleanup - Clean up expired sessions
pub async fn cleanup_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let count = admin_auth::cleanup_expired_sessions(&state.postgres)
        .await
        .map_err(|e| {
            tracing::error!("Failed to cleanup sessions: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to cleanup sessions".to_string() }),
            )
        })?;
    
    Ok(Json(MessageResponse {
        message: format!("Cleaned up {} expired sessions", count),
    }))
}

// ============================================================================
// RPC Metrics Handlers
// ============================================================================

/// GET /admin/rpc - RPC usage summary by provider and method
pub async fn get_rpc_summary(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RpcSummaryResponse>, (StatusCode, Json<AuthError>)> {
    let data = state.clickhouse.get_rpc_summary(params.hours)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC summary: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC summary".to_string() }),
            )
        })?;
    
    Ok(Json(RpcSummaryResponse {
        hours: params.hours,
        data,
    }))
}

/// GET /admin/rpc/providers - Per-provider stats
pub async fn get_rpc_providers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RpcProvidersResponse>, (StatusCode, Json<AuthError>)> {
    let providers = state.clickhouse.get_rpc_by_provider(params.hours)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC providers: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC providers".to_string() }),
            )
        })?;
    
    Ok(Json(RpcProvidersResponse {
        hours: params.hours,
        providers,
    }))
}

/// GET /admin/rpc/errors - Recent RPC errors
pub async fn get_rpc_errors(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RpcErrorsResponse>, (StatusCode, Json<AuthError>)> {
    let errors = state.clickhouse.get_rpc_errors(params.hours, params.limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC errors: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC errors".to_string() }),
            )
        })?;
    
    Ok(Json(RpcErrorsResponse {
        hours: params.hours,
        limit: params.limit,
        errors,
    }))
}

/// GET /admin/rpc/timeseries - RPC metrics over time (minute granularity)
pub async fn get_rpc_timeseries(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RpcTimeseriesResponse>, (StatusCode, Json<AuthError>)> {
    let timeseries = state.clickhouse.get_rpc_timeseries(params.hours)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC timeseries: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC timeseries".to_string() }),
            )
        })?;
    
    Ok(Json(RpcTimeseriesResponse {
        hours: params.hours,
        timeseries,
    }))
}

/// GET /admin/rpc/daily - Daily RPC summary
pub async fn get_rpc_daily(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcDailyQuery>,
) -> Result<Json<RpcDailyResponse>, (StatusCode, Json<AuthError>)> {
    let daily = state.clickhouse.get_rpc_daily(params.days)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC daily: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC daily".to_string() }),
            )
        })?;
    
    Ok(Json(RpcDailyResponse {
        days: params.days,
        daily,
    }))
}

#[derive(Debug, Serialize)]
pub struct RpcRequestsResponse {
    pub hours: u32,
    pub limit: u32,
    pub requests: Vec<crate::clickhouse::RpcRequestRow>,
}

/// GET /admin/rpc/requests?hours=24&limit=100
/// Get recent RPC requests (all requests, not just errors)
pub async fn get_rpc_requests(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RpcRequestsResponse>, (StatusCode, Json<AuthError>)> {
    let requests = state.clickhouse.get_rpc_requests(params.hours, params.limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get RPC requests: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get RPC requests".to_string() }),
            )
        })?;
    
    Ok(Json(RpcRequestsResponse {
        hours: params.hours,
        limit: params.limit,
        requests,
    }))
}

// ============================================================================
// WebSocket Metrics Handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct WsEventsResponse {
    pub hours: u32,
    pub events: Vec<crate::clickhouse::WsEventRow>,
}

#[derive(Debug, Serialize)]
pub struct WsThroughputResponse {
    pub hours: u32,
    pub throughput: Vec<crate::clickhouse::WsThroughputSummary>,
}

/// GET /admin/ws/events?hours=24&limit=100
/// Get recent WebSocket events
pub async fn get_ws_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<WsEventsResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit;
    
    let events = state.clickhouse
        .get_ws_events(hours, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get WS events: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get WS events".to_string() }),
            )
        })?;
    
    Ok(Json(WsEventsResponse { hours, events }))
}

/// GET /admin/ws/throughput?hours=24
/// Get WebSocket throughput summary
pub async fn get_ws_throughput(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<WsThroughputResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    
    let throughput = state.clickhouse
        .get_ws_throughput_summary(hours)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get WS throughput: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get WS throughput".to_string() }),
            )
        })?;
    
    Ok(Json(WsThroughputResponse { hours, throughput }))
}

// ============================================================================
// Server Metrics Handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ServerMetricsResponse {
    pub hours: u32,
    pub metrics: Vec<crate::clickhouse::ServerMetricsRow>,
}

/// GET /admin/server/metrics?hours=24&limit=100
/// Get recent server metrics snapshots
pub async fn get_server_metrics(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<ServerMetricsResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit;
    
    let metrics = state.clickhouse
        .get_server_metrics(hours, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get server metrics: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get server metrics".to_string() }),
            )
        })?;
    
    Ok(Json(ServerMetricsResponse { hours, metrics }))
}

#[derive(Debug, Serialize)]
pub struct RequestsTimeseriesResponse {
    pub hours: u32,
    pub rps: f64,
    pub timeseries: Vec<crate::clickhouse::RequestsPerMinuteRow>,
}

/// GET /admin/server/requests-timeseries?hours=24
/// Get requests per minute time series for graphing, plus current RPS
pub async fn get_requests_timeseries(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RequestsTimeseriesResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    
    // Fetch both RPS and time series in parallel
    let (rps_result, timeseries_result) = tokio::join!(
        state.clickhouse.get_requests_per_second(),
        state.clickhouse.get_requests_per_minute(hours)
    );
    
    let rps = rps_result.map_err(|e| {
        tracing::error!("Failed to get RPS: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: "Failed to get RPS".to_string() }),
        )
    })?;
    
    let timeseries = timeseries_result.map_err(|e| {
        tracing::error!("Failed to get requests timeseries: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: "Failed to get requests timeseries".to_string() }),
        )
    })?;
    
    Ok(Json(RequestsTimeseriesResponse { hours, rps, timeseries }))
}

// ============================================================================
// Request Logs Handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct RequestLogsResponse {
    pub hours: u32,
    pub logs: Vec<crate::clickhouse::RequestLogRow>,
}

#[derive(Debug, Serialize)]
pub struct EndpointSummaryResponse {
    pub hours: u32,
    pub endpoints: Vec<crate::clickhouse::EndpointSummaryRow>,
}

#[derive(Debug, Serialize)]
pub struct RateLimitEventsResponse {
    pub hours: u32,
    pub events: Vec<crate::clickhouse::RateLimitEventRow>,
}

#[derive(Debug, Serialize)]
pub struct IpActivityResponse {
    pub hours: u32,
    pub activity: Vec<crate::clickhouse::IpActivityRow>,
}

fn default_logs_limit() -> u32 {
    500
}

#[derive(Debug, Deserialize)]
pub struct RequestLogsQuery {
    #[serde(default = "default_hours")]
    pub hours: u32,
    #[serde(default = "default_logs_limit")]
    pub limit: u32,
    pub ip_hash: Option<String>,
    pub endpoint: Option<String>,
    pub status_code: Option<u16>,
    pub status_gte: Option<u16>,
    pub status_lte: Option<u16>,
}

/// GET /admin/requests/logs?hours=24&limit=500&ip_hash=xyz&endpoint=/round&status_code=200
/// Get recent request logs with optional filters:
/// - ip_hash: Filter by IP hash
/// - endpoint: Filter by endpoint (partial match)
/// - status_code: Filter by exact status code
/// - status_gte: Filter by status >= value (e.g., 400 for all errors)
/// - status_lte: Filter by status <= value
pub async fn get_request_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RequestLogsQuery>,
) -> Result<Json<RequestLogsResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit.min(2000); // Cap at 2000 to prevent abuse
    
    let logs = state.clickhouse
        .get_request_logs_filtered(
            hours, 
            limit, 
            params.ip_hash.as_deref(),
            params.endpoint.as_deref(),
            params.status_code,
            params.status_gte,
            params.status_lte,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to get request logs: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get request logs".to_string() }),
            )
        })?;
    
    Ok(Json(RequestLogsResponse { hours, logs }))
}

/// GET /admin/requests/endpoints?hours=24
/// Get endpoint summary statistics
pub async fn get_endpoint_summary(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<EndpointSummaryResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    
    let endpoints = state.clickhouse
        .get_endpoint_summary(hours)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get endpoint summary: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get endpoint summary".to_string() }),
            )
        })?;
    
    Ok(Json(EndpointSummaryResponse { hours, endpoints }))
}

/// GET /admin/requests/rate-limits?hours=24&limit=100
/// Get recent rate limit events
pub async fn get_rate_limit_events(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<RateLimitEventsResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit;
    
    let events = state.clickhouse
        .get_rate_limit_events(hours, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get rate limit events: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get rate limit events".to_string() }),
            )
        })?;
    
    Ok(Json(RateLimitEventsResponse { hours, events }))
}

/// GET /admin/requests/ip-activity?hours=24&limit=50
/// Get IP activity summary
pub async fn get_ip_activity(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RpcMetricsQuery>,
) -> Result<Json<IpActivityResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit;
    
    let activity = state.clickhouse
        .get_ip_activity(hours, limit)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get IP activity: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: "Failed to get IP activity".to_string() }),
            )
        })?;
    
    Ok(Json(IpActivityResponse { hours, activity }))
}

// ============================================================================
// Database Size Handlers
// ============================================================================

// ============================================================================
// Database Size Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DatabaseSizeResponse {
    pub summary: StorageSummary,
    pub clickhouse: ClickHouseSizes,
    pub postgres: PostgresSizes,
}

#[derive(Debug, Serialize)]
pub struct StorageSummary {
    pub total_bytes: u64,
    pub total_rows: u64,
    pub clickhouse_bytes: u64,
    pub postgres_bytes: i64,
    pub compression_ratio: f64,
}

#[derive(Debug, Serialize)]
pub struct ClickHouseSizes {
    pub databases: Vec<crate::clickhouse::DatabaseSizeRow>,
    pub tables: Vec<DetailedTable>,
    pub engines: Vec<crate::clickhouse::TableEngineRow>,
    pub total_bytes: u64,
    pub total_bytes_uncompressed: u64,
    pub total_rows: u64,
}

#[derive(Debug, Serialize)]
pub struct DetailedTable {
    pub database: String,
    pub table: String,
    pub bytes_on_disk: u64,
    pub bytes_uncompressed: u64,
    pub compression_ratio: f64,
    pub total_rows: u64,
    pub parts_count: u64,
    pub last_modified: String,
    pub avg_row_size: f64,
}

#[derive(Debug, Serialize)]
pub struct PostgresSizes {
    pub database_name: String,
    pub database_size_bytes: i64,
    pub table_sizes: Vec<PostgresTableSize>,
    pub total_rows: i64,
}

#[derive(Debug, Serialize)]
pub struct PostgresTableSize {
    pub table_name: String,
    pub total_size_bytes: i64,
    pub table_size_bytes: i64,
    pub index_size_bytes: i64,
    pub row_count: i64,
    pub avg_row_size: f64,
    pub dead_tuples: i64,
    pub last_vacuum: Option<String>,
    pub last_analyze: Option<String>,
}

/// GET /admin/database/sizes
/// Comprehensive database storage metrics for production monitoring
pub async fn get_database_sizes(
    State(state): State<Arc<AppState>>,
) -> Result<Json<DatabaseSizeResponse>, (StatusCode, Json<AuthError>)> {
    // Get ClickHouse database-level sizes
    let ch_databases = state.clickhouse
        .get_database_sizes()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get ClickHouse database sizes: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    // Get detailed table sizes (all databases)
    let ch_tables_raw = state.clickhouse
        .get_all_table_sizes()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get ClickHouse table sizes: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    // Get table engine info
    let ch_engines = state.clickhouse
        .get_table_engines()
        .await
        .map_err(|e| {
            tracing::error!("Failed to get ClickHouse table engines: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    // Transform to detailed tables with computed metrics
    let ch_tables: Vec<DetailedTable> = ch_tables_raw.iter().map(|t| {
        let compression_ratio = if t.bytes_on_disk > 0 {
            t.bytes_uncompressed as f64 / t.bytes_on_disk as f64
        } else {
            0.0
        };
        let avg_row_size = if t.total_rows > 0 {
            t.bytes_on_disk as f64 / t.total_rows as f64
        } else {
            0.0
        };
        
        DetailedTable {
            database: t.database.clone(),
            table: t.table.clone(),
            bytes_on_disk: t.bytes_on_disk,
            bytes_uncompressed: t.bytes_uncompressed,
            compression_ratio,
            total_rows: t.total_rows,
            parts_count: t.parts_count,
            last_modified: chrono::DateTime::from_timestamp(t.last_modified as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                .unwrap_or_else(|| "Unknown".to_string()),
            avg_row_size,
        }
    }).collect();
    
    let ch_total_bytes: u64 = ch_databases.iter().map(|d| d.bytes_on_disk).sum();
    let ch_total_rows: u64 = ch_databases.iter().map(|d| d.total_rows).sum();
    let ch_total_uncompressed: u64 = ch_tables_raw.iter().map(|t| t.bytes_uncompressed).sum();
    
    // Get PostgreSQL database info
    let pg_db_info: (String, i64) = sqlx::query_as(
        "SELECT current_database()::text, pg_database_size(current_database())"
    )
        .fetch_one(&state.postgres)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get PostgreSQL database size: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("PostgreSQL error: {}", e) }),
            )
        })?;
    
    // Get detailed PostgreSQL table info
    let pg_tables: Vec<(String, i64, i64, i64, i64, i64, Option<chrono::DateTime<chrono::Utc>>, Option<chrono::DateTime<chrono::Utc>>)> = sqlx::query_as(
        r#"
        SELECT 
            relname::text AS table_name,
            pg_total_relation_size(relid) AS total_size_bytes,
            pg_table_size(relid) AS table_size_bytes,
            pg_indexes_size(relid) AS index_size_bytes,
            n_live_tup AS row_count,
            n_dead_tup AS dead_tuples,
            last_vacuum,
            last_analyze
        FROM pg_stat_user_tables
        ORDER BY pg_total_relation_size(relid) DESC
        "#
    )
        .fetch_all(&state.postgres)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get PostgreSQL table sizes: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("PostgreSQL error: {}", e) }),
            )
        })?;
    
    let pg_total_rows: i64 = pg_tables.iter().map(|t| t.4).sum();
    
    let pg_table_sizes: Vec<PostgresTableSize> = pg_tables
        .into_iter()
        .map(|(name, total, table, index, rows, dead, vacuum, analyze)| {
            let avg_row_size = if rows > 0 {
                table as f64 / rows as f64
            } else {
                0.0
            };
            PostgresTableSize {
                table_name: name,
                total_size_bytes: total,
                table_size_bytes: table,
                index_size_bytes: index,
                row_count: rows,
                avg_row_size,
                dead_tuples: dead,
                last_vacuum: vacuum.map(|v| v.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
                last_analyze: analyze.map(|a| a.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
            }
        })
        .collect();
    
    // Calculate overall compression ratio
    let overall_compression = if ch_total_bytes > 0 {
        ch_total_uncompressed as f64 / ch_total_bytes as f64
    } else {
        0.0
    };
    
    Ok(Json(DatabaseSizeResponse {
        summary: StorageSummary {
            total_bytes: ch_total_bytes + pg_db_info.1 as u64,
            total_rows: ch_total_rows + pg_total_rows as u64,
            clickhouse_bytes: ch_total_bytes,
            postgres_bytes: pg_db_info.1,
            compression_ratio: overall_compression,
        },
        clickhouse: ClickHouseSizes {
            databases: ch_databases,
            tables: ch_tables,
            engines: ch_engines,
            total_bytes: ch_total_bytes,
            total_bytes_uncompressed: ch_total_uncompressed,
            total_rows: ch_total_rows,
        },
        postgres: PostgresSizes {
            database_name: pg_db_info.0,
            database_size_bytes: pg_db_info.1,
            table_sizes: pg_table_sizes,
            total_rows: pg_total_rows,
        },
    }))
}

// ============================================================================
// Backfill Action Queue Handlers
// ============================================================================

use crate::app_state::QueuedAction;

#[derive(Debug, Serialize)]
pub struct QueueStatusResponse {
    pub paused: bool,
    pub pending_count: u64,
    pub processing: Option<QueuedAction>,
    pub total_processed: u64,
    pub total_failed: u64,
    pub processing_rate: f64,
    pub recent_completed: Vec<QueuedAction>,
    pub recent_failed: Vec<QueuedAction>,
}

#[derive(Debug, Deserialize)]
pub struct BulkEnqueueRequest {
    pub start_round: u64,
    pub end_round: u64,
    pub action: String,
    #[serde(default)]
    pub skip_if_done: bool,
    #[serde(default)]
    pub only_in_workflow: bool,
}

#[derive(Debug, Serialize)]
pub struct BulkEnqueueResponse {
    pub queued: u64,
    pub skipped: u64,
    pub message: String,
}

/// GET /admin/backfill/queue/status
/// Get current queue status
pub async fn get_queue_status(
    State(state): State<Arc<AppState>>,
) -> Json<QueueStatusResponse> {
    let cache = state.backfill_queue_cache.read().await;
    
    Json(QueueStatusResponse {
        paused: cache.paused,
        pending_count: cache.pending_count,
        processing: cache.processing.clone(),
        total_processed: cache.total_processed,
        total_failed: cache.total_failed,
        processing_rate: cache.processing_rate,
        recent_completed: cache.recent_completed.iter().cloned().collect(),
        recent_failed: cache.recent_failed.iter().cloned().collect(),
    })
}

/// POST /admin/backfill/queue/enqueue
/// Bulk enqueue actions for a range of rounds
pub async fn enqueue_actions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkEnqueueRequest>,
) -> Result<Json<BulkEnqueueResponse>, (StatusCode, Json<AuthError>)> {
    // Validate action
    let valid_actions = ["fetch_txns", "reconstruct", "finalize"];
    if !valid_actions.contains(&req.action.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(AuthError { error: format!("Invalid action: {}. Valid: {:?}", req.action, valid_actions) }),
        ));
    }
    
    let pool = &state.postgres;
    let mut queued = 0u64;
    let mut skipped = 0u64;
    
    // Get rounds to process
    let rounds: Vec<i64> = if req.only_in_workflow {
        // Only rounds already in the workflow
        sqlx::query_scalar(
            r#"
            SELECT round_id FROM round_reconstruction_status 
            WHERE round_id >= $1 AND round_id <= $2
            ORDER BY round_id
            "#
        )
        .bind(req.start_round as i64)
        .bind(req.end_round as i64)
        .fetch_all(pool)
        .await
        .map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() }))
        })?
    } else {
        // All rounds in range
        (req.start_round..=req.end_round).map(|r| r as i64).collect()
    };
    
    for round_id in rounds {
        // Check if should skip based on existing status
        if req.skip_if_done {
            let should_skip: bool = match req.action.as_str() {
                "fetch_txns" => {
                    sqlx::query_scalar(
                        "SELECT transactions_fetched FROM round_reconstruction_status WHERE round_id = $1"
                    )
                    .bind(round_id)
                    .fetch_optional(pool)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false)
                }
                "reconstruct" => {
                    sqlx::query_scalar(
                        "SELECT reconstructed FROM round_reconstruction_status WHERE round_id = $1"
                    )
                    .bind(round_id)
                    .fetch_optional(pool)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false)
                }
                "finalize" => {
                    sqlx::query_scalar(
                        "SELECT finalized FROM round_reconstruction_status WHERE round_id = $1"
                    )
                    .bind(round_id)
                    .fetch_optional(pool)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false)
                }
                _ => false,
            };
            
            if should_skip {
                skipped += 1;
                continue;
            }
        }
        
        // Insert into queue (ignore duplicates due to unique index)
        let result = sqlx::query(
            r#"
            INSERT INTO backfill_action_queue (round_id, action, status, queued_at)
            VALUES ($1, $2, 'pending', NOW())
            ON CONFLICT DO NOTHING
            "#
        )
        .bind(round_id)
        .bind(&req.action)
        .execute(pool)
        .await;
        
        match result {
            Ok(r) if r.rows_affected() > 0 => queued += 1,
            Ok(_) => skipped += 1, // Already in queue
            Err(_) => skipped += 1,
        }
    }
    
    // Update cache pending count
    {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM backfill_action_queue WHERE status = 'pending'"
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        
        let mut cache = state.backfill_queue_cache.write().await;
        cache.pending_count = pending as u64;
    }
    
    Ok(Json(BulkEnqueueResponse {
        queued,
        skipped,
        message: format!("Enqueued {} rounds, skipped {}", queued, skipped),
    }))
}

/// POST /admin/backfill/queue/pause
pub async fn pause_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    
    sqlx::query("UPDATE backfill_queue_control SET paused = true, updated_at = NOW() WHERE id = 1")
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    {
        let mut cache = state.backfill_queue_cache.write().await;
        cache.paused = true;
    }
    
    Ok(Json(MessageResponse { message: "Queue paused".to_string() }))
}

/// POST /admin/backfill/queue/resume
pub async fn resume_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    
    sqlx::query("UPDATE backfill_queue_control SET paused = false, updated_at = NOW() WHERE id = 1")
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    {
        let mut cache = state.backfill_queue_cache.write().await;
        cache.paused = false;
    }
    
    Ok(Json(MessageResponse { message: "Queue resumed".to_string() }))
}

/// POST /admin/backfill/queue/clear
pub async fn clear_queue(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    
    let deleted = sqlx::query("DELETE FROM backfill_action_queue WHERE status = 'pending'")
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?
        .rows_affected();
    
    {
        let mut cache = state.backfill_queue_cache.write().await;
        cache.pending_count = 0;
    }
    
    Ok(Json(MessageResponse { message: format!("Cleared {} pending items", deleted) }))
}

/// POST /admin/backfill/queue/retry-failed
pub async fn retry_failed_items(
    State(state): State<Arc<AppState>>,
) -> Result<Json<MessageResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    
    let updated = sqlx::query(
        r#"
        UPDATE backfill_action_queue 
        SET status = 'pending', error = NULL, started_at = NULL, completed_at = NULL, queued_at = NOW()
        WHERE status = 'failed'
        "#
    )
    .execute(pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?
    .rows_affected();
    
    // Update cache
    {
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM backfill_action_queue WHERE status = 'pending'"
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);
        
        let mut cache = state.backfill_queue_cache.write().await;
        cache.pending_count = pending as u64;
        cache.recent_failed.clear();
    }
    
    Ok(Json(MessageResponse { message: format!("Retrying {} failed items", updated) }))
}

// ============================================================================
// Bulk Verify & Range Add Handlers
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct BulkVerifyRequest {
    /// List of specific round IDs to verify
    #[serde(default)]
    pub round_ids: Vec<u64>,
    /// Or specify a range
    pub start_round: Option<u64>,
    pub end_round: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct BulkVerifyResponse {
    pub verified: u64,
    pub message: String,
}

/// POST /admin/backfill/bulk-verify
/// Bulk mark rounds as verified
pub async fn bulk_verify_rounds(
    State(state): State<Arc<AppState>>,
    Json(req): Json<BulkVerifyRequest>,
) -> Result<Json<BulkVerifyResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    let mut verified = 0u64;
    
    if !req.round_ids.is_empty() {
        // Verify specific round IDs
        for round_id in &req.round_ids {
            let result = sqlx::query(
                r#"
                UPDATE round_reconstruction_status
                SET verified = true, verified_at = NOW(), verification_notes = 'Bulk verified via Command Center'
                WHERE round_id = $1 AND reconstructed = true
                "#
            )
            .bind(*round_id as i64)
            .execute(pool)
            .await;
            
            if let Ok(r) = result {
                verified += r.rows_affected();
            }
        }
    } else if let (Some(start), Some(end)) = (req.start_round, req.end_round) {
        // Verify range
        let result = sqlx::query(
            r#"
            UPDATE round_reconstruction_status
            SET verified = true, verified_at = NOW(), verification_notes = 'Bulk verified via Command Center'
            WHERE round_id >= $1 AND round_id <= $2 AND reconstructed = true
            "#
        )
        .bind(start as i64)
        .bind(end as i64)
        .execute(pool)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
        
        verified = result.rows_affected();
    }
    
    Ok(Json(BulkVerifyResponse {
        verified,
        message: format!("Marked {} rounds as verified", verified),
    }))
}

#[derive(Debug, Deserialize)]
pub struct AddRangeToBackfillRequest {
    pub start_round: u64,
    pub end_round: u64,
}

#[derive(Debug, Serialize)]
pub struct AddRangeToBackfillResponse {
    pub added: u64,
    pub already_in_workflow: u64,
    pub message: String,
}

/// POST /admin/backfill/add-range
/// Add a range of rounds to the backfill workflow
/// Only adds rounds that aren't already in the workflow
/// Marks meta_fetched as true since we already have round data
pub async fn add_range_to_backfill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AddRangeToBackfillRequest>,
) -> Result<Json<AddRangeToBackfillResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    let mut added = 0u64;
    let mut already_in_workflow = 0u64;
    
    for round_id in req.start_round..=req.end_round {
        // Check if round exists in workflow
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM round_reconstruction_status WHERE round_id = $1)"
        )
        .bind(round_id as i64)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
        
        if exists {
            already_in_workflow += 1;
            continue;
        }
        
        // Insert new round with meta_fetched = true
        let result = sqlx::query(
            r#"
            INSERT INTO round_reconstruction_status (round_id, meta_fetched, transactions_fetched, reconstructed, verified, finalized, created_at)
            VALUES ($1, true, false, false, false, false, NOW())
            ON CONFLICT (round_id) DO NOTHING
            "#
        )
        .bind(round_id as i64)
        .execute(pool)
        .await;
        
        if let Ok(r) = result {
            if r.rows_affected() > 0 {
                added += 1;
            } else {
                already_in_workflow += 1;
            }
        }
    }
    
    Ok(Json(AddRangeToBackfillResponse {
        added,
        already_in_workflow,
        message: format!("Added {} rounds to workflow, {} already existed", added, already_in_workflow),
    }))
}

// ============================================================================
// Pipeline Stats & Memory Handlers
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PipelineStatsResponse {
    /// Rounds with invalid deployments, not in workflow
    pub not_in_workflow: u64,
    /// In workflow, transactions not fetched
    pub pending_txns: u64,
    /// Txns fetched, not reconstructed
    pub pending_reconstruct: u64,
    /// Reconstructed, not verified
    pub pending_verify: u64,
    /// Verified, not finalized
    pub pending_finalize: u64,
    /// Finalized (complete)
    pub complete: u64,
}

/// GET /admin/backfill/pipeline-stats
/// Get counts for each stage of the pipeline
pub async fn get_pipeline_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PipelineStatsResponse>, (StatusCode, Json<AuthError>)> {
    let pool = &state.postgres;
    
    // Count rounds not in workflow but with invalid deployments (deployment_count = 0)
    // We query ClickHouse for rounds with 0 deployments and check if they're in the workflow
    let not_in_workflow: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM (
            SELECT round_id FROM round_reconstruction_status WHERE meta_fetched = true
        ) AS workflow
        RIGHT JOIN (
            SELECT generate_series(1, (SELECT MAX(round_id) FROM round_reconstruction_status)) AS round_id
        ) AS all_rounds ON workflow.round_id = all_rounds.round_id
        WHERE workflow.round_id IS NULL
        "#
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    
    // Pending txns: in workflow, meta fetched, but txns not fetched
    let pending_txns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM round_reconstruction_status WHERE meta_fetched = true AND transactions_fetched = false"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    
    // Pending reconstruct: txns fetched but not finalized AND not in memory
    // A round needs reconstruct if: txns fetched, not finalized, and not in the in-memory cache
    let finalized_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM round_reconstruction_status WHERE finalized = true"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    
    let txns_fetched_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM round_reconstruction_status WHERE transactions_fetched = true"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    
    // Pending verify = count of rounds IN MEMORY (reconstructed but not finalized)
    let pending_verify = {
        let cache = state.backfill_reconstructed_cache.read().await;
        cache.len() as u64
    };
    
    // Pending reconstruct = txns_fetched - finalized - in_memory
    let pending_reconstruct = (txns_fetched_count as u64)
        .saturating_sub(finalized_count as u64)
        .saturating_sub(pending_verify);
    
    // Complete: finalized
    let complete = finalized_count as u64;
    
    // Pending finalize is 0 - finalize happens immediately when called on in-memory data
    // (the old "verified but not finalized" state doesn't exist anymore)
    let pending_finalize = 0u64;
    
    Ok(Json(PipelineStatsResponse {
        not_in_workflow: not_in_workflow as u64,
        pending_txns: pending_txns as u64,
        pending_reconstruct,
        pending_verify,
        pending_finalize,
        complete,
    }))
}

#[derive(Debug, Serialize)]
pub struct MemoryUsageResponse {
    /// Process memory usage in bytes
    pub memory_bytes: u64,
    /// Process memory in human readable format
    pub memory_human: String,
    /// Queue cache size estimate
    pub queue_cache_items: u64,
}

/// GET /admin/backfill/memory
/// Get current memory usage
pub async fn get_memory_usage(
    State(state): State<Arc<AppState>>,
) -> Json<MemoryUsageResponse> {
    // Try to get memory usage from /proc/self/statm (Linux)
    // On macOS, fall back to a different method
    let memory_bytes = get_process_memory();
    
    let memory_human = if memory_bytes > 1_073_741_824 {
        format!("{:.2} GB", memory_bytes as f64 / 1_073_741_824.0)
    } else if memory_bytes > 1_048_576 {
        format!("{:.2} MB", memory_bytes as f64 / 1_048_576.0)
    } else {
        format!("{} KB", memory_bytes / 1024)
    };
    
    let queue_cache_items = {
        let cache = state.backfill_queue_cache.blocking_read();
        cache.recent_completed.len() as u64 + cache.recent_failed.len() as u64
    };
    
    Json(MemoryUsageResponse {
        memory_bytes,
        memory_human,
        queue_cache_items,
    })
}

fn get_process_memory() -> u64 {
    // Try Linux /proc/self/statm
    if let Ok(content) = std::fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = content.split_whitespace().collect();
        if let Some(pages) = parts.get(1) {
            if let Ok(pages) = pages.parse::<u64>() {
                return pages * 4096; // Page size is typically 4KB
            }
        }
    }
    
    // Fallback: try using sysinfo or return 0
    0
}

// ============================================================================
// Invalid Rounds Query
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct InvalidRoundsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Serialize)]
pub struct InvalidRoundsResponse {
    pub rounds: Vec<InvalidRound>,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct InvalidRound {
    pub round_id: u64,
    pub deployment_count: i64,
    pub in_workflow: bool,
}

/// GET /admin/backfill/reconstructed
/// Get list of rounds currently reconstructed in memory (awaiting finalize)
pub async fn get_reconstructed_rounds(
    State(state): State<Arc<AppState>>,
) -> Json<ReconstructedRoundsResponse> {
    let cache = state.backfill_reconstructed_cache.read().await;
    
    let rounds: Vec<ReconstructedRoundInfo> = cache.rounds
        .values()
        .map(|r| ReconstructedRoundInfo {
            round_id: r.round_id,
            deployment_count: r.deployments.len(),
            transaction_count: r.transaction_count,
            reconstructed_at: r.reconstructed_at.to_rfc3339(),
        })
        .collect();
    
    Json(ReconstructedRoundsResponse {
        count: rounds.len(),
        rounds,
    })
}

#[derive(Debug, Serialize)]
pub struct ReconstructedRoundsResponse {
    pub count: usize,
    pub rounds: Vec<ReconstructedRoundInfo>,
}

#[derive(Debug, Serialize)]
pub struct ReconstructedRoundInfo {
    pub round_id: u64,
    pub deployment_count: usize,
    pub transaction_count: usize,
    pub reconstructed_at: String,
}

/// GET /admin/backfill/invalid-rounds
/// Get rounds with invalid/missing deployments that aren't in the workflow
pub async fn get_invalid_rounds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<InvalidRoundsQuery>,
) -> Result<Json<InvalidRoundsResponse>, (StatusCode, Json<AuthError>)> {
    // Query ClickHouse for rounds with 0 deployments
    let invalid_from_ch = state.clickhouse.get_rounds_with_zero_deployments(params.limit, params.offset)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get invalid rounds from ClickHouse: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() }))
        })?;
    
    let pool = &state.postgres;
    
    // Check which rounds are in the workflow
    let mut rounds = Vec::new();
    for (round_id, deployment_count) in invalid_from_ch {
        let in_workflow: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM round_reconstruction_status WHERE round_id = $1)"
        )
        .bind(round_id as i64)
        .fetch_one(pool)
        .await
        .unwrap_or(false);
        
        rounds.push(InvalidRound {
            round_id,
            deployment_count,
            in_workflow,
        });
    }
    
    // Get total count of invalid rounds
    let total = state.clickhouse.count_rounds_with_zero_deployments()
        .await
        .unwrap_or(0);
    
    Ok(Json(InvalidRoundsResponse {
        rounds,
        total,
    }))
}

// ============================================================================
// Router
// ============================================================================

/// Create the admin router with all routes
/// Login is public, all other routes require authentication
pub fn admin_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    // Protected routes - require valid session
    let protected = Router::new()
        .route("/logout", post(logout))
        .route("/metrics", get(get_admin_metrics))
        .route("/blacklist", get(get_blacklist))
        .route("/blacklist", post(add_to_blacklist))
        .route("/blacklist/{ip}", delete(remove_from_blacklist))
        .route("/sessions/cleanup", post(cleanup_sessions))
        // RPC metrics
        .route("/rpc", get(get_rpc_summary))
        .route("/rpc/providers", get(get_rpc_providers))
        .route("/rpc/errors", get(get_rpc_errors))
        .route("/rpc/timeseries", get(get_rpc_timeseries))
        .route("/rpc/daily", get(get_rpc_daily))
        .route("/rpc/requests", get(get_rpc_requests))
        // WebSocket metrics
        .route("/ws/events", get(get_ws_events))
        .route("/ws/throughput", get(get_ws_throughput))
        // Server metrics
        .route("/server/metrics", get(get_server_metrics))
        .route("/server/requests-timeseries", get(get_requests_timeseries))
        // Request logs
        .route("/requests/logs", get(get_request_logs))
        .route("/requests/endpoints", get(get_endpoint_summary))
        .route("/requests/rate-limits", get(get_rate_limit_events))
        .route("/requests/ip-activity", get(get_ip_activity))
        // Database sizes
        .route("/database/sizes", get(get_database_sizes))
        // Backfill workflow
        .route("/backfill/rounds", post(crate::backfill::backfill_rounds))
        .route("/backfill/rounds/status", get(crate::backfill::get_backfill_rounds_status))
        .route("/backfill/rounds/cancel", post(crate::backfill::cancel_backfill_rounds))
        .route("/backfill/deployments", post(crate::backfill::add_to_backfill_workflow))
        .route("/rounds/pending", get(crate::backfill::get_pending_rounds))
        .route("/rounds/data", get(crate::backfill::get_rounds_with_data))
        .route("/rounds/missing", get(crate::backfill::get_missing_rounds))
        .route("/rounds/stats", get(crate::backfill::get_round_stats))
        .route("/rounds/bulk-delete", post(crate::backfill::bulk_delete_rounds))
        .route("/rounds/{round_id}/status", get(crate::backfill::get_round_data_status))
        .route("/rounds/{round_id}", delete(crate::backfill::delete_round_data))
        .route("/fetch-txns/{round_id}", post(crate::backfill::fetch_round_transactions))
        .route("/reset-txns/{round_id}", post(crate::backfill::reset_txns_status))
        .route("/reconstruct/{round_id}", post(crate::backfill::reconstruct_round))
        .route("/verify/{round_id}", get(crate::backfill::get_round_for_verification))
        .route("/verify/{round_id}", post(crate::backfill::verify_round))
        .route("/finalize/{round_id}", post(crate::backfill::finalize_backfill_round))
        // Transaction viewer
        .route("/transactions/rounds", get(crate::backfill::get_rounds_with_transactions))
        .route("/transactions/{round_id}", get(crate::backfill::get_round_transactions))
        .route("/transactions/{round_id}/raw", get(crate::backfill::get_round_transactions_raw))
        .route("/transactions/{round_id}/full", get(crate::backfill::get_round_transactions_full))
        .route("/transactions/single/{signature}", get(crate::backfill::get_single_transaction))
        // Automation state reconstruction
        .route("/automation/stats", get(crate::automation_states::get_queue_stats))
        .route("/automation/fetch-stats", get(crate::automation_states::get_fetch_stats))
        .route("/automation/live", get(crate::automation_states::get_live_stats))
        .route("/automation/queue", get(crate::automation_states::get_queue_items))
        .route("/automation/queue", post(crate::automation_states::add_to_queue))
        .route("/automation/queue/process", post(crate::automation_states::process_queue))
        .route("/automation/queue/retry", post(crate::automation_states::retry_failed))
        .route("/automation/queue/round/{round_id}", post(crate::automation_states::queue_missing_for_round))
        .route("/automation/queue/from-txns/{round_id}", post(crate::automation_states::queue_from_round_transactions))
        // Transaction parse queue (new queue-based system)
        .route("/automation/parse-queue", get(crate::automation_states::get_parse_queue_stats))
        .route("/automation/parse-queue/items", get(crate::automation_states::get_parse_queue_items))
        .route("/automation/queue-round/{round_id}", post(crate::automation_states::queue_round_for_parsing))
        // Backfill action queue (Command Center)
        .route("/backfill/queue/status", get(get_queue_status))
        .route("/backfill/queue/enqueue", post(enqueue_actions))
        .route("/backfill/queue/pause", post(pause_queue))
        .route("/backfill/queue/resume", post(resume_queue))
        .route("/backfill/queue/clear", post(clear_queue))
        .route("/backfill/queue/retry-failed", post(retry_failed_items))
        .route("/backfill/bulk-verify", post(bulk_verify_rounds))
        .route("/backfill/add-range", post(add_range_to_backfill))
        .route("/backfill/pipeline-stats", get(get_pipeline_stats))
        .route("/backfill/memory", get(get_memory_usage))
        .route("/backfill/invalid-rounds", get(get_invalid_rounds))
        .route("/backfill/reconstructed", get(get_reconstructed_rounds))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_auth::require_admin_auth,
        ));
    
    // Public routes (login) + protected routes
    Router::new()
        .route("/login", post(login))
        .merge(protected)
}

