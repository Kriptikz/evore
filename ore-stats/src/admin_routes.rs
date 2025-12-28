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

#[derive(Debug, Deserialize)]
pub struct RequestLogsQuery {
    #[serde(default = "default_hours")]
    pub hours: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub ip_hash: Option<String>,
}

/// GET /admin/requests/logs?hours=24&limit=100&ip_hash=xyz
/// Get recent request logs, optionally filtered by IP hash
pub async fn get_request_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RequestLogsQuery>,
) -> Result<Json<RequestLogsResponse>, (StatusCode, Json<AuthError>)> {
    let hours = params.hours;
    let limit = params.limit;
    
    let logs = state.clickhouse
        .get_request_logs(hours, limit, params.ip_hash.as_deref())
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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_auth::require_admin_auth,
        ));
    
    // Public routes (login) + protected routes
    Router::new()
        .route("/login", post(login))
        .merge(protected)
}

