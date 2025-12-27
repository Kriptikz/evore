//! HTTP middleware for request logging and metrics
//!
//! Provides:
//! - Request/response timing and logging to ClickHouse
//! - Real IP tracking from X-Forwarded-For
//! - Rate limit event detection

use std::sync::Arc;
use std::time::Instant;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::net::SocketAddr;

use crate::app_state::AppState;
use crate::clickhouse::{RequestLog, RateLimitEvent};

/// Middleware to log HTTP requests to ClickHouse
pub async fn request_logging_middleware(
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let start = Instant::now();
    
    // Extract request info before passing to handler
    let method = request.method().to_string();
    let uri = request.uri().path().to_string();
    let user_agent = request
        .headers()
        .get("user-agent")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_string();
    
    // Get real IP from X-Forwarded-For header (from Nginx) or connection
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|h| h.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| addr.ip().to_string());
    
    // Execute the request
    let response = next.run(request).await;
    
    let duration_ms = start.elapsed().as_millis() as u32;
    let status_code = response.status().as_u16();
    
    // Log to ClickHouse asynchronously
    let log = RequestLog {
        endpoint: uri.clone(),
        method: method.clone(),
        status_code,
        duration_ms,
        client_ip: client_ip.clone(),
        user_agent: truncate_string(&user_agent, 256),
    };
    
    let clickhouse = state.clickhouse.clone();
    tokio::spawn(async move {
        if let Err(e) = clickhouse.insert_request_log(log).await {
            tracing::warn!("Failed to log request: {}", e);
        }
    });
    
    // Log rate limit events (429 responses)
    if status_code == 429 {
        let event = RateLimitEvent {
            client_ip,
            endpoint: uri,
            requests_in_window: 0, // We don't have this info here
            window_seconds: 0,
        };
        
        let clickhouse = state.clickhouse.clone();
        tokio::spawn(async move {
            if let Err(e) = clickhouse.insert_rate_limit_event(event).await {
                tracing::warn!("Failed to log rate limit event: {}", e);
            }
        });
    }
    
    response
}

/// Truncate a string to max length, adding ... if truncated
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello...");
    }
}

