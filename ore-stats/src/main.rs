use std::{env, str::FromStr, sync::Arc, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};

use sqlx::{sqlite::SqliteConnectOptions};
use const_crypto::ed25519;
use ore_api::{consts::{BOARD, ROUND, TREASURY_ADDRESS}};
use steel::{AccountDeserialize, Pubkey};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::{account_tracker::AccountTracker, database::Database, rpc::AppRPC};

pub mod app_state;
pub mod app_error;
pub mod external_api;
pub mod rpc;
pub mod database;
pub mod entropy_api;
pub mod account_tracker;
pub mod helius_api;

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
        .foreign_keys(true);

    let db_writer_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(100))
        .connect_with(db_connect_ops.clone())
        .await?;

    let db_reader_pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(db_connect_ops.read_only(true))
        .await?;

    tracing::info!("Running optimize...");
    sqlx::query("PRAGMA optimize").execute(&db_writer_pool).await?;
    tracing::info!("Optimize complete!");



    tracing::info!("Running migrations...");

    sqlx::migrate!("./migrations").run(&db_writer_pool).await?;

    tracing::info!("Database migrations complete.");
    tracing::info!("Database ready!");

   let db = Database::new(db_writer_pool, db_reader_pool);

    let rpc_url = env::var("RPC_URL").expect("RPC_URL must be set");
    let mut app_rpc = AppRPC::new(rpc_url);

    //let round_id = 60_089;
    let round_id = 15_119;
    if let Ok(rr) = app_rpc.reconstruct_round_by_id(round_id).await {
        db.insert_reconstructed_round(&rr).await?;
    }

    // Start the account tracker
    let _account_tracker = AccountTracker::new(app_rpc, db);

    // Get Treasury from db
    //
    // Get Board
    //
    // Get Round
    //
    // Get Miners

    Ok(())
}
