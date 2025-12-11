//! Database module for tracking autodeploy transactions
//!
//! Uses SQLite via sqlx for persistent transaction tracking

use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;

/// Transaction status enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxStatus {
    /// Transaction has been sent but not yet confirmed
    Pending = 0,
    /// Transaction has been confirmed (processed)
    Confirmed = 1,
    /// Transaction has been finalized
    Finalized = 2,
    /// Transaction failed with an error
    Failed = 3,
    /// Transaction expired (blockhash expired)
    Expired = 4,
}

impl TxStatus {
    pub fn from_i32(value: i32) -> Self {
        match value {
            0 => TxStatus::Pending,
            1 => TxStatus::Confirmed,
            2 => TxStatus::Finalized,
            3 => TxStatus::Failed,
            4 => TxStatus::Expired,
            _ => TxStatus::Pending,
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            TxStatus::Pending => "pending",
            TxStatus::Confirmed => "confirmed",
            TxStatus::Finalized => "finalized",
            TxStatus::Failed => "failed",
            TxStatus::Expired => "expired",
        }
    }
}

/// Record for tracking an autodeploy transaction
#[derive(Debug, Clone)]
pub struct AutodeployTx {
    pub id: i64,
    /// Transaction signature (base58)
    pub signature: String,
    /// Manager account pubkey
    pub manager_key: String,
    /// Deployer account pubkey
    pub deployer_key: String,
    /// Auth ID for the managed miner
    pub auth_id: i64,
    /// ORE round ID
    pub round_id: i64,
    /// Amount deployed per square (lamports)
    pub amount_per_square: i64,
    /// Bitmask of squares deployed to
    pub squares_mask: i64,
    /// Number of squares deployed to
    pub num_squares: i32,
    /// Total amount deployed (lamports)
    pub total_deployed: i64,
    /// Deployer fee paid (lamports)
    pub deployer_fee: i64,
    /// Protocol fee paid (lamports)
    pub protocol_fee: i64,
    /// Priority fee (lamports)
    pub priority_fee: i64,
    /// Jito tip (lamports)
    pub jito_tip: i64,
    /// Last valid blockhash height
    pub last_valid_blockheight: i64,
    /// Unix timestamp when transaction was sent
    pub sent_at: i64,
    /// Unix timestamp when transaction was confirmed (null if not confirmed)
    pub confirmed_at: Option<i64>,
    /// Unix timestamp when transaction was finalized (null if not finalized)
    pub finalized_at: Option<i64>,
    /// Transaction status (0=pending, 1=confirmed, 2=finalized, 3=failed, 4=expired)
    pub status: i32,
    /// Error message if transaction failed
    pub error_message: Option<String>,
    /// Compute units consumed (if confirmed)
    pub compute_units_consumed: Option<i64>,
    /// Slot when confirmed
    pub slot: Option<i64>,
}

/// Initialize the database and create tables
pub async fn init_db(db_path: &Path) -> Result<Pool<Sqlite>, sqlx::Error> {
    // Create database file if it doesn't exist
    if !db_path.exists() {
        std::fs::File::create(db_path)?;
    }
    
    let db_url = format!("sqlite:{}", db_path.display());
    
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await?;
    
    // Create the autodeploy_txs table
    sqlx::query(r#"
        CREATE TABLE IF NOT EXISTS autodeploy_txs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            signature TEXT NOT NULL UNIQUE,
            manager_key TEXT NOT NULL,
            deployer_key TEXT NOT NULL,
            auth_id INTEGER NOT NULL,
            round_id INTEGER NOT NULL,
            amount_per_square INTEGER NOT NULL,
            squares_mask INTEGER NOT NULL,
            num_squares INTEGER NOT NULL,
            total_deployed INTEGER NOT NULL,
            deployer_fee INTEGER NOT NULL,
            protocol_fee INTEGER NOT NULL,
            priority_fee INTEGER NOT NULL,
            jito_tip INTEGER NOT NULL,
            last_valid_blockheight INTEGER NOT NULL,
            sent_at INTEGER NOT NULL,
            confirmed_at INTEGER,
            finalized_at INTEGER,
            status INTEGER NOT NULL DEFAULT 0,
            error_message TEXT,
            compute_units_consumed INTEGER,
            slot INTEGER,
            created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
        )
    "#)
    .execute(&pool)
    .await?;
    
    // Create indexes for common queries
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_autodeploy_txs_status ON autodeploy_txs(status)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_autodeploy_txs_manager ON autodeploy_txs(manager_key)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_autodeploy_txs_round ON autodeploy_txs(round_id)")
        .execute(&pool)
        .await?;
    
    sqlx::query("CREATE INDEX IF NOT EXISTS idx_autodeploy_txs_sent_at ON autodeploy_txs(sent_at)")
        .execute(&pool)
        .await?;
    
    Ok(pool)
}

/// Insert a new autodeploy transaction record
pub async fn insert_tx(
    pool: &Pool<Sqlite>,
    signature: &str,
    manager_key: &str,
    deployer_key: &str,
    auth_id: u64,
    round_id: u64,
    amount_per_square: u64,
    squares_mask: u32,
    num_squares: u32,
    total_deployed: u64,
    deployer_fee: u64,
    protocol_fee: u64,
    priority_fee: u64,
    jito_tip: u64,
    last_valid_blockheight: u64,
    sent_at: i64,
) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(r#"
        INSERT INTO autodeploy_txs (
            signature, manager_key, deployer_key, auth_id, round_id,
            amount_per_square, squares_mask, num_squares, total_deployed,
            deployer_fee, protocol_fee, priority_fee, jito_tip,
            last_valid_blockheight, sent_at, status
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
    "#)
    .bind(signature)
    .bind(manager_key)
    .bind(deployer_key)
    .bind(auth_id as i64)
    .bind(round_id as i64)
    .bind(amount_per_square as i64)
    .bind(squares_mask as i64)
    .bind(num_squares as i32)
    .bind(total_deployed as i64)
    .bind(deployer_fee as i64)
    .bind(protocol_fee as i64)
    .bind(priority_fee as i64)
    .bind(jito_tip as i64)
    .bind(last_valid_blockheight as i64)
    .bind(sent_at)
    .execute(pool)
    .await?;
    
    Ok(result.last_insert_rowid())
}

/// Update transaction status to confirmed
pub async fn update_tx_confirmed(
    pool: &Pool<Sqlite>,
    signature: &str,
    confirmed_at: i64,
    slot: u64,
    compute_units: Option<u64>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        UPDATE autodeploy_txs 
        SET status = 1, confirmed_at = ?, slot = ?, compute_units_consumed = ?
        WHERE signature = ?
    "#)
    .bind(confirmed_at)
    .bind(slot as i64)
    .bind(compute_units.map(|cu| cu as i64))
    .bind(signature)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Update transaction status to finalized
pub async fn update_tx_finalized(
    pool: &Pool<Sqlite>,
    signature: &str,
    finalized_at: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        UPDATE autodeploy_txs 
        SET status = 2, finalized_at = ?
        WHERE signature = ?
    "#)
    .bind(finalized_at)
    .bind(signature)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Update transaction status to failed
pub async fn update_tx_failed(
    pool: &Pool<Sqlite>,
    signature: &str,
    error_message: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        UPDATE autodeploy_txs 
        SET status = 3, error_message = ?
        WHERE signature = ?
    "#)
    .bind(error_message)
    .bind(signature)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Update transaction status to expired
pub async fn update_tx_expired(
    pool: &Pool<Sqlite>,
    signature: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"
        UPDATE autodeploy_txs 
        SET status = 4
        WHERE signature = ?
    "#)
    .bind(signature)
    .execute(pool)
    .await?;
    
    Ok(())
}

/// Get all pending transactions
pub async fn get_pending_txs(pool: &Pool<Sqlite>) -> Result<Vec<AutodeployTx>, sqlx::Error> {
    let rows = sqlx::query(r#"
        SELECT 
            id, signature, manager_key, deployer_key, auth_id, round_id,
            amount_per_square, squares_mask, num_squares, total_deployed,
            deployer_fee, protocol_fee, priority_fee, jito_tip,
            last_valid_blockheight, sent_at, confirmed_at, finalized_at,
            status, error_message, compute_units_consumed, slot
        FROM autodeploy_txs 
        WHERE status = 0
        ORDER BY sent_at ASC
        "#)
    .fetch_all(pool)
    .await?;
    
    let txs = rows.into_iter().map(|row| {
        use sqlx::Row;
        AutodeployTx {
            id: row.get("id"),
            signature: row.get("signature"),
            manager_key: row.get("manager_key"),
            deployer_key: row.get("deployer_key"),
            auth_id: row.get("auth_id"),
            round_id: row.get("round_id"),
            amount_per_square: row.get("amount_per_square"),
            squares_mask: row.get("squares_mask"),
            num_squares: row.get("num_squares"),
            total_deployed: row.get("total_deployed"),
            deployer_fee: row.get("deployer_fee"),
            protocol_fee: row.get("protocol_fee"),
            priority_fee: row.get("priority_fee"),
            jito_tip: row.get("jito_tip"),
            last_valid_blockheight: row.get("last_valid_blockheight"),
            sent_at: row.get("sent_at"),
            confirmed_at: row.get("confirmed_at"),
            finalized_at: row.get("finalized_at"),
            status: row.get("status"),
            error_message: row.get("error_message"),
            compute_units_consumed: row.get("compute_units_consumed"),
            slot: row.get("slot"),
        }
    }).collect();
    
    Ok(txs)
}

/// Get recent transactions (last N)
pub async fn get_recent_txs(pool: &Pool<Sqlite>, limit: i32) -> Result<Vec<AutodeployTx>, sqlx::Error> {
    let rows = sqlx::query(r#"
        SELECT 
            id, signature, manager_key, deployer_key, auth_id, round_id,
            amount_per_square, squares_mask, num_squares, total_deployed,
            deployer_fee, protocol_fee, priority_fee, jito_tip,
            last_valid_blockheight, sent_at, confirmed_at, finalized_at,
            status, error_message, compute_units_consumed, slot
        FROM autodeploy_txs 
        ORDER BY sent_at DESC
        LIMIT ?
        "#)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    
    let txs = rows.into_iter().map(|row| {
        use sqlx::Row;
        AutodeployTx {
            id: row.get("id"),
            signature: row.get("signature"),
            manager_key: row.get("manager_key"),
            deployer_key: row.get("deployer_key"),
            auth_id: row.get("auth_id"),
            round_id: row.get("round_id"),
            amount_per_square: row.get("amount_per_square"),
            squares_mask: row.get("squares_mask"),
            num_squares: row.get("num_squares"),
            total_deployed: row.get("total_deployed"),
            deployer_fee: row.get("deployer_fee"),
            protocol_fee: row.get("protocol_fee"),
            priority_fee: row.get("priority_fee"),
            jito_tip: row.get("jito_tip"),
            last_valid_blockheight: row.get("last_valid_blockheight"),
            sent_at: row.get("sent_at"),
            confirmed_at: row.get("confirmed_at"),
            finalized_at: row.get("finalized_at"),
            status: row.get("status"),
            error_message: row.get("error_message"),
            compute_units_consumed: row.get("compute_units_consumed"),
            slot: row.get("slot"),
        }
    }).collect();
    
    Ok(txs)
}

/// Get transaction stats for a time range
pub async fn get_tx_stats(
    pool: &Pool<Sqlite>,
    since_timestamp: i64,
) -> Result<TxStats, sqlx::Error> {
    let row = sqlx::query(r#"
        SELECT 
            COUNT(*) as total_count,
            SUM(CASE WHEN status = 2 THEN 1 ELSE 0 END) as finalized_count,
            SUM(CASE WHEN status = 3 THEN 1 ELSE 0 END) as failed_count,
            SUM(CASE WHEN status = 4 THEN 1 ELSE 0 END) as expired_count,
            SUM(CASE WHEN status = 2 THEN total_deployed ELSE 0 END) as total_deployed_finalized,
            SUM(CASE WHEN status = 2 THEN deployer_fee ELSE 0 END) as total_deployer_fee,
            SUM(CASE WHEN status = 2 THEN protocol_fee ELSE 0 END) as total_protocol_fee
        FROM autodeploy_txs 
        WHERE sent_at >= ?
        "#)
    .bind(since_timestamp)
    .fetch_one(pool)
    .await?;
    
    use sqlx::Row;
    Ok(TxStats {
        total_count: row.get::<i64, _>("total_count") as u64,
        finalized_count: row.get::<Option<i64>, _>("finalized_count").unwrap_or(0) as u64,
        failed_count: row.get::<Option<i64>, _>("failed_count").unwrap_or(0) as u64,
        expired_count: row.get::<Option<i64>, _>("expired_count").unwrap_or(0) as u64,
        total_deployed_finalized: row.get::<Option<i64>, _>("total_deployed_finalized").unwrap_or(0) as u64,
        total_deployer_fee: row.get::<Option<i64>, _>("total_deployer_fee").unwrap_or(0) as u64,
        total_protocol_fee: row.get::<Option<i64>, _>("total_protocol_fee").unwrap_or(0) as u64,
    })
}

/// Transaction statistics
#[derive(Debug, Clone, Default)]
pub struct TxStats {
    pub total_count: u64,
    pub finalized_count: u64,
    pub failed_count: u64,
    pub expired_count: u64,
    pub total_deployed_finalized: u64,
    pub total_deployer_fee: u64,
    pub total_protocol_fee: u64,
}
