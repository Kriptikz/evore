//! ore-stats - Live ORE mining data server
//!
//! Provides:
//! - Real-time account data via HTTP endpoints
//! - SSE streams for live updates
//! - RPC proxy for frontend
//! - Metrics tracking to ClickHouse

use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    routing::get,
    Router,
};
use tokio::sync::RwLock;
use tower_http::cors::{CorsLayer, Any};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod app_state;
mod app_error;
mod app_rpc;
mod clickhouse;
mod database;
mod entropy_api;
mod external_api;
mod helius_api;
mod ore_token_cache;
mod routes;
mod rpc;
mod sse;
mod tasks;
mod websocket;

// Keep these for reference but don't compile:
// - main_old.rs
// - database_old.rs  
// - app_state_old.rs
// - old_rpc.rs
mod account_tracker;

use app_state::AppState;
use app_rpc::AppRpc;
use clickhouse::ClickHouseClient;
use helius_api::HeliusApi;
use ore_token_cache::OreTokenCache;
use websocket::WebSocketManager;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment
    dotenvy::dotenv().ok();
    
    // Initialize tracing
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,ore_stats=debug"));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env_filter)
        .init();
    
    tracing::info!("Starting ore-stats server...");
    
    // ========== Database Connections ==========
    
    // ClickHouse
    let clickhouse_url = env::var("CLICKHOUSE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:8123".to_string());
    let clickhouse_user = env::var("CLICKHOUSE_USER")
        .unwrap_or_else(|_| "default".to_string());
    let clickhouse_password = env::var("CLICKHOUSE_PASSWORD")
        .unwrap_or_else(|_| "".to_string());
    let clickhouse_db = env::var("CLICKHOUSE_DATABASE")
        .unwrap_or_else(|_| "ore_stats".to_string());
    
    let clickhouse: Arc<ClickHouseClient> = Arc::new(
        ClickHouseClient::new(&clickhouse_url, &clickhouse_db, &clickhouse_user, &clickhouse_password)
    );
    tracing::info!("Connected to ClickHouse at {}", clickhouse_url);
    
    // PostgreSQL
    let postgres_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");
    let postgres = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&postgres_url)
        .await?;
    tracing::info!("Connected to PostgreSQL");
    
    // ========== RPC Client ==========
    
    let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let rpc = Arc::new(AppRpc::new(rpc_url.clone(), Some(clickhouse.clone())));
    tracing::info!("RPC client initialized");
    
    // ========== Helius API for token holders ==========
    
    let helius = Arc::new(RwLock::new(HeliusApi::new(rpc_url.clone())));
    
    // ========== Application State ==========
    
    let state = Arc::new(AppState::new(
        clickhouse.clone(),
        postgres.clone(),
        rpc.clone(),
    ));
    
    // ========== Background Tasks ==========
    
    // WebSocket manager for slot tracking
    let ws_manager = WebSocketManager::new(rpc_url.clone());
    
    // Slot subscription
    let slot_handle = ws_manager.spawn_slot_subscription(state.slot_cache.clone());
    tracing::info!("Slot subscription started");
    
    // Round broadcaster (sends to SSE every 500ms)
    let round_broadcast_handle = ws_manager.spawn_round_broadcaster(state.clone());
    tracing::info!("Round broadcaster started");
    
    // ORE token cache
    let token_cache = Arc::new(OreTokenCache::new(
        helius.clone(),
        state.ore_holders_cache.clone(),
        state.slot_cache.clone(),
    ));
    let token_cache_handle = token_cache.spawn_update_task();
    tracing::info!("ORE token cache started");
    
    // RPC polling task
    let polling_handle = tasks::spawn_rpc_polling(state.clone());
    tracing::info!("RPC polling started");
    
    // Metrics snapshot task
    let metrics_handle = tasks::spawn_metrics_snapshot(state.clone());
    tracing::info!("Metrics snapshot task started");
    
    // ========== Axum Router ==========
    
    let app = Router::new()
        // Health check
        .route("/health", get(routes::health))
        
        // ORE Account endpoints (from cache)
        .route("/treasury", get(routes::get_treasury))
        .route("/board", get(routes::get_board))
        .route("/round", get(routes::get_round))
        .route("/miners", get(routes::get_miners))
        .route("/miner/{pubkey}", get(routes::get_miner))
        
        // Live data
        .route("/live/round", get(routes::get_live_round))
        .route("/slot", get(routes::get_slot))
        
        // RPC proxy
        .route("/balance/{pubkey}", get(routes::get_balance))
        
        // ORE token balances
        .route("/ore-balance/{owner}", get(routes::get_ore_balance))
        .route("/ore-holders", get(routes::get_ore_holders))
        
        // Metrics
        .route("/metrics", get(routes::get_metrics))
        
        // SSE streams
        .route("/sse/rounds", get(sse::sse_rounds))
        .route("/sse/deployments", get(sse::sse_deployments))
        
        // State
        .with_state(state.clone())
        
        // CORS
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        );
    
    // ========== Start Server ==========
    
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(3000);
    
    // Bind to localhost only - nginx will proxy external requests
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    // Cleanup (won't reach here normally)
    slot_handle.abort();
    round_broadcast_handle.abort();
    token_cache_handle.abort();
    polling_handle.abort();
    metrics_handle.abort();
    
    Ok(())
}
