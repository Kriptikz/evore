//! EVORE Account API Routes (Phase 1b)
//!
//! Endpoints for reading EVORE program accounts (Managers, Deployers)
//! Note: Auth balances are NOT cached - frontend fetches them manually via /balance/{pubkey}
//! Note: refined_ore is already calculated when miners are cached,
//! so no additional calculation is needed when serving data.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::app_state::AppState;
use crate::evore_cache::{
    AutoMinerInfo, CachedDeployer, CachedManager, EvoreCacheStats, MinerInfo,
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
// Combined Endpoint
// ============================================================================

/// GET /evore/my-miners/{authority} - Full data for all user's AutoMiners
/// Note: Auth balances are NOT included - frontend fetches them manually via /balance/{pubkey}
/// 
/// We check auth_id 0, 1, 2, 3 at minimum for each manager, then continue checking
/// higher auth_ids if we find a miner at auth_id 3 (some users start at 0, others at 1).
async fn get_my_miners(
    State(state): State<Arc<AppState>>,
    Path(authority): Path<String>,
) -> Json<MyMinersResponse> {
    use crate::evore_cache::managed_miner_auth_pda;
    use steel::Pubkey;
    
    let evore_cache = state.evore_cache.read().await;
    let miners_cache = state.miners_cache.read().await;
    
    let managers = evore_cache.get_managers_by_authority(&authority);
    
    let mut autominers: Vec<AutoMinerInfo> = Vec::new();
    
    for manager in managers {
        let deployer = evore_cache.get_deployer_for_manager(&manager.address).cloned();
        
        // Parse manager pubkey
        let manager_pubkey = match Pubkey::try_from(manager.address.as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                // Can't parse manager pubkey, add without miners
                autominers.push(AutoMinerInfo {
                    manager: (*manager).clone(),
                    deployer,
                    miners: Vec::new(),
                });
                continue;
            }
        };
        
        // Check auth_ids 0, 1, 2, 3 at minimum, then keep going if we find one at 3
        let mut found_miners: Vec<(u64, MinerInfo)> = Vec::new();
        let mut auth_id: u64 = 0;
        let min_check = 4; // Always check 0, 1, 2, 3
        
        loop {
            let (auth_pda, _) = managed_miner_auth_pda(&manager_pubkey, auth_id);
            let auth_pda_str = auth_pda.to_string();
            
            // Check if this auth PDA has a miner in the cache
            // The miners_cache is keyed by the miner's authority, which IS the auth PDA
            // Note: refined_ore is already accurate - calculated when miner was cached
            if let Some(miner) = miners_cache.get(&auth_pda_str) {
                found_miners.push((auth_id, MinerInfo {
                    address: auth_pda_str,
                    auth_id,
                    round_id: miner.round_id,
                    checkpoint_id: miner.checkpoint_id,
                    deployed: miner.deployed,
                    rewards_sol: miner.rewards_sol,
                    rewards_ore: miner.rewards_ore,
                    refined_ore: miner.refined_ore,
                }));
            }
            
            auth_id += 1;
            
            // If we've checked up to min_check and haven't found any beyond 3, stop
            // If we found one at auth_id 3 (index 3), keep checking
            if auth_id >= min_check {
                // Check if we found a miner at the previous auth_id
                let found_at_prev = found_miners.iter().any(|(id, _)| *id == auth_id - 1);
                if !found_at_prev {
                    break;
                }
            }
            
            // Safety limit - don't check more than 100 auth_ids
            if auth_id > 100 {
                break;
            }
        }
        
        // Create AutoMinerInfo with all found miners
        let miners: Vec<MinerInfo> = found_miners.into_iter().map(|(_, m)| m).collect();
        
        autominers.push(AutoMinerInfo {
            manager: (*manager).clone(),
            deployer,
            miners,
        });
    }
    
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

