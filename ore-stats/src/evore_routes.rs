//! EVORE Account API Routes (Phase 1b)
//!
//! Endpoints for reading EVORE program accounts (Managers, Deployers, Auth balances)

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::evore_cache::{
    AutoMinerInfo, CachedAuthBalance, CachedDeployer, CachedManager, EvoreCacheStats, MinerInfo,
};

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

// ============================================================================
// Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ManagersResponse {
    pub managers: Vec<CachedManager>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct DeployersResponse {
    pub deployers: Vec<CachedDeployer>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct MyMinersResponse {
    pub authority: String,
    pub autominers: Vec<AutoMinerInfo>,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Router
// ============================================================================

pub fn evore_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        // Manager endpoints
        .route("/managers", get(get_all_managers))
        .route("/managers/{pubkey}", get(get_manager))
        .route("/managers/by-authority/{pubkey}", get(get_managers_by_authority))
        
        // Deployer endpoints
        .route("/deployers", get(get_all_deployers))
        .route("/deployers/{pubkey}", get(get_deployer))
        .route("/deployers/by-manager/{pubkey}", get(get_deployer_by_manager))
        .route("/deployers/by-authority/{pubkey}", get(get_deployers_by_authority))
        
        // Auth balance endpoints
        .route("/auth-balance/{manager}/{auth_id}", get(get_auth_balance))
        .route("/auth-balances/{pubkey}", get(get_auth_balances_by_authority))
        
        // Combined endpoint for frontend optimization
        .route("/my-miners/{authority}", get(get_my_miners))
        
        // Cache stats
        .route("/stats", get(get_evore_stats))
        
        .with_state(state)
}

// ============================================================================
// Manager Handlers
// ============================================================================

/// GET /evore/managers - All managers (paginated)
async fn get_all_managers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationQuery>,
) -> Json<ManagersResponse> {
    let cache = state.evore_cache.read().await;
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);
    
    let managers: Vec<CachedManager> = cache.managers
        .values()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    
    Json(ManagersResponse {
        total: cache.managers.len(),
        managers,
    })
}

/// GET /evore/managers/{pubkey} - Single manager by address
async fn get_manager(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<CachedManager>, Json<ErrorResponse>> {
    let cache = state.evore_cache.read().await;
    
    cache.managers
        .get(&pubkey)
        .cloned()
        .map(Json)
        .ok_or_else(|| Json(ErrorResponse { error: "Manager not found".to_string() }))
}

/// GET /evore/managers/by-authority/{pubkey} - Managers owned by authority
async fn get_managers_by_authority(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Json<ManagersResponse> {
    let cache = state.evore_cache.read().await;
    
    let managers: Vec<CachedManager> = cache.get_managers_by_authority(&pubkey)
        .into_iter()
        .cloned()
        .collect();
    
    Json(ManagersResponse {
        total: managers.len(),
        managers,
    })
}

// ============================================================================
// Deployer Handlers
// ============================================================================

/// GET /evore/deployers - All deployers (paginated)
async fn get_all_deployers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationQuery>,
) -> Json<DeployersResponse> {
    let cache = state.evore_cache.read().await;
    let limit = params.limit.unwrap_or(100).min(1000);
    let offset = params.offset.unwrap_or(0);
    
    let deployers: Vec<CachedDeployer> = cache.deployers
        .values()
        .skip(offset)
        .take(limit)
        .cloned()
        .collect();
    
    Json(DeployersResponse {
        total: cache.deployers.len(),
        deployers,
    })
}

/// GET /evore/deployers/{pubkey} - Single deployer by address
async fn get_deployer(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<CachedDeployer>, Json<ErrorResponse>> {
    let cache = state.evore_cache.read().await;
    
    cache.deployers
        .get(&pubkey)
        .cloned()
        .map(Json)
        .ok_or_else(|| Json(ErrorResponse { error: "Deployer not found".to_string() }))
}

/// GET /evore/deployers/by-manager/{pubkey} - Deployer for a manager
async fn get_deployer_by_manager(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<CachedDeployer>, Json<ErrorResponse>> {
    let cache = state.evore_cache.read().await;
    
    cache.get_deployer_for_manager(&pubkey)
        .cloned()
        .map(Json)
        .ok_or_else(|| Json(ErrorResponse { error: "Deployer not found for manager".to_string() }))
}

/// GET /evore/deployers/by-authority/{pubkey} - Deployers for authority's managers
async fn get_deployers_by_authority(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Json<DeployersResponse> {
    let cache = state.evore_cache.read().await;
    
    // Get all managers for this authority, then get their deployers
    let deployers: Vec<CachedDeployer> = cache.get_managers_by_authority(&pubkey)
        .iter()
        .filter_map(|manager| cache.get_deployer_for_manager(&manager.address))
        .cloned()
        .collect();
    
    Json(DeployersResponse {
        total: deployers.len(),
        deployers,
    })
}

// ============================================================================
// Auth Balance Handlers
// ============================================================================

/// GET /evore/auth-balance/{manager}/{auth_id} - Balance of ManagedMinerAuth PDA
async fn get_auth_balance(
    State(state): State<Arc<AppState>>,
    Path((manager, auth_id)): Path<(String, u64)>,
) -> Result<Json<CachedAuthBalance>, Json<ErrorResponse>> {
    let cache = state.evore_cache.read().await;
    
    // For now, we only support auth_id = 0
    if auth_id != 0 {
        return Err(Json(ErrorResponse { error: "Only auth_id 0 is supported".to_string() }));
    }
    
    cache.get_auth_balance_for_manager(&manager)
        .cloned()
        .map(Json)
        .ok_or_else(|| Json(ErrorResponse { error: "Auth balance not found".to_string() }))
}

/// GET /evore/auth-balances/{pubkey} - All auth balances for authority's managers
async fn get_auth_balances_by_authority(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Json<Vec<CachedAuthBalance>> {
    let cache = state.evore_cache.read().await;
    
    let balances: Vec<CachedAuthBalance> = cache.get_managers_by_authority(&pubkey)
        .iter()
        .filter_map(|manager| cache.get_auth_balance_for_manager(&manager.address))
        .cloned()
        .collect();
    
    Json(balances)
}

// ============================================================================
// Combined Endpoint
// ============================================================================

/// GET /evore/my-miners/{authority} - Full data for all user's AutoMiners
async fn get_my_miners(
    State(state): State<Arc<AppState>>,
    Path(authority): Path<String>,
) -> Json<MyMinersResponse> {
    let evore_cache = state.evore_cache.read().await;
    let miners_cache = state.miners_cache.read().await;
    
    let managers = evore_cache.get_managers_by_authority(&authority);
    
    let autominers: Vec<AutoMinerInfo> = managers
        .iter()
        .map(|manager| {
            let deployer = evore_cache.get_deployer_for_manager(&manager.address).cloned();
            let auth_balance = evore_cache.get_auth_balance_for_manager(&manager.address).cloned();
            
            // Try to find the linked ORE miner
            // The miner authority is the ManagedMinerAuth PDA
            let miner = auth_balance.as_ref().and_then(|auth| {
                miners_cache.get(&auth.address).map(|m| MinerInfo {
                    address: auth.address.clone(),
                    round_id: m.round_id,
                    checkpoint_id: m.checkpoint_id,
                    deployed: m.deployed,
                    rewards_sol: m.rewards_sol,
                    rewards_ore: m.rewards_ore,
                    refined_ore: m.refined_ore,
                })
            });
            
            AutoMinerInfo {
                manager: (*manager).clone(),
                deployer,
                auth_balance,
                miner,
            }
        })
        .collect();
    
    Json(MyMinersResponse {
        authority,
        autominers,
    })
}

// ============================================================================
// Stats
// ============================================================================

/// GET /evore/stats - Cache statistics
async fn get_evore_stats(
    State(state): State<Arc<AppState>>,
) -> Json<EvoreCacheStats> {
    let cache = state.evore_cache.read().await;
    Json(cache.stats())
}

