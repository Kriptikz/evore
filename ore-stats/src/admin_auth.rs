//! Admin authentication module
//!
//! Provides:
//! - Password verification with Argon2
//! - Session management (create, validate, revoke)
//! - IP blacklist checking
//! - Failed login attempt tracking

use std::net::IpAddr;
use std::sync::Arc;

use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::app_state::AppState;

// ============================================================================
// Constants
// ============================================================================

/// Maximum failed login attempts before IP is blacklisted
const MAX_FAILED_ATTEMPTS: i64 = 3;

/// Session duration in hours
const SESSION_DURATION_HOURS: i64 = 24;

/// Failed attempts window in minutes (only count recent attempts)
const FAILED_ATTEMPTS_WINDOW_MINUTES: i64 = 15;

// ============================================================================
// Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AuthError {
    pub error: String,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        (StatusCode::UNAUTHORIZED, Json(self)).into_response()
    }
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionInfo {
    pub id: Uuid,
    pub created_at: String,
    pub expires_at: String,
    pub ip_address: String,
}

// ============================================================================
// Password Verification
// ============================================================================

/// Verify a password against the stored Argon2 hash from ADMIN_PASSWORD_HASH env var
pub fn verify_password(password: &str, password_hash: &str) -> bool {
    let parsed_hash = match PasswordHash::new(password_hash) {
        Ok(h) => h,
        Err(_) => return false,
    };
    
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok()
}

/// Hash a password with Argon2 (for generating the hash to store in env)
/// This is a utility function, not used at runtime
#[allow(dead_code)]
pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    use argon2::{password_hash::SaltString, PasswordHasher};
    use rand::rngs::OsRng;
    
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

// ============================================================================
// Session Management
// ============================================================================

/// Generate a new session token (64 random bytes, hex encoded = 128 chars)
fn generate_session_token() -> String {
    let mut bytes = [0u8; 64];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

/// Hash a session token for storage (SHA256)
fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

/// Create a new admin session
pub async fn create_session(
    pool: &PgPool,
    ip: IpAddr,
    user_agent: Option<&str>,
) -> Result<(String, DateTime<Utc>), sqlx::Error> {
    let token = generate_session_token();
    let token_hash = hash_token(&token);
    let expires_at = Utc::now() + Duration::hours(SESSION_DURATION_HOURS);
    let ip_str = ip.to_string();
    
    sqlx::query(
        r#"
        INSERT INTO admin_sessions (token_hash, expires_at, ip_address, user_agent)
        VALUES ($1, $2, $3::inet, $4)
        "#
    )
    .bind(&token_hash)
    .bind(expires_at)
    .bind(&ip_str)
    .bind(user_agent)
    .execute(pool)
    .await?;
    
    Ok((token, expires_at))
}

/// Validate a session token and return session info if valid
pub async fn validate_session(
    pool: &PgPool,
    token: &str,
) -> Result<Option<SessionInfo>, sqlx::Error> {
    let token_hash = hash_token(token);
    
    let row = sqlx::query(
        r#"
        SELECT id, created_at, expires_at, ip_address::text
        FROM admin_sessions
        WHERE token_hash = $1 AND expires_at > NOW()
        "#
    )
    .bind(&token_hash)
    .fetch_optional(pool)
    .await?;
    
    Ok(row.map(|r| SessionInfo {
        id: r.get("id"),
        created_at: r.get::<DateTime<Utc>, _>("created_at").to_rfc3339(),
        expires_at: r.get::<DateTime<Utc>, _>("expires_at").to_rfc3339(),
        ip_address: r.get("ip_address"),
    }))
}

/// Revoke a session by token
pub async fn revoke_session(pool: &PgPool, token: &str) -> Result<bool, sqlx::Error> {
    let token_hash = hash_token(token);
    
    let result = sqlx::query("DELETE FROM admin_sessions WHERE token_hash = $1")
        .bind(&token_hash)
        .execute(pool)
        .await?;
    
    Ok(result.rows_affected() > 0)
}

/// Revoke all sessions (logout everywhere)
pub async fn revoke_all_sessions(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM admin_sessions")
        .execute(pool)
        .await?;
    
    Ok(result.rows_affected())
}

/// Clean up expired sessions
pub async fn cleanup_expired_sessions(pool: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query("DELETE FROM admin_sessions WHERE expires_at < NOW()")
        .execute(pool)
        .await?;
    
    Ok(result.rows_affected())
}

// ============================================================================
// IP Blacklist
// ============================================================================

/// Check if an IP is blacklisted
pub async fn is_ip_blacklisted(pool: &PgPool, ip: IpAddr) -> Result<bool, sqlx::Error> {
    let ip_str = ip.to_string();
    
    let row: (bool,) = sqlx::query_as(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM ip_blacklist 
            WHERE ip_address = $1::inet 
            AND (expires_at IS NULL OR expires_at > NOW())
        )
        "#
    )
    .bind(&ip_str)
    .fetch_one(pool)
    .await?;
    
    Ok(row.0)
}

/// Record a failed login attempt and potentially blacklist the IP
pub async fn record_failed_attempt(
    pool: &PgPool,
    ip: IpAddr,
    endpoint: &str,
) -> Result<bool, sqlx::Error> {
    let ip_str = ip.to_string();
    
    // Record the attempt
    sqlx::query("INSERT INTO failed_login_attempts (ip_address, endpoint) VALUES ($1::inet, $2)")
        .bind(&ip_str)
        .bind(endpoint)
        .execute(pool)
        .await?;
    
    // Count recent attempts
    let window_start = Utc::now() - Duration::minutes(FAILED_ATTEMPTS_WINDOW_MINUTES);
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT COUNT(*)
        FROM failed_login_attempts
        WHERE ip_address = $1::inet AND attempted_at > $2
        "#
    )
    .bind(&ip_str)
    .bind(window_start)
    .fetch_one(pool)
    .await?;
    
    // Blacklist if too many attempts
    if count.0 >= MAX_FAILED_ATTEMPTS {
        sqlx::query(
            r#"
            INSERT INTO ip_blacklist (ip_address, reason, failed_attempts)
            VALUES ($1::inet, 'Too many failed login attempts', $2)
            ON CONFLICT (ip_address) DO UPDATE 
            SET failed_attempts = ip_blacklist.failed_attempts + 1,
                blocked_at = NOW()
            "#
        )
        .bind(&ip_str)
        .bind(count.0 as i32)
        .execute(pool)
        .await?;
        
        return Ok(true); // IP was blacklisted
    }
    
    Ok(false)
}

/// Manually blacklist an IP (admin action)
pub async fn blacklist_ip(
    pool: &PgPool,
    ip: IpAddr,
    reason: &str,
    permanent: bool,
) -> Result<(), sqlx::Error> {
    let ip_str = ip.to_string();
    let expires_at = if permanent {
        None
    } else {
        Some(Utc::now() + Duration::hours(24))
    };
    
    sqlx::query(
        r#"
        INSERT INTO ip_blacklist (ip_address, reason, expires_at, created_by)
        VALUES ($1::inet, $2, $3, 'admin')
        ON CONFLICT (ip_address) DO UPDATE 
        SET reason = $2, expires_at = $3, blocked_at = NOW()
        "#
    )
    .bind(&ip_str)
    .bind(reason)
    .bind(expires_at)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Remove an IP from the blacklist
pub async fn unblacklist_ip(pool: &PgPool, ip: IpAddr) -> Result<bool, sqlx::Error> {
    let ip_str = ip.to_string();
    
    let result = sqlx::query("DELETE FROM ip_blacklist WHERE ip_address = $1::inet")
        .bind(&ip_str)
        .execute(pool)
        .await?;
    
    Ok(result.rows_affected() > 0)
}

/// Get all blacklisted IPs
pub async fn get_blacklist(pool: &PgPool) -> Result<Vec<BlacklistEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"
        SELECT 
            ip_address::text,
            reason,
            failed_attempts,
            blocked_at,
            expires_at,
            created_by
        FROM ip_blacklist
        ORDER BY blocked_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;
    
    let entries = rows.into_iter().map(|r| BlacklistEntry {
        ip_address: r.get("ip_address"),
        reason: r.get("reason"),
        failed_attempts: r.get("failed_attempts"),
        blocked_at: r.get("blocked_at"),
        expires_at: r.get("expires_at"),
        created_by: r.get("created_by"),
    }).collect();
    
    Ok(entries)
}

#[derive(Debug, Serialize)]
pub struct BlacklistEntry {
    pub ip_address: String,
    pub reason: String,
    pub failed_attempts: i32,
    pub blocked_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
}

// ============================================================================
// Middleware
// ============================================================================

/// Extract client IP from request headers (respects X-Forwarded-For from nginx)
pub fn extract_client_ip(headers: &HeaderMap) -> Option<IpAddr> {
    // Try X-Forwarded-For first (from nginx)
    if let Some(xff) = headers.get("x-forwarded-for") {
        if let Ok(xff_str) = xff.to_str() {
            // Take the first IP in the chain (original client)
            if let Some(first_ip) = xff_str.split(',').next() {
                if let Ok(ip) = first_ip.trim().parse::<IpAddr>() {
                    return Some(ip);
                }
            }
        }
    }
    
    // Fallback to X-Real-IP
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            if let Ok(ip) = ip_str.parse::<IpAddr>() {
                return Some(ip);
            }
        }
    }
    
    None
}

/// Extract bearer token from Authorization header
pub fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
}

/// Admin authentication middleware
/// Checks for valid session token in Authorization header
pub async fn require_admin_auth(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = request.headers();
    
    // Check IP blacklist first
    if let Some(ip) = extract_client_ip(headers) {
        match is_ip_blacklisted(&state.postgres, ip).await {
            Ok(true) => {
                return Err(AuthError {
                    error: "IP address is blacklisted".to_string(),
                });
            }
            Err(e) => {
                tracing::error!("Failed to check IP blacklist: {}", e);
                // Continue anyway - don't block on DB errors
            }
            _ => {}
        }
    }
    
    // Extract and validate token
    let token = extract_bearer_token(headers).ok_or_else(|| AuthError {
        error: "Missing or invalid Authorization header".to_string(),
    })?;
    
    match validate_session(&state.postgres, &token).await {
        Ok(Some(_session)) => {
            // Valid session, proceed
            Ok(next.run(request).await)
        }
        Ok(None) => Err(AuthError {
            error: "Invalid or expired session".to_string(),
        }),
        Err(e) => {
            tracing::error!("Session validation error: {}", e);
            Err(AuthError {
                error: "Authentication error".to_string(),
            })
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_password_hashing() {
        let password = "test_password_123";
        let hash = hash_password(password).unwrap();
        
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }
    
    #[test]
    fn test_token_generation() {
        let token1 = generate_session_token();
        let token2 = generate_session_token();
        
        // 64 bytes = 128 hex chars
        assert_eq!(token1.len(), 128);
        assert_eq!(token2.len(), 128);
        
        // Should be different
        assert_ne!(token1, token2);
    }
    
    #[test]
    fn test_token_hashing() {
        let token = "test_token";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);
        
        // SHA256 = 32 bytes = 64 hex chars
        assert_eq!(hash1.len(), 64);
        
        // Same input = same hash
        assert_eq!(hash1, hash2);
        
        // Different input = different hash
        assert_ne!(hash_token("other_token"), hash1);
    }
}
