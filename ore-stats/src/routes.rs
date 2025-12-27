//! HTTP route handlers for ore-stats API
//!
//! All routes read from in-memory caches for fast responses.

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
};
use serde::{Deserialize, Serialize};
use steel::Pubkey;

use crate::app_state::AppState;

// ============================================================================
// Response Types
// ============================================================================

#[derive(Serialize)]
pub struct TreasuryResponse {
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
}

#[derive(Serialize)]
pub struct BoardResponse {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
}

#[derive(Serialize)]
pub struct RoundResponse {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub slots_remaining: i64,
    pub deployed: [u64; 25],
    pub count: [u64; 25],
    pub total_deployed: u64,
    pub unique_miners: u32,
}

#[derive(Serialize)]
pub struct MinerResponse {
    pub authority: String,
    pub round_id: u64,
    pub deployed: [u64; 25],
    pub total_deployed: u64,
    pub rewards_sol: u64,
    pub rewards_ore: u64,
    pub refined_ore: u64,
    pub lifetime_rewards_sol: u64,
    pub lifetime_rewards_ore: u64,
}

#[derive(Serialize)]
pub struct SlotResponse {
    pub slot: u64,
}

#[derive(Serialize)]
pub struct BalanceResponse {
    pub pubkey: String,
    pub lamports: u64,
}

#[derive(Serialize)]
pub struct OreBalanceResponse {
    pub owner: String,
    pub balance: u64,
}

#[derive(Serialize)]
pub struct SignatureStatusResponse {
    pub signature: String,
    pub slot: Option<u64>,
    pub confirmations: Option<usize>,
    pub status: Option<String>, // "confirmed", "finalized", "processed", or null
    pub err: Option<String>,
}

#[derive(Serialize)]
pub struct OreHoldersResponse {
    pub holders: Vec<OreHolderEntry>,
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
}

#[derive(Serialize)]
pub struct OreHolderEntry {
    pub owner: String,
    pub balance: u64,
}

#[derive(Serialize)]
pub struct MetricsResponse {
    pub uptime_seconds: u64,
    pub current_slot: u64,
    pub cache: CacheMetrics,
}

#[derive(Serialize)]
pub struct CacheMetrics {
    pub miners_count: usize,
    pub ore_holders_count: usize,
    pub round_id: u64,
}

#[derive(Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

// ============================================================================
// Query Parameters
// ============================================================================

#[derive(Deserialize)]
pub struct PaginationParams {
    pub page: Option<usize>,
    pub per_page: Option<usize>,
    pub min_balance: Option<u64>,
    pub sort_by_balance: Option<bool>,
}

#[derive(Deserialize)]
pub struct RoundsPaginationParams {
    /// Number of rounds per page (default 50, max 100)
    pub per_page: Option<usize>,
    /// Page number (1-based, for offset pagination)
    pub page: Option<usize>,
    /// Cursor: get rounds before this round_id (for cursor-based pagination)
    pub before: Option<u64>,
}

// ============================================================================
// Route Handlers
// ============================================================================

/// GET /treasury - Current treasury state
pub async fn get_treasury(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TreasuryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cache = state.treasury_cache.read().await;
    
    match cache.as_ref() {
        Some(treasury) => Ok(Json(TreasuryResponse {
            balance: treasury.balance,
            motherlode: treasury.motherlode,
            total_staked: treasury.total_staked,
            total_unclaimed: treasury.total_unclaimed,
            total_refined: treasury.total_refined,
        })),
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse { error: "Treasury data not yet available".to_string() }),
        )),
    }
}

/// GET /board - Current board state
pub async fn get_board(
    State(state): State<Arc<AppState>>,
) -> Result<Json<BoardResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cache = state.board_cache.read().await;
    
    match cache.as_ref() {
        Some(board) => Ok(Json(BoardResponse {
            round_id: board.round_id,
            start_slot: board.start_slot,
            end_slot: board.end_slot,
        })),
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse { error: "Board data not yet available".to_string() }),
        )),
    }
}

/// GET /round - Current round with live data
pub async fn get_round(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RoundResponse>, (StatusCode, Json<ErrorResponse>)> {
    let cache = state.round_cache.read().await;
    let current_slot = *state.slot_cache.read().await;
    
    match cache.as_ref() {
        Some(round) => {
            let mut response = RoundResponse {
                round_id: round.round_id,
                start_slot: round.start_slot,
                end_slot: round.end_slot,
                slots_remaining: round.end_slot.saturating_sub(current_slot) as i64,
                deployed: round.deployed,
                count: round.count,
                total_deployed: round.total_deployed,
                unique_miners: round.unique_miners,
            };
            // Recalculate slots remaining with latest slot
            if current_slot > round.end_slot {
                response.slots_remaining = 0;
            }
            Ok(Json(response))
        }
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse { error: "Round data not yet available".to_string() }),
        )),
    }
}

/// GET /slot - Current slot
pub async fn get_slot(
    State(state): State<Arc<AppState>>,
) -> Json<SlotResponse> {
    let slot = *state.slot_cache.read().await;
    Json(SlotResponse { slot })
}

/// GET /miner/{pubkey} - Single miner by authority
pub async fn get_miner(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<MinerResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate pubkey format
    let _ = pubkey.parse::<Pubkey>().map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid pubkey".to_string() }))
    })?;
    
    let cache = state.miners_cache.read().await;
    
    // Look up by String key (BTreeMap is keyed by authority string)
    match cache.get(&pubkey) {
        Some(miner) => {
            let total_deployed: u64 = miner.deployed.iter().sum();
            Ok(Json(MinerResponse {
                authority: miner.authority.to_string(),
                round_id: miner.round_id,
                deployed: miner.deployed,
                total_deployed,
                rewards_sol: miner.rewards_sol,
                rewards_ore: miner.rewards_ore,
                refined_ore: miner.refined_ore,
                lifetime_rewards_sol: miner.lifetime_rewards_sol,
                lifetime_rewards_ore: miner.lifetime_rewards_ore,
            }))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "Miner not found".to_string() }),
        )),
    }
}

/// GET /miners - All miners (paginated, sorted alphabetically by authority)
pub async fn get_miners(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Json<Vec<MinerResponse>> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(100);
    let offset = (page - 1) * per_page;
    
    let cache = state.miners_cache.read().await;
    
    // BTreeMap is already sorted by key (authority string), just paginate
    let miners: Vec<MinerResponse> = cache
        .values()
        .skip(offset)
        .take(per_page)
        .map(|miner| {
            let total_deployed: u64 = miner.deployed.iter().sum();
            MinerResponse {
                authority: miner.authority.to_string(),
                round_id: miner.round_id,
                deployed: miner.deployed,
                total_deployed,
                rewards_sol: miner.rewards_sol,
                rewards_ore: miner.rewards_ore,
                refined_ore: miner.refined_ore,
                lifetime_rewards_sol: miner.lifetime_rewards_sol,
                lifetime_rewards_ore: miner.lifetime_rewards_ore,
            }
        })
        .collect();
    
    Json(miners)
}

/// GET /balance/{pubkey} - SOL balance (RPC proxy)
pub async fn get_balance(
    State(state): State<Arc<AppState>>,
    Path(pubkey): Path<String>,
) -> Result<Json<BalanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pk = pubkey.parse::<Pubkey>().map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid pubkey".to_string() }))
    })?;
    
    match state.rpc.get_balance(&pk).await {
        Ok(lamports) => Ok(Json(BalanceResponse {
            pubkey,
            lamports,
        })),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("RPC error: {}", e) }),
        )),
    }
}

/// GET /signature/{signature} - Transaction signature status (RPC proxy)
pub async fn get_signature_status(
    State(state): State<Arc<AppState>>,
    Path(signature): Path<String>,
) -> Result<Json<SignatureStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Basic validation - signature should be base58 encoded, typically 87-88 chars
    if signature.len() < 80 || signature.len() > 100 {
        return Err((StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid signature format".to_string() })));
    }
    
    match state.rpc.get_signature_statuses(&[signature.clone()]).await {
        Ok(statuses) => {
            let status = statuses.into_iter().next().flatten();
            
            let (slot, confirmations, status_str, err) = match status {
                Some(s) => {
                    // Use confirmation_status from RPC if available, otherwise infer
                    let commitment = s.confirmation_status.or_else(|| {
                        if s.confirmations.is_none() {
                            Some("finalized".to_string())
                        } else if s.confirmations.unwrap_or(0) > 0 {
                            Some("confirmed".to_string())
                        } else {
                            Some("processed".to_string())
                        }
                    });
                    
                    (s.slot, s.confirmations, commitment, s.err)
                }
                None => (None, None, None, None),
            };
            
            Ok(Json(SignatureStatusResponse {
                signature,
                slot,
                confirmations,
                status: status_str,
                err,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("RPC error: {}", e) }),
        )),
    }
}

/// GET /ore-balance/{owner} - ORE token balance
pub async fn get_ore_balance(
    State(state): State<Arc<AppState>>,
    Path(owner): Path<String>,
) -> Result<Json<OreBalanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let pk = owner.parse::<Pubkey>().map_err(|_| {
        (StatusCode::BAD_REQUEST, Json(ErrorResponse { error: "Invalid pubkey".to_string() }))
    })?;
    
    let cache = state.ore_holders_cache.read().await;
    
    let balance = cache.get(&pk).copied().unwrap_or(0);
    
    Ok(Json(OreBalanceResponse {
        owner,
        balance,
    }))
}

/// GET /ore-holders - All ORE token holders (paginated)
pub async fn get_ore_holders(
    State(state): State<Arc<AppState>>,
    Query(params): Query<PaginationParams>,
) -> Json<OreHoldersResponse> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(100);
    let offset = (page - 1) * per_page;
    let min_balance = params.min_balance.unwrap_or(0);
    let sort_by_balance = params.sort_by_balance.unwrap_or(true);
    
    let cache = state.ore_holders_cache.read().await;
    
    let mut holders: Vec<_> = cache
        .iter()
        .filter(|(_, &balance)| balance >= min_balance)
        .map(|(owner, &balance)| OreHolderEntry {
            owner: owner.to_string(),
            balance,
        })
        .collect();
    
    let total = holders.len();
    
    if sort_by_balance {
        holders.sort_by(|a, b| b.balance.cmp(&a.balance));
    }
    
    let page_holders: Vec<_> = holders
        .into_iter()
        .skip(offset)
        .take(per_page)
        .collect();
    
    Json(OreHoldersResponse {
        holders: page_holders,
        total,
        page,
        per_page,
    })
}

/// GET /metrics - Public server metrics
pub async fn get_metrics(
    State(state): State<Arc<AppState>>,
) -> Json<MetricsResponse> {
    let miners_count = state.miners_cache.read().await.len();
    let ore_holders_count = state.ore_holders_cache.read().await.len();
    let current_slot = *state.slot_cache.read().await;
    let round_id = state.round_cache.read().await
        .as_ref()
        .map(|r| r.round_id)
        .unwrap_or(0);
    
    let uptime_seconds = state.uptime_seconds();
    
    Json(MetricsResponse {
        uptime_seconds,
        current_slot,
        cache: CacheMetrics {
            miners_count,
            ore_holders_count,
            round_id,
        },
    })
}

/// GET /live/round - Live round with unique miners (same as /round)
pub async fn get_live_round(
    State(state): State<Arc<AppState>>,
) -> Result<Json<RoundResponse>, (StatusCode, Json<ErrorResponse>)> {
    get_round(State(state)).await
}

/// Health check
pub async fn health() -> &'static str {
    "OK"
}

// ============================================================================
// Historical Data Endpoints
// ============================================================================

/// GET /rounds - Historical rounds with pagination
/// 
/// Query params:
/// - `per_page`: Number of rounds per page (default 50, max 100)
/// - `page`: Page number (1-based) for offset pagination
/// - `before`: Round ID cursor - get rounds before this ID (more efficient for deep pagination)
/// 
/// Use cursor-based (`before`) for infinite scroll or deep pagination.
/// Use page-based (`page`) for traditional pagination with page numbers.
pub async fn get_rounds(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RoundsPaginationParams>,
) -> Result<Json<RoundsListResponse>, (StatusCode, Json<ErrorResponse>)> {
    let per_page = params.per_page.unwrap_or(50).min(100);
    let limit = per_page as u32;
    
    // Determine pagination mode
    let (before_round_id, offset) = if let Some(before) = params.before {
        // Cursor-based pagination
        (Some(before), None)
    } else if let Some(page) = params.page {
        // Offset-based pagination (page 1 = offset 0)
        let page = page.max(1);
        (None, Some(((page - 1) * per_page) as u32))
    } else {
        // No pagination, get latest
        (None, None)
    };
    
    match state.clickhouse.get_rounds_paginated(before_round_id, offset, limit).await {
        Ok((rounds, has_more)) => {
            // Get next cursor (last round_id in results)
            let next_cursor = if has_more {
                rounds.last().map(|r| r.round_id)
            } else {
                None
            };
            
            Ok(Json(RoundsListResponse {
                rounds: rounds.into_iter().map(|r| RoundSummary {
                    round_id: r.round_id,
                    start_slot: r.start_slot,
                    end_slot: r.end_slot,
                    winning_square: r.winning_square,
                    top_miner: r.top_miner,
                    total_deployed: r.total_deployed,
                    total_winnings: r.total_winnings,
                    unique_miners: r.unique_miners,
                    motherlode: r.motherlode,
                    motherlode_hit: r.motherlode_hit > 0,
                }).collect(),
                has_more,
                next_cursor,
                page: params.page,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Database error: {}", e) }),
        )),
    }
}

/// GET /rounds/{round_id} - Single historical round with full details
pub async fn get_round_by_id(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<RoundDetailResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get round
    let round = match state.clickhouse.get_round_by_id(round_id).await {
        Ok(Some(r)) => r,
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse { error: "Round not found".to_string() }),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Database error: {}", e) }),
        )),
    };
    
    // Get deployments
    let deployments = match state.clickhouse.get_deployments_for_round(round_id).await {
        Ok(d) => d,
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse { error: format!("Database error: {}", e) }),
        )),
    };
    
    Ok(Json(RoundDetailResponse {
        round_id: round.round_id,
        start_slot: round.start_slot,
        end_slot: round.end_slot,
        winning_square: round.winning_square,
        top_miner: round.top_miner,
        top_miner_reward: round.top_miner_reward,
        total_deployed: round.total_deployed,
        total_vaulted: round.total_vaulted,
        total_winnings: round.total_winnings,
        unique_miners: round.unique_miners,
        motherlode: round.motherlode,
        motherlode_hit: round.motherlode_hit > 0,
        source: round.source,
        deployments: deployments.into_iter().map(|d| DeploymentSummary {
            miner_pubkey: d.miner_pubkey,
            square_id: d.square_id,
            amount: d.amount,
            deployed_slot: d.deployed_slot,
            sol_earned: d.sol_earned,
            ore_earned: d.ore_earned,
            is_winner: d.is_winner > 0,
            is_top_miner: d.is_top_miner > 0,
        }).collect(),
    }))
}

#[derive(Serialize)]
pub struct RoundsListResponse {
    pub rounds: Vec<RoundSummary>,
    /// Whether there are more rounds available
    pub has_more: bool,
    /// Cursor for next page (use as `before` param)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<u64>,
    /// Current page number (if using page-based pagination)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<usize>,
}

#[derive(Serialize)]
pub struct RoundSummary {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub total_deployed: u64,
    pub total_winnings: u64,
    pub unique_miners: u32,
    pub motherlode: u64,
    pub motherlode_hit: bool,
}

#[derive(Serialize)]
pub struct RoundDetailResponse {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub top_miner_reward: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub unique_miners: u32,
    pub motherlode: u64,
    pub motherlode_hit: bool,
    pub source: String,
    pub deployments: Vec<DeploymentSummary>,
}

#[derive(Serialize)]
pub struct DeploymentSummary {
    pub miner_pubkey: String,
    pub square_id: u8,
    pub amount: u64,
    pub deployed_slot: u64,
    pub sol_earned: u64,
    pub ore_earned: u64,
    pub is_winner: bool,
    pub is_top_miner: bool,
}

