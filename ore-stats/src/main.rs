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
    middleware::from_fn_with_state,
    routing::get,
    Router,
};
use tokio::sync::RwLock;
// CORS is handled by nginx - no tower_http::cors needed
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod admin_auth;
mod admin_routes;
mod app_state;
mod app_error;
mod app_rpc;
mod automation_states;
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
mod middleware;
mod websocket;
mod finalization;
mod backfill;
mod evore_cache;
mod evore_routes;
mod historical_routes;
mod tx_analyzer;

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
    
    // ========== RPC Clients ==========

    let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let flux_rpc_url = env::var("FLUX_RPC_URL").expect("FLUX_RPC_URL must be set");
    let rpc = Arc::new(AppRpc::new(rpc_url.clone(), flux_rpc_url.clone(), Some(clickhouse.clone())));
    tracing::info!("RPC clients initialized (Helius + Flux)");
    
    // ========== Helius API for token holders ==========
    
    let helius = Arc::new(RwLock::new(HeliusApi::with_clickhouse(rpc_url.clone(), Some(clickhouse.clone()))));
    
    // ========== Admin Password ==========
    
    let admin_password = env::var("ADMIN_PASSWORD")
        .expect("ADMIN_PASSWORD must be set - this is required for admin authentication");
    let admin_password_hash = admin_auth::hash_password(&admin_password)
        .expect("Failed to hash admin password");
    tracing::info!("Admin password hashed and ready");
    
    // ========== Application State ==========
    
    let state = Arc::new(AppState::new(
        admin_password_hash,
        clickhouse.clone(),
        postgres.clone(),
        rpc.clone(),
        helius.clone(),
    ));
    
    // ========== Background Tasks ==========
    
    // WebSocket manager for slot tracking
    let ws_manager = WebSocketManager::with_clickhouse(rpc_url.clone(), Some(clickhouse.clone()));
    
    // Slot subscription
    let slot_handle = ws_manager.spawn_slot_subscription(state.slot_cache.clone());
    tracing::info!("Slot subscription started");
    
    // Round broadcaster (sends to SSE every 500ms)
    let round_broadcast_handle = ws_manager.spawn_round_broadcaster(state.clone());
    tracing::info!("Round broadcaster started");
    
    // Program account subscription for SSE deployments
    let program_sub_state = state.clone();
    let program_sub_url = rpc_url.clone();
    let program_sub_clickhouse = Some(clickhouse.clone());
    let program_sub_handle = tokio::spawn(async move {
        loop {
            tracing::info!("Starting ORE program account subscription for SSE...");
            if let Err(e) = websocket::subscribe_to_program_accounts(&program_sub_url, program_sub_state.clone(), program_sub_clickhouse.clone()).await {
                tracing::error!("Program account subscription error: {}, reconnecting in 5s...", e);
            } else {
                tracing::warn!("Program account subscription ended unexpectedly, reconnecting...");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
    tracing::info!("Program account subscription started");
    
    // ORE token cache
    let token_cache = Arc::new(OreTokenCache::new(
        helius.clone(),
        state.ore_holders_cache.clone(),
        state.slot_cache.clone(),
    ));
    let token_cache_handle = token_cache.spawn_update_task();
    tracing::info!("ORE token cache started");
    
    // RPC polling task (Board, Treasury, Round every 2 seconds)
    let polling_handle = tasks::spawn_rpc_polling(state.clone());
    tracing::info!("RPC polling started");
    
    // Miners polling task (full load then incremental every 30 seconds)
    let miners_handle = tasks::spawn_miners_polling(state.clone());
    tracing::info!("Miners polling started");
    
    // Metrics snapshot task
    let metrics_handle = tasks::spawn_metrics_snapshot(state.clone());
    tracing::info!("Metrics snapshot task started");
    
    // EVORE accounts polling task
    let evore_handle = tasks::spawn_evore_polling(state.clone());
    tracing::info!("EVORE polling started");
    
    // Automation state reconstruction background task
    automation_states::spawn_automation_task(state.clone());
    tracing::info!("Automation state reconstruction task started");
    
    // Transaction parse queue background task
    automation_states::spawn_transaction_parse_task(state.clone());
    tracing::info!("Transaction parse queue task started");
    
    // Backfill action queue worker
    if let Err(e) = backfill::init_queue_worker(state.clone()).await {
        tracing::warn!("Failed to initialize queue worker: {}", e);
    } else {
        let queue_state = state.clone();
        tokio::spawn(async move {
            backfill::run_queue_worker(queue_state).await;
        });
        tracing::info!("Backfill action queue worker started");
    }
    
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
        .route("/live/deployments", get(routes::get_live_deployments))
        .route("/slot", get(routes::get_slot))
        
        // RPC proxy
        .route("/balance/{pubkey}", get(routes::get_balance))
        .route("/signature/{signature}", get(routes::get_signature_status))
        
        // ORE token balances
        .route("/ore-balance/{owner}", get(routes::get_ore_balance))
        .route("/ore-holders", get(routes::get_ore_holders))
        
        // EVORE accounts (Phase 1b)
        .nest("/evore", evore_routes::evore_router(state.clone()))
        
        // Historical data endpoints (Phase 3)
        .nest("/history", historical_routes::historical_router(state.clone()))
        
        // Metrics
        .route("/metrics", get(routes::get_metrics))
        
        // Historical rounds (from ClickHouse)
        .route("/rounds", get(routes::get_rounds))
        .route("/rounds/{round_id}", get(routes::get_round_by_id))
        
        // SSE streams
        .route("/sse/rounds", get(sse::sse_rounds))
        .route("/sse/deployments", get(sse::sse_deployments))
        
        // Admin routes (nested under /admin)
        .nest("/admin", admin_routes::admin_router(state.clone()))
        
        // Apply request logging middleware
        .layer(from_fn_with_state(state.clone(), middleware::request_logging_middleware))
        
        // State
        .with_state(state.clone());
        
        // Note: CORS is handled by nginx at the edge
        // ore-stats binds to localhost only, so no CORS needed here
    
    // ========== Start Server ==========
    
    let port: u16 = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse()
        .unwrap_or(3000);
    
    // Bind to localhost only - nginx will proxy external requests
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await?;
    
    // Cleanup (won't reach here normally)
    slot_handle.abort();
    round_broadcast_handle.abort();
    program_sub_handle.abort();
    token_cache_handle.abort();
    polling_handle.abort();
    miners_handle.abort();
    metrics_handle.abort();
    evore_handle.abort();

    Ok(())
}
