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
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_auth::require_admin_auth,
        ));
    
    // Public routes (login) + protected routes
    Router::new()
        .route("/login", post(login))
        .merge(protected)
}

