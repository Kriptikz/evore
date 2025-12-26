use std::{collections::HashMap, convert::Infallible, env, str::FromStr, sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

use anyhow::{anyhow, bail};
use sqlx::{sqlite::SqliteConnectOptions, Pool, Sqlite};
use thiserror::Error;
use axum::{body::Body, extract::{Path, Query, State}, http::{Request, Response, StatusCode}, middleware::{self, Next}, response::{sse, Sse}, routing::get, Json, Router};
use const_crypto::ed25519;
use evore::ore_api::{BOARD, ROUND, TREASURY_ADDRESS, round_pda, Board, Miner, Round, Treasury};
use serde::{Deserialize, Serialize};
use solana_account_decoder_client_types::UiAccountEncoding;
use solana_client::{nonblocking::rpc_client::RpcClient, rpc_filter::RpcFilterType};
use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
use steel::{AccountDeserialize, Pubkey};
use tokio::{signal, sync::{broadcast, RwLock}};
use tokio_stream::Stream;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{app_state::{AppBoard, AppLiveDeployment, AppMiner, AppRound, AppState, AppTreasury, LiveBroadcastData}, database::{get_deployments_by_round, DbMinerSnapshot, DbTreasury, GetDeployment, MinerLeaderboardRow, MinerOreLeaderboardRow, MinerTotalsRow, RoundRow}, rpc::{infer_refined_ore, update_data_system, watch_live_board}};

/// Program id for const pda derivations
const PROGRAM_ID_BYTES: [u8; 32] = evore::ore_api::PROGRAM_ID.to_bytes();


/// The address of the board account.
pub const BOARD_ADDRESS: Pubkey =
    Pubkey::new_from_array(ed25519::derive_program_address(&[BOARD], &PROGRAM_ID_BYTES).0);

/// The address of the square account.
pub const ROUND_ADDRESS: Pubkey =
    Pubkey::new_from_array(ed25519::derive_program_address(&[ROUND], &PROGRAM_ID_BYTES).0);

pub mod app_state;
pub mod rpc;
pub mod database;
pub mod entropy_api;
pub mod account_tracker;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().expect("Failed to load env");

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(env_filter)
        .init();

    let db_url = env::var("DATABASE_URL").unwrap_or_else(|_| "data/app.db".to_string());
    if let Some(parent) = std::path::Path::new(&db_url).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let db_connect_ops = SqliteConnectOptions::from_str(&db_url)?
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .pragma("cache_size", "-200000") // Set cache to ~200MB (200,000KB)
        .pragma("temp_store", "memory") // Store temporary data in memory
        .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(400))
        .foreign_keys(true);

    let db_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(1000))
        .connect_with(db_connect_ops)
        .await?;

    tracing::info!("Running optimize...");
    sqlx::query("PRAGMA optimize").execute(&db_pool).await?;
    tracing::info!("Optimize complete!");



    tracing::info!("Running migrations...");

    sqlx::migrate!("./migrations").run(&db_pool).await?;

    tracing::info!("Database migrations complete.");
    tracing::info!("Database ready!");

    let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let prefix = "https://".to_string();
    let connection = RpcClient::new_with_commitment(prefix + &rpc_url, CommitmentConfig { commitment: CommitmentLevel::Confirmed });

    let treasury = if let Ok(treasury) = connection.get_account_data(&TREASURY_ADDRESS).await {
        if let Ok(treasury) = Treasury::try_from_bytes(&treasury) {
            treasury.clone()
        } else {
            bail!("Failed to parse Treasury account");
        }
    } else {
        bail!("Failed to load treasury account data");
    };

    // Sleep between RPC Calls
    tokio::time::sleep(Duration::from_secs(1)).await;

    let board = if let Ok(board) = connection.get_account_data(&BOARD_ADDRESS).await {
        if let Ok(board) = Board::try_from_bytes(&board) {
            board.clone()
        } else {
            bail!("Failed to parse Board account");
        }
    } else {
        bail!("Failed to load board account data");
    };
    tokio::time::sleep(Duration::from_secs(1)).await;

    let round = if let Ok(round) = connection.get_account_data(&round_pda(board.round_id).0).await {
        if let Ok(round) = Round::try_from_bytes(&round) {
            round.clone()
        } else {
            bail!("Failed to parse Round account");
        }
    } else {
        bail!("Failed to load round account data");
    };
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut miners = vec![];
    if let Ok(miners_data_raw) = connection.get_program_accounts_with_config(
        &evore::ore_api::id(),
        solana_client::rpc_config::RpcProgramAccountsConfig { 
            filters: Some(vec![RpcFilterType::DataSize(size_of::<Miner>() as u64 + 8)]),
            account_config: solana_client::rpc_config::RpcAccountInfoConfig {
                encoding: Some(UiAccountEncoding::Base64),
                data_slice: None,
                commitment: Some(CommitmentConfig { commitment: CommitmentLevel::Confirmed }),
                min_context_slot: None,
            },
            with_context: None,
            sort_results: None
        } 
    ).await {
        for miner_data in miners_data_raw {
            if let Ok(miner) = Miner::try_from_bytes(&miner_data.1.data) {
                let mut miner = miner.clone();
                miner.refined_ore = infer_refined_ore(&miner, &treasury);
                miners.push(miner.clone().into());
            }
        }
    }

    let (live_broadcaster, _rx) = broadcast::channel(1000);


    let app_state = AppState {
        treasury: Arc::new(RwLock::new(treasury.into())),
        board: Arc::new(RwLock::new(board.into())),
        staring_round: board.round_id,
        rounds: Arc::new(RwLock::new(vec![])),
        miners: Arc::new(RwLock::new(miners)),
        live_data_broadcaster: live_broadcaster,
        live_round: Arc::new(RwLock::new(AppRound::from(round))),
        live_deployments: Arc::new(RwLock::new(vec![])),
        deployments_cache: Arc::new(RwLock::new(app_state::DeploymentsCache { item: HashMap::new() })),
        db_pool,
    };

    let s = app_state.clone();
    update_data_system(connection, s).await;

    let s = app_state.clone();
    watch_live_board(&rpc_url, s).await;

    let state = app_state.clone();

    let app = Router::new()
        .route("/", get(root))
        .route("/treasury", get(get_treasury))
        .route("/board", get(get_board))
        .route("/round", get(get_round))
        .route("/round/{round_id}", get(get_round_by_id))
        .route("/miners", get(get_miners))
        .route("/deployments", get(get_deployments_old))
        .route("/v2/deployments", get(get_deployments))
        .route("/rounds", get(get_rounds))
        .route("/v2/rounds", get(v2_get_rounds))
        .route("/treasuries", get(get_treasuries))
        .route("/search/pubkey/{letters}", get(get_available_pubkeys))
        .route("/miner/latest/{pubkey}", get(get_miner_latest))
        .route("/miner/snapshot/{pubkey}", get(get_miner_snapshot))
        .route("/miner/{pubkey}", get(get_miner_history))
        .route("/miner/rounds/{pubkey}", get(get_miner_rounds))
        .route("/v2/miner/rounds/{pubkey}", get(get_miner_rounds_v2))
        .route("/miner/stats/{pubkey}", get(get_miner_stats))
        .route("/miner/totals", get(get_miner_totals))
        .route("/miner/totals/ore", get(get_miner_totals_ore))
        .route("/leaderboard", get(get_leaderboard))
        .route("/leaderboard/ore", get(get_leaderboard_ore))
        .route("/leaderboard/latest-rounds", get(get_leaderboard_latest_rounds))
        .route("/leaderboard/latest-rounds/ore", get(get_leaderboard_latest_rounds_ore))
        .route("/leaderboard/all-time", get(get_leaderboard_all_time))
        .route("/leaderboard/all-time/ore", get(get_leaderboard_all_time_ore))
        .route("/sse", get(sse_handler))
        .route("/sse/deployments", get(sse_deployments_handler))
        .route("/sse/rounds", get(sse_rounds_handler))
        .route("/live/round", get(get_live_round))
        .route("/live/deployments", get(get_live_deployments))
        .layer(middleware::from_fn(log_request_time))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await?;

    tracing::debug!("Listening on {}", listener.local_addr()?);

    //axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    axum::serve(listener, app).await?;

    Ok(())
}


async fn log_request_time(
    req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, StatusCode> {
    let start_time = Instant::now();
    let method = req.method().to_string();
    let uri = req.uri().to_string();

    let headers = req.headers();

    let forwarded_for = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let response = next.run(req).await;

    let duration = start_time.elapsed();
    tracing::info!(
        "Client IP: {} - Request: {} {} - - Duration: {:?}",
        forwarded_for,
        method,
        uri,
        duration
    );

    Ok(response)
}

async fn root() -> &'static str {
    "ORE"
}

#[derive(Debug, Deserialize)]
struct MinersPagination {
    limit: Option<i64>,
    offset: Option<i64>,
    order_by: Option<String>,
}

async fn get_miners(
    State(state): State<AppState>,
    Query(p): Query<MinersPagination>,
) -> Result<Json<Vec<AppMiner>>, AppError> {
    let limit = p.limit.unwrap_or(2500).max(1).min(2500) as usize;
    let offset = p.offset.unwrap_or(0).max(0) as usize;
    let miners = state.miners.clone();
    let reader = miners.read().await;
    let mut miners = reader.clone();
    drop(reader);
    if miners.len() > 0 {
        match p.order_by {
            Some(v) => {
                if v.eq("unclaimed_sol") {
                    miners.sort_by(|a, b| b.rewards_sol.partial_cmp(&a.rewards_sol).unwrap());
                } else if v.eq("unclaimed_ore") {
                    miners.sort_by(|a, b| b.rewards_ore.partial_cmp(&a.rewards_ore).unwrap());
                } else if v.eq("refined_ore") {
                    miners.sort_by(|a, b| b.refined_ore.partial_cmp(&a.refined_ore).unwrap());
                } else if v.eq("total_deployed") {
                    miners.sort_by(|a, b| b.total_deployed.partial_cmp(&a.total_deployed).unwrap());
                } else if v.eq("round_id") {
                    miners.sort_by(|a, b| b.round_id.partial_cmp(&a.round_id).unwrap());
                }
            },
            None => {
                // No ordering
            }
        }
        let start = offset.min(miners.len() - 2);
        let end = start + limit.min(miners.len() - 1 - start);
        return Ok(Json(miners[start..end].to_vec()));
    }
    Ok(Json(miners))
}

async fn get_treasury(
    State(state): State<AppState>,
) -> Result<Json<AppTreasury>, AppError> {
    let r = state.treasury.clone();
    let lock = r.read().await;
    let data = lock.clone();
    Ok(Json(data))
}


async fn get_board(
    State(state): State<AppState>,
) -> Result<Json<AppBoard>, AppError> {
    let r = state.board.clone();
    let lock = r.read().await;
    let data = lock.clone();
    Ok(Json(data))
}

async fn get_round(
    State(state): State<AppState>,
) -> Result<Json<AppRound>, AppError> {
    let r = state.rounds.clone();
    let lock = r.read().await;
    let data = lock.clone();
    drop(lock);
    if let Some(d) = data.last() {
        Ok(Json(d.clone()))
    } else {
        Err(anyhow!("Failed to get last round").into())
    }
}

async fn get_round_by_id(
    Path(p): Path<i64>,
    State(state): State<AppState>,
) -> Result<Json<Vec<RoundRow>>, AppError> {
    let round = database::get_round_by_id(&state.db_pool, p).await?;

    Ok(Json(round))
}

#[derive(Debug, Deserialize)]
struct RoundsPagination {
    limit: Option<i64>,
    offset: Option<i64>,
    ml: Option<bool>
}

async fn get_rounds(
    State(state): State<AppState>,
    Query(p): Query<RoundsPagination>,
) -> Result<Json<Vec<RoundRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).max(1).min(2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rounds = database::get_rounds(&state.db_pool, limit, offset, p.ml).await?;
    Ok(Json(rounds))
}

#[derive(Debug, Deserialize)]
struct V2RoundsPagination {
    limit: Option<i64>,
    round_id: Option<i64>,
    ml: Option<bool>
}

async fn v2_get_rounds(
    State(state): State<AppState>,
    Query(p): Query<V2RoundsPagination>,
) -> Result<Json<Vec<RoundRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).max(1).min(2000);
    if let Some(rid) = p.round_id {
        let rounds = database::get_rounds_via_cursor(&state.db_pool, limit, rid, p.ml).await?;
        Ok(Json(rounds))
    } else {
        let rounds = database::get_rounds(&state.db_pool, limit, 0, p.ml).await?;
        Ok(Json(rounds))
    }
}

async fn get_treasuries(
    State(state): State<AppState>,
    Query(p): Query<RoundsPagination>,
) -> Result<Json<Vec<DbTreasury>>, AppError> {
    let limit = p.limit.unwrap_or(2000).max(1).min(2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let treasuries = database::get_treasuries(&state.db_pool, limit, offset).await?;
    Ok(Json(treasuries))
}

async fn get_miner_history(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
    Query(p): Query<RoundsPagination>,
) -> Result<Json<Vec<DbMinerSnapshot>>, AppError> {
    let limit = p.limit.unwrap_or(1200).max(1).min(2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let miners_history = database::get_miner_snapshots(&state.db_pool, pubkey, limit, offset).await?;
    Ok(Json(miners_history))
}

async fn get_miner_rounds(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
    Query(p): Query<RoundsPagination>,
) -> Result<Json<Vec<RoundRow>>, AppError> {
    let limit = p.limit.unwrap_or(10).max(1).min(100);
    let offset = p.offset.unwrap_or(0).max(0);
    let rounds = database::get_miner_rounds(&state.db_pool, pubkey, limit, offset).await?;
    Ok(Json(rounds))
}

async fn get_miner_rounds_v2(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
    Query(p): Query<V2RoundsPagination>,
) -> Result<Json<Vec<RoundRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).max(1).min(100);
    if let Some(rid) = p.round_id {
        let rounds = database::get_miner_rounds_via_cursor(&state.db_pool, pubkey, limit, rid).await?;
        Ok(Json(rounds))
    } else {
        let rounds = database::get_miner_rounds(&state.db_pool, pubkey, limit, 0).await?;
        Ok(Json(rounds))
    }
}

#[derive(Debug, Deserialize)]
pub struct RoundId {
    pub round_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDeploymentSquished {
    pub round_id: u64,
    pub pubkey: String,
    pub deployments: [u64; 25],
    pub sol_deployed: u64,
    pub sol_earned: u64,
    pub ore_earned: u64,
}

pub async fn get_deployments_old(
    State(state): State<AppState>,
    Query(p): Query<RoundId>,
) -> Result<Json<Vec<GetDeployment>>, AppError> {
    let deployments = get_deployments_by_round(&state.db_pool, p.round_id as i64).await?;
    Ok(Json(deployments))
}

pub async fn get_deployments(
    State(state): State<AppState>,
    Query(p): Query<RoundId>,
) -> Result<Json<Vec<GetDeploymentSquished>>, AppError> {
    let reader = state.deployments_cache.read().await;
    let dc = reader.clone();
    drop(reader);

    if let Some(data) = dc.item.get(&p.round_id) {
        return Ok(Json(data.0.to_vec()))
    } else {
        let rounds = database::get_rounds(&state.db_pool, 1, 0, None).await;
        match rounds {
            Ok(rs) => {
                let latest_round = rs[0].clone();
                if (latest_round.id as u64 - p.round_id) > 10 {
                    let deployments = get_deployments_by_round(&state.db_pool, p.round_id as i64).await?;

                    // group + squish
                    let mut by_pubkey: HashMap<String, GetDeploymentSquished> = HashMap::new();

                    for d in deployments {
                        // make sure there's an entry for this pubkey
                        let entry = by_pubkey.entry(d.pubkey.clone()).or_insert_with(|| GetDeploymentSquished {
                            round_id: d.round_id as u64,
                            pubkey: d.pubkey.clone(),
                            deployments: [0u64; 25],
                            sol_deployed: 0,
                            sol_earned: 0,
                            ore_earned: 0,
                        });

                        // place this deployment's amount into the correct slot
                        // assuming square_id is 0..24
                        let idx = d.square_id as usize;
                        if idx < 25 {
                            entry.deployments[idx] = d.amount as u64;
                        }
                        // accumulate totals
                        entry.sol_deployed += d.amount as u64;
                        entry.sol_earned += d.sol_earned as u64;
                        entry.ore_earned += d.ore_earned as u64;
                    }

                    // turn the map into a vec
                    let squished: Vec<GetDeploymentSquished> = by_pubkey.into_values().collect();
                    Ok(Json(squished))
                } else {
                    let deployments = get_deployments_by_round(&state.db_pool, p.round_id as i64).await?;

                    // group + squish
                    let mut by_pubkey: HashMap<String, GetDeploymentSquished> = HashMap::new();

                    for d in deployments {
                        // make sure there's an entry for this pubkey
                        let entry = by_pubkey.entry(d.pubkey.clone()).or_insert_with(|| GetDeploymentSquished {
                            round_id: d.round_id as u64,
                            pubkey: d.pubkey.clone(),
                            deployments: [0u64; 25],
                            sol_deployed: 0,
                            sol_earned: 0,
                            ore_earned: 0,
                        });

                        // place this deployment's amount into the correct slot
                        // assuming square_id is 0..24
                        let idx = d.square_id as usize;
                        if idx < 25 {
                            entry.deployments[idx] = d.amount as u64;
                        }
                        // accumulate totals
                        entry.sol_deployed += d.amount as u64;
                        entry.sol_earned += d.sol_earned as u64;
                        entry.ore_earned += d.ore_earned as u64;
                    }

                    // turn the map into a vec
                    let squished: Vec<GetDeploymentSquished> = by_pubkey.into_values().collect();
                    let mut w = state.deployments_cache.write().await;
                    if w.item.len() < 10 {
                        w.item.insert(p.round_id, (squished.clone(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::from_secs(0)).as_secs()));
                    } else {
                        tracing::warn!("Deployments cache max length reached, clearing cache...");
                        w.item = HashMap::new();
                        w.item.insert(p.round_id, (squished.clone(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_else(|_| Duration::from_secs(0)).as_secs()));
                    }
                    drop(w);

                    Ok(Json(squished))
                }
            }
            Err(e) => {
               return Err(AppError::Sqlx(e))
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct Pagination {
    limit: Option<i64>,
    offset: Option<i64>,
}

async fn get_miner_totals(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerTotalsRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rows = database::get_miner_totals_all_time(&state.db_pool, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard_all_time(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerTotalsRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rows = database::get_miner_totals_all_time_v2(&state.db_pool, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerLeaderboardRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rounds = 60;
    let rows = database::get_leaderboard_last_n_rounds(&state.db_pool, rounds, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard_latest_rounds(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerLeaderboardRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rounds = 60;
    let rows = database::get_leaderboard_last_n_rounds_v2(&state.db_pool, rounds, limit, offset).await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct OreLeaderboardQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    //rounds: Option<i64>, // if present, use "Last X rounds"; else All Time
}

async fn get_miner_totals_ore(
    State(state): State<AppState>,
    Query(q): Query<OreLeaderboardQuery>,
) -> Result<Json<Vec<MinerOreLeaderboardRow>>, AppError> {
    let limit  = q.limit.unwrap_or(100).clamp(1, 2000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows =  database::get_ore_leaderboard_all_time(&state.db_pool, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard_all_time_ore(
    State(state): State<AppState>,
    Query(q): Query<OreLeaderboardQuery>,
) -> Result<Json<Vec<MinerOreLeaderboardRow>>, AppError> {
    let limit  = q.limit.unwrap_or(100).clamp(1, 2000);
    let offset = q.offset.unwrap_or(0).max(0);
    let rows =  database::get_ore_leaderboard_all_time_v2(&state.db_pool, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard_ore(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerOreLeaderboardRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rows = database::get_ore_leaderboard_last_n_rounds(&state.db_pool, 60, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_leaderboard_latest_rounds_ore(
    State(state): State<AppState>,
    Query(p): Query<Pagination>,
) -> Result<Json<Vec<MinerOreLeaderboardRow>>, AppError> {
    let limit = p.limit.unwrap_or(100).clamp(1, 2000);
    let offset = p.offset.unwrap_or(0).max(0);
    let rows = database::get_ore_leaderboard_last_n_rounds_v2(&state.db_pool, 60, limit, offset).await?;
    Ok(Json(rows))
}

async fn get_miner_stats(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
    Query(p): Query<RoundsPagination>,
) -> Result<Json<Vec<MinerTotalsRow>>, AppError> {
    let miner_stats = database::get_miner_stats(&state.db_pool, pubkey).await?;
    if let Some(s) = miner_stats {
        return Ok(Json(vec![s]))
    } else {
        return Ok(Json(vec![]))
    }
}

async fn get_miner_latest(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> Result<Json<Option<AppMiner>>, AppError> {
    let pubkey = if let Ok(p) = Pubkey::from_str(&pubkey) {
        p.to_string()
    } else {
        return Ok(Json(None))
    };

    let miners = state.miners.clone();
    let reader = miners.read().await;
    let miners = reader.clone();
    drop(reader);
    if miners.len() > 0 {
        for m in miners {
            if m.authority == pubkey {
                return Ok(Json(Some(m)));
            }
        }
    }
    Ok(Json(None))
}

async fn get_miner_snapshot(
    State(state): State<AppState>,
    Path(pubkey): Path<String>,
) -> Result<Json<Option<DbMinerSnapshot>>, AppError> {
    if let Ok(p) = Pubkey::from_str(&pubkey) {
        let earnings = database::get_snapshot_24h_ago(&state.db_pool, p.to_string()).await?;
        return Ok(Json(earnings))
    } else {
        return Ok(Json(None))
    };
}

async fn get_available_pubkeys(
    State(state): State<AppState>,
    Path(letters): Path<String>,
) -> Result<Json<Vec<String>>, AppError> {
    let pubkeys = database::get_available_pubkeys(&state.db_pool, letters).await?;
    return Ok(Json(pubkeys))
}

async fn get_live_round(
    State(state): State<AppState>,
) -> Result<Json<AppRound>, AppError> {
    let round = state.live_round.clone();
    let reader = round.read().await;
    let round = reader.clone();
    drop(reader);
    Ok(Json(round))
}

async fn get_live_deployments(
    State(state): State<AppState>,
) -> Result<Json<Vec<AppLiveDeployment>>, AppError> {
    let deployments = state.live_deployments.clone();
    let reader = deployments.read().await;
    let deployments = reader.clone();
    drop(reader);
    Ok(Json(deployments))
}

async fn sse_handler(
    State(app_state): State<AppState>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let mut rx_broadcast = app_state.live_data_broadcaster.subscribe();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx_broadcast.recv().await {
            // Create an SSE event with the message data
            if let Ok(msg) = serde_json::to_string(&msg) {
                yield Ok(sse::Event::default().data(msg));
            }
        }
    };

    Sse::new(stream).keep_alive(sse::KeepAlive::default())
}

async fn sse_rounds_handler(
    State(app_state): State<AppState>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let mut rx_broadcast = app_state.live_data_broadcaster.subscribe();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx_broadcast.recv().await {
            // Create an SSE event with the message data
            match msg {
                LiveBroadcastData::Round(_) => {
                    if let Ok(msg) = serde_json::to_string(&msg) {
                        yield Ok(sse::Event::default().data(msg));
                    }
                },
                _ => {}
            }
        }
    };

    Sse::new(stream).keep_alive(sse::KeepAlive::default())
}

async fn sse_deployments_handler(
    State(app_state): State<AppState>,
) -> Sse<impl Stream<Item = Result<sse::Event, Infallible>>> {
    let mut rx_broadcast = app_state.live_data_broadcaster.subscribe();

    let stream = async_stream::stream! {
        while let Ok(msg) = rx_broadcast.recv().await {
            // Create an SSE event with the message data
            match msg {
                LiveBroadcastData::Deployment(_) => {
                    if let Ok(msg) = serde_json::to_string(&msg) {
                        yield Ok(sse::Event::default().data(msg));
                    }
                },
                LiveBroadcastData::WinningSquare(_) => {
                    if let Ok(msg) = serde_json::to_string(&msg) {
                        yield Ok(sse::Event::default().data(msg));
                    }
                },
                _ => {}
            }
        }
    };

    Sse::new(stream).keep_alive(sse::KeepAlive::default())
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        use axum::{http::StatusCode, Json};
        #[derive(Serialize)]
        struct ErrBody { error: String }
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, Json(ErrBody { error: "not found".into() })).into_response(),
            other => {
                tracing::error!("internal error: {other:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrBody { error: "internal server error".into() })).into_response()
            }
        }
    }
}


async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        signal(SignalKind::terminate())
            .expect("install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutting down");
}

