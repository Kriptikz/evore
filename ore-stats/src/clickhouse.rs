//! ClickHouse client for ore-stats metrics and historical data.
//!
//! This module provides batched inserts for:
//! - Request logs and server metrics
//! - Rounds and deployments (append-only, immutable)
//! - Treasury and miner snapshots
//! - RPC usage metrics

use std::time::Duration;

use clickhouse::{Client, Row, inserter::Inserter};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClickHouseError {
    #[error("ClickHouse error: {0}")]
    Client(#[from] clickhouse::error::Error),
    
    #[error("Configuration error: {0}")]
    Config(String),
}

/// ClickHouse client wrapper with connection pooling and batched inserts.
#[derive(Clone)]
pub struct ClickHouseClient {
    client: Client,
}

impl ClickHouseClient {
    /// Create a new ClickHouse client.
    /// 
    /// # Arguments
    /// * `url` - ClickHouse HTTP URL (e.g., "http://localhost:8123")
    /// * `database` - Database name (e.g., "ore_stats")
    /// * `user` - Username
    /// * `password` - Password
    pub fn new(url: &str, database: &str, user: &str, password: &str) -> Self {
        let client = Client::default()
            .with_url(url)
            .with_database(database)
            .with_user(user)
            .with_password(password);
        
        Self { client }
    }
    
    /// Create from environment variables.
    /// Expects: CLICKHOUSE_URL, CLICKHOUSE_DB, CLICKHOUSE_USER, CLICKHOUSE_PASSWORD
    pub fn from_env() -> Result<Self, ClickHouseError> {
        let url = std::env::var("CLICKHOUSE_URL")
            .map_err(|_| ClickHouseError::Config("CLICKHOUSE_URL not set".into()))?;
        let database = std::env::var("CLICKHOUSE_DB")
            .unwrap_or_else(|_| "ore_stats".to_string());
        let user = std::env::var("CLICKHOUSE_USER")
            .unwrap_or_else(|_| "default".to_string());
        let password = std::env::var("CLICKHOUSE_PASSWORD")
            .unwrap_or_default();
        
        Ok(Self::new(&url, &database, &user, &password))
    }
    
    /// Get the underlying client for custom queries.
    pub fn client(&self) -> &Client {
        &self.client
    }
    
    // ========== Request Logs ==========
    
    /// Create an inserter for request logs.
    /// Flushes every 1000 rows or 1 second, whichever comes first.
    pub fn request_logs_inserter(&self) -> Result<Inserter<RequestLog>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RequestLog>("request_logs")?
            .with_max_rows(1000)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert a single request log (for immediate writes, prefer inserter for batching).
    pub async fn insert_request_log(&self, log: RequestLog) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("request_logs")?;
        insert.write(&log).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Server Metrics ==========
    
    /// Insert server metrics snapshot.
    pub async fn insert_server_metrics(&self, metrics: ServerMetrics) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("server_metrics")?;
        insert.write(&metrics).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Rounds ==========
    
    /// Insert a finalized round (from live tracker or backfill).
    pub async fn insert_round(&self, round: RoundInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rounds")?;
        insert.write(&round).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Insert multiple rounds (for batch backfill from external API).
    pub async fn insert_rounds(&self, rounds: Vec<RoundInsert>) -> Result<(), ClickHouseError> {
        if rounds.is_empty() {
            return Ok(());
        }
        let mut insert = self.client.insert("rounds")?;
        for r in rounds {
            insert.write(&r).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    /// Check if a round exists (for skipping duplicates during backfill).
    pub async fn round_exists(&self, round_id: u64) -> Result<bool, ClickHouseError> {
        let count: u64 = self.client
            .query("SELECT count() FROM rounds WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(count > 0)
    }
    
    /// Delete a round by ID (for re-backfill).
    pub async fn delete_round(&self, round_id: u64) -> Result<u64, ClickHouseError> {
        // ClickHouse uses ALTER TABLE ... DELETE for MergeTree tables
        self.client
            .query("ALTER TABLE rounds DELETE WHERE round_id = ?")
            .bind(round_id)
            .execute()
            .await?;
        
        // Return approximate affected rows (ClickHouse DELETE is async)
        Ok(1)
    }
    
    /// Delete all deployments for a round (for re-backfill).
    pub async fn delete_deployments_for_round(&self, round_id: u64) -> Result<u64, ClickHouseError> {
        self.client
            .query("ALTER TABLE deployments DELETE WHERE round_id = ?")
            .bind(round_id)
            .execute()
            .await?;
        
        Ok(1)
    }
    
    /// Delete all raw transactions for a round (for re-fetch).
    pub async fn delete_raw_transactions_for_round(&self, round_id: u64) -> Result<(), ClickHouseError> {
        self.client
            .query("ALTER TABLE raw_transactions DELETE WHERE round_id = ?")
            .bind(round_id)
            .execute()
            .await?;
        
        Ok(())
    }
    
    /// Count deployments for a round (to check if data exists).
    pub async fn count_deployments_for_round(&self, round_id: u64) -> Result<u64, ClickHouseError> {
        let count: u64 = self.client
            .query("SELECT count() FROM deployments WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(count)
    }
    
    /// Sum of all deployment amounts for a round (for validation against round total_deployed).
    pub async fn sum_deployments_for_round(&self, round_id: u64) -> Result<u64, ClickHouseError> {
        let sum: u64 = self.client
            .query("SELECT sum(amount) FROM deployments WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(sum)
    }
    
    /// Get deployment count and sum for a round (combined for efficiency).
    pub async fn get_deployment_stats_for_round(&self, round_id: u64) -> Result<(u64, u64), ClickHouseError> {
        let row: (u64, u64) = self.client
            .query("SELECT count(), sum(amount) FROM deployments WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(row)
    }
    
    /// Get the oldest round ID in the database.
    pub async fn get_oldest_round_id(&self) -> Result<Option<u64>, ClickHouseError> {
        let result: Option<u64> = self.client
            .query("SELECT min(round_id) FROM rounds")
            .fetch_optional()
            .await?;
        Ok(result)
    }
    
    /// Get recent rounds (for listing).
    pub async fn get_recent_rounds(&self, limit: u32) -> Result<Vec<RoundRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    round_id,
                    start_slot,
                    end_slot,
                    winning_square,
                    top_miner,
                    top_miner_reward,
                    total_deployed,
                    total_vaulted,
                    total_winnings,
                    motherlode,
                    motherlode_hit,
                    total_deployments,
                    unique_miners,
                    source,
                    created_at
                FROM rounds
                ORDER BY round_id DESC
                LIMIT ?
            "#)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get rounds with pagination support.
    /// - `before_round_id`: If provided, returns rounds with round_id < this value (cursor-based)
    /// - `offset`: If provided and before_round_id is None, skips this many rounds (offset-based)
    /// - `limit`: Max number of rounds to return
    /// Returns (rounds, has_more)
    pub async fn get_rounds_paginated(
        &self,
        before_round_id: Option<u64>,
        offset: Option<u32>,
        limit: u32,
    ) -> Result<(Vec<RoundRow>, bool), ClickHouseError> {
        // Fetch one extra to determine if there are more
        let fetch_limit = limit + 1;
        
        let results: Vec<RoundRow> = if let Some(before_id) = before_round_id {
            // Cursor-based pagination - rounds before this ID
            self.client
                .query(r#"
                    SELECT 
                        round_id,
                        start_slot,
                        end_slot,
                        winning_square,
                        top_miner,
                        top_miner_reward,
                        total_deployed,
                        total_vaulted,
                        total_winnings,
                        motherlode,
                        motherlode_hit,
                        total_deployments,
                        unique_miners,
                        source,
                        created_at
                    FROM rounds
                    WHERE round_id < ?
                    ORDER BY round_id DESC
                    LIMIT ?
                "#)
                .bind(before_id)
                .bind(fetch_limit)
                .fetch_all()
                .await?
        } else if let Some(skip) = offset {
            // Offset-based pagination
            self.client
                .query(r#"
                    SELECT 
                        round_id,
                        start_slot,
                        end_slot,
                        winning_square,
                        top_miner,
                        top_miner_reward,
                        total_deployed,
                        total_vaulted,
                        total_winnings,
                        motherlode,
                        motherlode_hit,
                        total_deployments,
                        unique_miners,
                        source,
                        created_at
                    FROM rounds
                    ORDER BY round_id DESC
                    LIMIT ? OFFSET ?
                "#)
                .bind(fetch_limit)
                .bind(skip)
                .fetch_all()
                .await?
        } else {
            // No pagination, just get latest
            self.client
                .query(r#"
                    SELECT 
                        round_id,
                        start_slot,
                        end_slot,
                        winning_square,
                        top_miner,
                        top_miner_reward,
                        total_deployed,
                        total_vaulted,
                        total_winnings,
                        motherlode,
                        motherlode_hit,
                        total_deployments,
                        unique_miners,
                        source,
                        created_at
                    FROM rounds
                    ORDER BY round_id DESC
                    LIMIT ?
                "#)
                .bind(fetch_limit)
                .fetch_all()
                .await?
        };
        
        let has_more = results.len() > limit as usize;
        let rounds: Vec<RoundRow> = results.into_iter().take(limit as usize).collect();
        
        Ok((rounds, has_more))
    }
    
    /// Get rounds with flexible filtering for admin backfill
    pub async fn get_rounds_filtered_for_admin(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        before_round_id: Option<u64>,
        offset: Option<u32>,
        limit: u32,
    ) -> Result<(Vec<RoundRow>, bool), ClickHouseError> {
        // Build WHERE conditions
        let mut conditions = Vec::new();
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        if let Some(before) = before_round_id {
            conditions.push(format!("round_id < {}", before));
        }
        
        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };
        
        // Fetch one extra to determine if there are more
        let fetch_limit = limit + 1;
        let skip = offset.unwrap_or(0);
        
        let query = format!(r#"
            SELECT 
                round_id,
                start_slot,
                end_slot,
                winning_square,
                top_miner,
                top_miner_reward,
                total_deployed,
                total_vaulted,
                total_winnings,
                motherlode,
                motherlode_hit,
                total_deployments,
                unique_miners,
                source,
                created_at
            FROM rounds
            WHERE {}
            ORDER BY round_id DESC
            LIMIT {} OFFSET {}
        "#, where_clause, fetch_limit, skip);
        
        let results: Vec<RoundRow> = self.client
            .query(&query)
            .fetch_all()
            .await?;
        
        let has_more = results.len() > limit as usize;
        let rounds: Vec<RoundRow> = results.into_iter().take(limit as usize).collect();
        
        Ok((rounds, has_more))
    }
    
    /// Get total count of rounds matching filter criteria
    pub async fn get_rounds_count_filtered(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
    ) -> Result<u64, ClickHouseError> {
        let mut conditions = Vec::new();
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        
        let where_clause = if conditions.is_empty() {
            "1=1".to_string()
        } else {
            conditions.join(" AND ")
        };
        
        let query = format!("SELECT count() FROM rounds WHERE {}", where_clause);
        let count: u64 = self.client.query(&query).fetch_one().await?;
        Ok(count)
    }
    
    /// Get rounds that have no deployments stored (missing deployment data)
    pub async fn get_rounds_with_missing_deployments(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        before_round_id: Option<u64>,
        offset: Option<u32>,
        limit: u32,
    ) -> Result<(Vec<RoundRow>, bool), ClickHouseError> {
        let mut conditions = vec!["total_deployed > 0".to_string()]; // Only care about rounds that should have deployments
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("r.round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("r.round_id <= {}", lte));
        }
        if let Some(before) = before_round_id {
            conditions.push(format!("r.round_id < {}", before));
        }
        
        let where_clause = conditions.join(" AND ");
        let fetch_limit = limit + 1;
        let skip = offset.unwrap_or(0);
        
        // Use LEFT JOIN to find rounds with no deployments
        let query = format!(r#"
            SELECT 
                r.round_id,
                r.start_slot,
                r.end_slot,
                r.winning_square,
                r.top_miner,
                r.top_miner_reward,
                r.total_deployed,
                r.total_vaulted,
                r.total_winnings,
                r.motherlode,
                r.motherlode_hit,
                r.total_deployments,
                r.unique_miners,
                r.source,
                r.created_at
            FROM rounds r
            LEFT JOIN (
                SELECT round_id, count() as dep_count
                FROM deployments
                GROUP BY round_id
            ) d ON r.round_id = d.round_id
            WHERE {} AND (d.dep_count IS NULL OR d.dep_count = 0)
            ORDER BY r.round_id DESC
            LIMIT {} OFFSET {}
        "#, where_clause, fetch_limit, skip);
        
        let results: Vec<RoundRow> = self.client
            .query(&query)
            .fetch_all()
            .await?;
        
        let has_more = results.len() > limit as usize;
        let rounds: Vec<RoundRow> = results.into_iter().take(limit as usize).collect();
        
        Ok((rounds, has_more))
    }
    
    /// Get rounds where deployment amounts don't match total_deployed (invalid data)
    pub async fn get_rounds_with_invalid_deployments(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        before_round_id: Option<u64>,
        offset: Option<u32>,
        limit: u32,
    ) -> Result<(Vec<RoundRowWithDeploymentStats>, bool), ClickHouseError> {
        let mut conditions = vec!["r.total_deployed > 0".to_string()]; // Only care about rounds that should have deployments
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("r.round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("r.round_id <= {}", lte));
        }
        if let Some(before) = before_round_id {
            conditions.push(format!("r.round_id < {}", before));
        }
        
        let where_clause = conditions.join(" AND ");
        let fetch_limit = limit + 1;
        let skip = offset.unwrap_or(0);
        
        // Join with deployments sum and filter for mismatches
        let query = format!(r#"
            SELECT 
                r.round_id,
                r.start_slot,
                r.end_slot,
                r.winning_square,
                r.top_miner,
                r.top_miner_reward,
                r.total_deployed,
                r.total_vaulted,
                r.total_winnings,
                r.motherlode,
                r.motherlode_hit,
                r.total_deployments,
                r.unique_miners,
                r.source,
                r.created_at,
                COALESCE(d.dep_count, 0) as deployment_count,
                COALESCE(d.dep_sum, 0) as deployments_sum
            FROM rounds r
            LEFT JOIN (
                SELECT round_id, count() as dep_count, sum(amount) as dep_sum
                FROM deployments
                GROUP BY round_id
            ) d ON r.round_id = d.round_id
            WHERE {} AND (
                d.dep_count IS NULL OR d.dep_count = 0 OR d.dep_sum != r.total_deployed
            )
            ORDER BY r.round_id DESC
            LIMIT {} OFFSET {}
        "#, where_clause, fetch_limit, skip);
        
        let results: Vec<RoundRowWithDeploymentStats> = self.client
            .query(&query)
            .fetch_all()
            .await?;
        
        let has_more = results.len() > limit as usize;
        let rounds: Vec<RoundRowWithDeploymentStats> = results.into_iter().take(limit as usize).collect();
        
        Ok((rounds, has_more))
    }
    
    /// Get missing round IDs (gaps between min and max stored round)
    pub async fn get_missing_round_ids(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        offset: Option<u32>,
        limit: u32,
    ) -> Result<(Vec<u64>, bool, u64, u64), ClickHouseError> {
        // First get the range of round IDs
        let range_query = "SELECT min(round_id) as min_id, max(round_id) as max_id FROM rounds";
        let (min_id, max_id): (u64, u64) = self.client.query(range_query).fetch_one().await?;
        
        if min_id == 0 && max_id == 0 {
            return Ok((vec![], false, 0, 0));
        }
        
        // Apply filters if provided
        let actual_min = round_id_gte.map(|g| g.max(min_id)).unwrap_or(min_id);
        let actual_max = round_id_lte.map(|l| l.min(max_id)).unwrap_or(max_id);
        
        if actual_min > actual_max {
            return Ok((vec![], false, min_id, max_id));
        }
        
        let fetch_limit = limit + 1;
        let skip = offset.unwrap_or(0);
        
        // Generate all expected IDs and find ones that don't exist
        // Using numbers() table function to generate the range
        let query = format!(r#"
            SELECT n.number as missing_id
            FROM numbers({}, {}) n
            WHERE n.number NOT IN (
                SELECT round_id FROM rounds WHERE round_id >= {} AND round_id <= {}
            )
            ORDER BY n.number DESC
            LIMIT {} OFFSET {}
        "#, actual_min, actual_max - actual_min + 1, actual_min, actual_max, fetch_limit, skip);
        
        let results: Vec<u64> = self.client
            .query(&query)
            .fetch_all()
            .await?;
        
        let has_more = results.len() > limit as usize;
        let missing_ids: Vec<u64> = results.into_iter().take(limit as usize).collect();
        
        Ok((missing_ids, has_more, min_id, max_id))
    }
    
    /// Get count of rounds with missing deployments
    pub async fn get_rounds_with_missing_deployments_count(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
    ) -> Result<u64, ClickHouseError> {
        let mut conditions = vec!["total_deployed > 0".to_string()];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("r.round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("r.round_id <= {}", lte));
        }
        
        let where_clause = conditions.join(" AND ");
        
        let query = format!(r#"
            SELECT count()
            FROM rounds r
            LEFT JOIN (
                SELECT round_id, count() as dep_count
                FROM deployments
                GROUP BY round_id
            ) d ON r.round_id = d.round_id
            WHERE {} AND (d.dep_count IS NULL OR d.dep_count = 0)
        "#, where_clause);
        
        let count: u64 = self.client.query(&query).fetch_one().await?;
        Ok(count)
    }
    
    /// Get count of rounds with invalid deployments
    pub async fn get_rounds_with_invalid_deployments_count(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
    ) -> Result<u64, ClickHouseError> {
        let mut conditions = vec!["r.total_deployed > 0".to_string()];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("r.round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("r.round_id <= {}", lte));
        }
        
        let where_clause = conditions.join(" AND ");
        
        let query = format!(r#"
            SELECT count()
            FROM rounds r
            LEFT JOIN (
                SELECT round_id, count() as dep_count, sum(amount) as dep_sum
                FROM deployments
                GROUP BY round_id
            ) d ON r.round_id = d.round_id
            WHERE {} AND (
                d.dep_count IS NULL OR d.dep_count = 0 OR d.dep_sum != r.total_deployed
            )
        "#, where_clause);
        
        let count: u64 = self.client.query(&query).fetch_one().await?;
        Ok(count)
    }
    
    /// Get count of missing round IDs
    pub async fn get_missing_round_ids_count(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
    ) -> Result<u64, ClickHouseError> {
        let range_query = "SELECT min(round_id) as min_id, max(round_id) as max_id FROM rounds";
        let (min_id, max_id): (u64, u64) = self.client.query(range_query).fetch_one().await?;
        
        if min_id == 0 && max_id == 0 {
            return Ok(0);
        }
        
        let actual_min = round_id_gte.map(|g| g.max(min_id)).unwrap_or(min_id);
        let actual_max = round_id_lte.map(|l| l.min(max_id)).unwrap_or(max_id);
        
        if actual_min > actual_max {
            return Ok(0);
        }
        
        let query = format!(r#"
            SELECT count()
            FROM numbers({}, {}) n
            WHERE n.number NOT IN (
                SELECT round_id FROM rounds WHERE round_id >= {} AND round_id <= {}
            )
        "#, actual_min, actual_max - actual_min + 1, actual_min, actual_max);
        
        let count: u64 = self.client.query(&query).fetch_one().await?;
        Ok(count)
    }
    
    /// Get total count of rounds in database.
    pub async fn get_rounds_count(&self) -> Result<u64, ClickHouseError> {
        let result: u64 = self.client
            .query("SELECT count() FROM rounds")
            .fetch_one()
            .await?;
        Ok(result)
    }
    
    /// Get a single round by ID.
    pub async fn get_round_by_id(&self, round_id: u64) -> Result<Option<RoundRow>, ClickHouseError> {
        let result = self.client
            .query(r#"
                SELECT 
                    round_id,
                    start_slot,
                    end_slot,
                    winning_square,
                    top_miner,
                    top_miner_reward,
                    total_deployed,
                    total_vaulted,
                    total_winnings,
                    motherlode,
                    motherlode_hit,
                    total_deployments,
                    unique_miners,
                    source,
                    created_at
                FROM rounds
                WHERE round_id = ?
            "#)
            .bind(round_id)
            .fetch_optional()
            .await?;
        Ok(result)
    }
    
    /// Get deployments for a round.
    pub async fn get_deployments_for_round(&self, round_id: u64) -> Result<Vec<DeploymentRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    round_id,
                    miner_pubkey,
                    square_id,
                    amount,
                    deployed_slot,
                    sol_earned,
                    ore_earned,
                    is_winner,
                    is_top_miner
                FROM deployments
                WHERE round_id = ?
                ORDER BY amount DESC
            "#)
            .bind(round_id)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Deployments ==========
    
    /// Create an inserter for deployments.
    /// Flushes every 500 rows or 500ms, whichever comes first.
    pub fn deployments_inserter(&self) -> Result<Inserter<DeploymentInsert>, ClickHouseError> {
        let inserter = self.client
            .inserter::<DeploymentInsert>("deployments")?
            .with_max_rows(500)
            .with_period(Some(Duration::from_millis(500)));
        Ok(inserter)
    }
    
    /// Insert multiple deployments at once.
    pub async fn insert_deployments(&self, deployments: Vec<DeploymentInsert>) -> Result<(), ClickHouseError> {
        if deployments.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("deployments")?;
        for d in deployments {
            insert.write(&d).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    // ========== Treasury Snapshots ==========
    
    /// Insert a treasury snapshot.
    pub async fn insert_treasury_snapshot(&self, snapshot: TreasurySnapshot) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("treasury_snapshots")?;
        insert.write(&snapshot).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== Miner Snapshots ==========
    
    /// Create an inserter for miner snapshots.
    pub fn miner_snapshots_inserter(&self) -> Result<Inserter<MinerSnapshot>, ClickHouseError> {
        let inserter = self.client
            .inserter::<MinerSnapshot>("miner_snapshots")?
            .with_max_rows(1000)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert multiple miner snapshots at once.
    pub async fn insert_miner_snapshots(&self, snapshots: Vec<MinerSnapshot>) -> Result<(), ClickHouseError> {
        if snapshots.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("miner_snapshots")?;
        for s in snapshots {
            insert.write(&s).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    /// Get the latest round_id that has miner snapshots.
    pub async fn get_latest_snapshot_round(&self) -> Result<Option<u64>, ClickHouseError> {
        let query = "SELECT max(round_id) as max_round FROM miner_snapshots";
        let result: Option<u64> = self.client.query(query).fetch_optional().await?;
        Ok(result.filter(|&r| r > 0))
    }
    
    /// Get miners from a specific snapshot round with filtering and pagination.
    /// sort_by: "refined_ore", "unclaimed_ore", "lifetime_sol", "lifetime_ore"
    /// sort_order: "desc" or "asc"
    pub async fn get_miner_snapshots(
        &self,
        round_id: u64,
        sort_by: &str,
        sort_order: &str,
        offset: u32,
        limit: u32,
        search: Option<&str>,
    ) -> Result<(Vec<MinerSnapshotRow>, u64), ClickHouseError> {
        // Validate sort column
        let sort_column = match sort_by {
            "unclaimed_ore" => "unclaimed_ore",
            "lifetime_sol" => "lifetime_sol",
            "lifetime_ore" => "lifetime_ore",
            _ => "refined_ore", // Default
        };
        
        let order = if sort_order == "asc" { "ASC" } else { "DESC" };
        
        // Build search filter
        let search_filter = if let Some(s) = search {
            if !s.is_empty() {
                format!("AND miner_pubkey LIKE '%{}%'", s.replace("'", "''"))
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        
        // Count query
        let count_query = format!(
            "SELECT count(*) FROM miner_snapshots WHERE round_id = {} {}",
            round_id, search_filter
        );
        let total_count: u64 = self.client.query(&count_query).fetch_one().await?;
        
        // Data query
        let query = format!(
            r#"SELECT 
                round_id,
                miner_pubkey,
                unclaimed_ore,
                refined_ore,
                lifetime_sol,
                lifetime_ore
            FROM miner_snapshots
            WHERE round_id = {} {}
            ORDER BY {} {}
            LIMIT {} OFFSET {}"#,
            round_id, search_filter, sort_column, order, limit, offset
        );
        
        let rows: Vec<MinerSnapshotRow> = self.client.query(&query).fetch_all().await?;
        
        Ok((rows, total_count))
    }
    
    // ========== RPC Metrics ==========
    
    /// Create an inserter for RPC request metrics.
    pub fn rpc_metrics_inserter(&self) -> Result<Inserter<RpcRequestInsert>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RpcRequestInsert>("rpc_requests")?
            .with_max_rows(100)
            .with_period(Some(Duration::from_secs(1)));
        Ok(inserter)
    }
    
    /// Insert a single RPC metric (for immediate logging).
    pub async fn insert_rpc_metric(&self, metric: RpcRequestInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rpc_requests")?;
        insert.write(&metric).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== RPC Metrics Queries ==========
    
    /// Get RPC metrics summary for the last N hours, grouped by provider and method.
    pub async fn get_rpc_summary(&self, hours: u32) -> Result<Vec<RpcSummaryRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    program,
                    provider,
                    method,
                    target_type,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    sum(timeout_count) AS timeout_count,
                    sum(rate_limited_count) AS rate_limited_count,
                    sum(not_found_count) AS not_found_count,
                    sum(total_operations) AS total_operations,
                    sum(total_results) AS total_results,
                    sum(total_duration_ms) / sum(duration_count) AS avg_duration_ms,
                    max(max_duration_ms) AS max_duration_ms,
                    min(min_duration_ms) AS min_duration_ms,
                    sum(total_request_bytes) AS total_request_bytes,
                    sum(total_response_bytes) AS total_response_bytes
                FROM rpc_metrics_minute
                WHERE minute > now() - INTERVAL ? HOUR
                GROUP BY program, provider, method, target_type
                ORDER BY total_requests DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC metrics by provider for the last N hours.
    pub async fn get_rpc_by_provider(&self, hours: u32) -> Result<Vec<RpcProviderRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    program,
                    provider,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    sum(timeout_count) AS timeout_count,
                    sum(rate_limited_count) AS rate_limited_count,
                    sum(total_operations) AS total_operations,
                    sum(total_results) AS total_results,
                    sum(total_duration_ms) / sum(duration_count) AS avg_duration_ms,
                    max(max_duration_ms) AS max_duration_ms,
                    sum(total_request_bytes) AS total_request_bytes,
                    sum(total_response_bytes) AS total_response_bytes
                FROM rpc_metrics_minute
                WHERE minute > now() - INTERVAL ? HOUR
                GROUP BY program, provider
                ORDER BY total_requests DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC errors for the last N hours.
    pub async fn get_rpc_errors(&self, hours: u32, limit: u32) -> Result<Vec<RpcErrorRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    program,
                    provider,
                    method,
                    target_type,
                    target_address,
                    status,
                    error_code,
                    error_message,
                    duration_ms
                FROM rpc_requests
                WHERE timestamp > now() - INTERVAL ? HOUR
                  AND status != 'success'
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get RPC metrics time series for the last N hours (minute granularity).
    pub async fn get_rpc_timeseries(&self, hours: u32) -> Result<Vec<RpcTimeseriesRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    minute,
                    sum(total_requests) AS total_requests,
                    sum(success_count) AS success_count,
                    sum(error_count) AS error_count,
                    sum(timeout_count) AS timeout_count,
                    sum(total_operations) AS total_operations,
                    sum(total_results) AS total_results,
                    sum(total_duration_ms) / sum(duration_count) AS avg_duration_ms,
                    max(max_duration_ms) AS max_duration_ms
                FROM rpc_metrics_minute
                WHERE minute > now() - INTERVAL ? HOUR
                GROUP BY minute
                ORDER BY minute ASC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get daily RPC summary for the last N days.
    pub async fn get_rpc_daily(&self, days: u32) -> Result<Vec<RpcDailyRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    day,
                    program,
                    provider,
                    total_requests,
                    success_count,
                    error_count,
                    rate_limited_count,
                    total_operations,
                    total_results,
                    total_duration_ms / duration_count AS avg_duration_ms,
                    total_request_bytes,
                    total_response_bytes,
                    unique_methods
                FROM rpc_metrics_daily
                WHERE day > today() - INTERVAL ? DAY
                ORDER BY day DESC, total_requests DESC
            "#)
            .bind(days)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get recent RPC requests (all, not just errors).
    pub async fn get_rpc_requests(&self, hours: u32, limit: u32) -> Result<Vec<RpcRequestRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    program,
                    provider,
                    method,
                    target_type,
                    target_address,
                    is_batch,
                    batch_size,
                    status,
                    error_code,
                    error_message,
                    result_count,
                    filters_json,
                    duration_ms,
                    request_size,
                    response_size
                FROM rpc_requests
                WHERE timestamp > now() - INTERVAL ? HOUR
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Rate Limit Events ==========
    
    /// Insert a rate limit event.
    pub async fn insert_rate_limit_event(&self, event: RateLimitEvent) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("rate_limit_events")?;
        insert.write(&event).await?;
        insert.end().await?;
        Ok(())
    }
    
    // ========== WebSocket Metrics ==========
    
    /// Insert a WebSocket event (connect/disconnect/error).
    pub async fn insert_ws_event(&self, event: WsEventInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("ws_events")?;
        insert.write(&event).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Insert WebSocket throughput sample.
    pub async fn insert_ws_throughput(&self, sample: WsThroughputInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("ws_throughput")?;
        insert.write(&sample).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Get WebSocket events for the last N hours.
    pub async fn get_ws_events(&self, hours: u32, limit: u32) -> Result<Vec<WsEventRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    program,
                    provider,
                    subscription_type,
                    subscription_key,
                    event,
                    error_message,
                    disconnect_reason,
                    uptime_seconds,
                    messages_received,
                    reconnect_count
                FROM ws_events
                WHERE timestamp > now() - INTERVAL ? HOUR
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get WebSocket throughput summary for the last N hours.
    pub async fn get_ws_throughput_summary(&self, hours: u32) -> Result<Vec<WsThroughputSummary>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    program,
                    provider,
                    subscription_type,
                    sum(messages_received) AS total_messages,
                    sum(bytes_received) AS total_bytes,
                    avg(avg_process_time_us) AS avg_process_time_us
                FROM ws_throughput
                WHERE timestamp > now() - INTERVAL ? HOUR
                GROUP BY program, provider, subscription_type
                ORDER BY total_messages DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Server Metrics Queries ==========
    
    /// Get server metrics for the last N hours.
    pub async fn get_server_metrics(&self, hours: u32, limit: u32) -> Result<Vec<ServerMetricsRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    requests_total,
                    requests_success,
                    requests_error,
                    latency_p50,
                    latency_p95,
                    latency_p99,
                    latency_avg,
                    active_connections,
                    memory_used,
                    cache_hits,
                    cache_misses
                FROM server_metrics
                WHERE timestamp > now() - INTERVAL ? HOUR
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Request Logs Queries ==========
    
    /// Get recent request logs, optionally filtered by IP hash.
    pub async fn get_request_logs(&self, hours: u32, limit: u32, ip_hash: Option<&str>) -> Result<Vec<RequestLogRow>, ClickHouseError> {
        let results = if let Some(ip) = ip_hash {
            self.client
                .query(r#"
                    SELECT 
                        timestamp,
                        endpoint,
                        method,
                        status_code,
                        duration_ms,
                        ip_hash,
                        user_agent
                    FROM request_logs
                    WHERE timestamp > now() - INTERVAL ? HOUR AND ip_hash = ?
                    ORDER BY timestamp DESC
                    LIMIT ?
                "#)
                .bind(hours)
                .bind(ip)
                .bind(limit)
                .fetch_all()
                .await?
        } else {
            self.client
                .query(r#"
                    SELECT 
                        timestamp,
                        endpoint,
                        method,
                        status_code,
                        duration_ms,
                        ip_hash,
                        user_agent
                    FROM request_logs
                    WHERE timestamp > now() - INTERVAL ? HOUR
                    ORDER BY timestamp DESC
                    LIMIT ?
                "#)
                .bind(hours)
                .bind(limit)
                .fetch_all()
                .await?
        };
        Ok(results)
    }
    
    /// Get request logs summary by endpoint for the last N hours.
    pub async fn get_endpoint_summary(&self, hours: u32) -> Result<Vec<EndpointSummaryRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    endpoint,
                    count() AS total_requests,
                    countIf(status_code < 400) AS success_count,
                    countIf(status_code >= 400) AS error_count,
                    avg(duration_ms) AS avg_duration_ms,
                    max(duration_ms) AS max_duration_ms,
                    quantile(0.95)(duration_ms) AS p95_duration_ms
                FROM request_logs
                WHERE timestamp > now() - INTERVAL ? HOUR
                GROUP BY endpoint
                ORDER BY total_requests DESC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get rate limit events for the last N hours.
    pub async fn get_rate_limit_events(&self, hours: u32, limit: u32) -> Result<Vec<RateLimitEventRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    timestamp,
                    ip_hash,
                    endpoint,
                    requests_in_window,
                    window_seconds
                FROM rate_limit_events
                WHERE timestamp > now() - INTERVAL ? HOUR
                ORDER BY timestamp DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get IP activity summary for the last N hours.
    pub async fn get_ip_activity(&self, hours: u32, limit: u32) -> Result<Vec<IpActivityRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT 
                    ip_hash,
                    sum(request_count) AS total_requests,
                    sum(error_count) AS error_count,
                    sum(rate_limit_count) AS rate_limit_count,
                    avg(avg_duration_ms) AS avg_duration_ms
                FROM ip_activity_hourly
                WHERE hour > now() - INTERVAL ? HOUR
                GROUP BY ip_hash
                ORDER BY total_requests DESC
                LIMIT ?
            "#)
            .bind(hours)
            .bind(limit)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    // ========== Database Size Queries ==========
    
    /// Get ClickHouse database sizes for all databases
    pub async fn get_database_sizes(&self) -> Result<Vec<DatabaseSizeRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT
                    database,
                    sum(bytes_on_disk) AS bytes_on_disk,
                    sum(rows) AS total_rows,
                    count() AS table_count
                FROM system.parts
                WHERE active = 1
                GROUP BY database
                ORDER BY bytes_on_disk DESC
            "#)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get ClickHouse table sizes for ore_stats database (legacy - use get_all_table_sizes)
    pub async fn get_table_sizes(&self) -> Result<Vec<TableSizeRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT
                    table,
                    sum(bytes_on_disk) AS bytes_on_disk,
                    sum(rows) AS total_rows,
                    count() AS parts_count
                FROM system.parts
                WHERE active = 1 AND database = 'ore_stats'
                GROUP BY table
                ORDER BY bytes_on_disk DESC
            "#)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get ALL table sizes across all databases (including system)
    pub async fn get_all_table_sizes(&self) -> Result<Vec<DetailedTableSizeRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT
                    database,
                    table,
                    sum(bytes_on_disk) AS bytes_on_disk,
                    sum(data_uncompressed_bytes) AS bytes_uncompressed,
                    sum(rows) AS total_rows,
                    count() AS parts_count,
                    max(modification_time) AS last_modified
                FROM system.parts
                WHERE active = 1
                GROUP BY database, table
                ORDER BY database, bytes_on_disk DESC
            "#)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get ClickHouse storage engine info for tables (all databases)
    pub async fn get_table_engines(&self) -> Result<Vec<TableEngineRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT
                    database,
                    name AS table,
                    engine,
                    partition_key,
                    sorting_key,
                    primary_key
                FROM system.tables
                WHERE engine != ''
                ORDER BY database, name
            "#)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get request stats for the last minute (for metrics snapshot)
    pub async fn get_recent_request_stats(&self) -> Result<RecentRequestStats, ClickHouseError> {
        let result: Option<RecentRequestStats> = self.client
            .query(r#"
                SELECT
                    count() AS total,
                    countIf(status_code >= 200 AND status_code < 400) AS success,
                    countIf(status_code >= 400) AS errors,
                    if(count() > 0, avg(duration_ms), 0) AS avg_duration,
                    if(count() > 0, quantile(0.5)(duration_ms), 0) AS p50,
                    if(count() > 0, quantile(0.95)(duration_ms), 0) AS p95,
                    if(count() > 0, quantile(0.99)(duration_ms), 0) AS p99
                FROM ore_stats.request_logs
                WHERE timestamp > now64(3) - INTERVAL 1 MINUTE
            "#)
            .fetch_one()
            .await
            .ok();
        
        Ok(result.unwrap_or_default())
    }
    
    /// Get requests per minute time series for the last N hours.
    /// Returns data points grouped by minute for graphing.
    pub async fn get_requests_per_minute(&self, hours: u32) -> Result<Vec<RequestsPerMinuteRow>, ClickHouseError> {
        let results = self.client
            .query(r#"
                SELECT
                    toUnixTimestamp(toStartOfMinute(timestamp)) AS minute_ts,
                    count() AS request_count,
                    countIf(status_code >= 200 AND status_code < 400) AS success_count,
                    countIf(status_code >= 400) AS error_count,
                    if(count() > 0, avg(duration_ms), 0) AS avg_latency_ms
                FROM ore_stats.request_logs
                WHERE timestamp > now64(3) - INTERVAL ? HOUR
                GROUP BY minute_ts
                ORDER BY minute_ts ASC
            "#)
            .bind(hours)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get current requests per second (average over last minute).
    pub async fn get_requests_per_second(&self) -> Result<f64, ClickHouseError> {
        let result: Option<RequestCountRow> = self.client
            .query(r#"
                SELECT count() AS cnt
                FROM ore_stats.request_logs
                WHERE timestamp > now64(3) - INTERVAL 1 MINUTE
            "#)
            .fetch_one()
            .await
            .ok();
        
        let count = result.map(|r| r.cnt).unwrap_or(0);
        Ok(count as f64 / 60.0)
    }
    
    // ========== Raw Transactions (Historical Backfill Only) ==========
    
    /// Create an inserter for raw transactions.
    /// Flushes every 100 rows or 500ms for efficient batch inserts.
    pub fn raw_transactions_inserter(&self) -> Result<Inserter<RawTransaction>, ClickHouseError> {
        let inserter = self.client
            .inserter::<RawTransaction>("raw_transactions")?
            .with_max_rows(100)
            .with_period(Some(Duration::from_millis(500)));
        Ok(inserter)
    }
    
    /// Insert multiple raw transactions (for batch processing).
    pub async fn insert_raw_transactions(&self, txs: Vec<RawTransaction>) -> Result<(), ClickHouseError> {
        if txs.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("raw_transactions")?;
        for tx in txs {
            insert.write(&tx).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    /// Get all raw transactions for a round (for reconstruction).
    pub async fn get_raw_transactions_for_round(&self, round_id: u64) -> Result<Vec<RawTransaction>, ClickHouseError> {
        let results = self.client
            .query(
                "SELECT signature, slot, block_time, round_id, tx_type, raw_json, signer, authority 
                 FROM raw_transactions FINAL 
                 WHERE round_id = ? 
                 ORDER BY slot ASC"
            )
            .bind(round_id)
            .fetch_all()
            .await?;
        Ok(results)
    }
    
    /// Get raw transactions count for a round.
    pub async fn get_raw_transaction_count(&self, round_id: u64) -> Result<u32, ClickHouseError> {
        let count: u64 = self.client
            .query("SELECT count() FROM raw_transactions WHERE round_id = ?")
            .bind(round_id)
            .fetch_one()
            .await?;
        Ok(count as u32)
    }
    
    // ========== Automation States ==========
    
    /// Insert an automation state snapshot.
    pub async fn insert_automation_state(&self, state: AutomationStateInsert) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("automation_states")?;
        insert.write(&state).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Insert multiple automation states.
    pub async fn insert_automation_states(&self, states: Vec<AutomationStateInsert>) -> Result<(), ClickHouseError> {
        if states.is_empty() {
            return Ok(());
        }
        
        let mut insert = self.client.insert("automation_states")?;
        for s in states {
            insert.write(&s).await?;
        }
        insert.end().await?;
        Ok(())
    }
    
    // ========== Phase 3: Historical Query Methods ==========
    
    /// Get rounds with filters and cursor pagination.
    pub async fn get_rounds_filtered(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        slot_gte: Option<u64>,
        slot_lte: Option<u64>,
        motherlode_hit: Option<bool>,
        cursor: Option<&str>,
        limit: u32,
        order_desc: bool,
    ) -> Result<Vec<RoundRowWithTimestamp>, ClickHouseError> {
        let mut conditions = vec!["1=1".to_string()];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        if let Some(gte) = slot_gte {
            conditions.push(format!("start_slot >= {}", gte));
        }
        if let Some(lte) = slot_lte {
            conditions.push(format!("end_slot <= {}", lte));
        }
        if let Some(hit) = motherlode_hit {
            conditions.push(format!("motherlode_hit = {}", if hit { 1 } else { 0 }));
        }
        if let Some(c) = cursor {
            if let Ok(rid) = c.parse::<u64>() {
                if order_desc {
                    conditions.push(format!("round_id < {}", rid));
                } else {
                    conditions.push(format!("round_id > {}", rid));
                }
            }
        }
        
        let order = if order_desc { "DESC" } else { "ASC" };
        let query = format!(
            r#"SELECT round_id, start_slot, end_slot, winning_square, top_miner, 
                      total_deployed, total_winnings, unique_miners, motherlode, 
                      motherlode_hit, created_at
               FROM rounds FINAL
               WHERE {} 
               ORDER BY round_id {}
               LIMIT {}"#,
            conditions.join(" AND "), order, limit
        );
        
        let results = self.client.query(&query).fetch_all().await?;
        Ok(results)
    }
    
    /// Get deployments for a round with filters.
    pub async fn get_deployments_for_round_filtered(
        &self,
        round_id: u64,
        miner: Option<&str>,
        winner_only: Option<bool>,
        min_sol_earned: Option<u64>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<DeploymentRow>, ClickHouseError> {
        let mut conditions = vec![format!("round_id = {}", round_id)];
        
        if let Some(m) = miner {
            conditions.push(format!("miner_pubkey = '{}'", m));
        }
        if winner_only == Some(true) {
            conditions.push("is_winner = 1".to_string());
        }
        if let Some(min) = min_sol_earned {
            conditions.push(format!("sol_earned >= {}", min));
        }
        if let Some(c) = cursor {
            // Cursor format: "miner:square"
            let parts: Vec<&str> = c.split(':').collect();
            if parts.len() >= 2 {
                conditions.push(format!("(miner_pubkey, square_id) > ('{}', {})", parts[0], parts[1].parse::<u8>().unwrap_or(0)));
            }
        }
        
        let query = format!(
            r#"SELECT round_id, miner_pubkey, square_id, amount, deployed_slot,
                      sol_earned, ore_earned, is_winner, is_top_miner
               FROM deployments
               WHERE {}
               ORDER BY miner_pubkey, square_id
               LIMIT {}"#,
            conditions.join(" AND "), limit
        );
        
        let results = self.client.query(&query).fetch_all().await?;
        Ok(results)
    }
    
    /// Get deployments across rounds with filters.
    pub async fn get_deployments_filtered(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        miner: Option<&str>,
        winner_only: Option<bool>,
        min_sol_earned: Option<u64>,
        max_sol_earned: Option<u64>,
        min_ore_earned: Option<u64>,
        max_ore_earned: Option<u64>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<DeploymentRow>, ClickHouseError> {
        let mut conditions = vec!["1=1".to_string()];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        if let Some(m) = miner {
            conditions.push(format!("miner_pubkey = '{}'", m));
        }
        if winner_only == Some(true) {
            conditions.push("is_winner = 1".to_string());
        }
        if let Some(min) = min_sol_earned {
            conditions.push(format!("sol_earned >= {}", min));
        }
        if let Some(max) = max_sol_earned {
            conditions.push(format!("sol_earned <= {}", max));
        }
        if let Some(min) = min_ore_earned {
            conditions.push(format!("ore_earned >= {}", min));
        }
        if let Some(max) = max_ore_earned {
            conditions.push(format!("ore_earned <= {}", max));
        }
        if let Some(c) = cursor {
            // Cursor format: "round:miner:square"
            let parts: Vec<&str> = c.split(':').collect();
            if parts.len() >= 3 {
                let rid = parts[0].parse::<u64>().unwrap_or(0);
                let sq = parts[2].parse::<u8>().unwrap_or(0);
                conditions.push(format!("(round_id, miner_pubkey, square_id) > ({}, '{}', {})", rid, parts[1], sq));
            }
        }
        
        let query = format!(
            r#"SELECT round_id, miner_pubkey, square_id, amount, deployed_slot,
                      sol_earned, ore_earned, is_winner, is_top_miner
               FROM deployments
               WHERE {}
               ORDER BY round_id DESC, miner_pubkey, square_id
               LIMIT {}"#,
            conditions.join(" AND "), limit
        );
        
        let results = self.client.query(&query).fetch_all().await?;
        Ok(results)
    }
    
    /// Get miner's deployment history.
    pub async fn get_miner_deployments(
        &self,
        miner: &str,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        winner_only: Option<bool>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<DeploymentRow>, ClickHouseError> {
        let mut conditions = vec![format!("miner_pubkey = '{}'", miner)];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        if winner_only == Some(true) {
            conditions.push("is_winner = 1".to_string());
        }
        if let Some(c) = cursor {
            // Cursor format: "round:square"
            let parts: Vec<&str> = c.split(':').collect();
            if parts.len() >= 2 {
                let rid = parts[0].parse::<u64>().unwrap_or(0);
                let sq = parts[1].parse::<u8>().unwrap_or(0);
                conditions.push(format!("(round_id, square_id) < ({}, {})", rid, sq));
            }
        }
        
        let query = format!(
            r#"SELECT round_id, miner_pubkey, square_id, amount, deployed_slot,
                      sol_earned, ore_earned, is_winner, is_top_miner
               FROM deployments
               WHERE {}
               ORDER BY round_id DESC, square_id DESC
               LIMIT {}"#,
            conditions.join(" AND "), limit
        );
        
        let results = self.client.query(&query).fetch_all().await?;
        Ok(results)
    }
    
    /// Get aggregated miner stats.
    pub async fn get_miner_stats(&self, miner: &str) -> Result<Option<crate::historical_routes::MinerStats>, ClickHouseError> {
        let query = r#"
            SELECT 
                d.miner_pubkey as miner_pubkey,
                sum(d.amount) as total_deployed,
                sum(d.sol_earned) as total_sol_earned,
                sum(d.ore_earned) as total_ore_earned,
                toInt64(sum(d.sol_earned)) - toInt64(sum(d.amount)) as net_sol_change,
                count(DISTINCT d.round_id) as rounds_played,
                countIf(d.is_winner = 1) as rounds_won,
                avg(CASE 
                    WHEN d.deployed_slot > 0 AND r.end_slot > d.deployed_slot 
                    THEN r.end_slot - d.deployed_slot 
                    ELSE 0 
                END) as avg_slots_left
            FROM deployments d
            LEFT JOIN rounds r ON d.round_id = r.round_id
            WHERE d.miner_pubkey = ?
            GROUP BY d.miner_pubkey
        "#;
        
        let row: Option<MinerStatsRow> = self.client.query(query)
            .bind(miner)
            .fetch_optional()
            .await?;
        
        Ok(row.map(|r| {
            let win_rate = if r.rounds_played > 0 {
                (r.rounds_won as f64 / r.rounds_played as f64) * 100.0
            } else {
                0.0
            };
            let avg_deployment = if r.rounds_played > 0 {
                r.total_deployed / r.rounds_played
            } else {
                0
            };
            
            crate::historical_routes::MinerStats {
                miner_pubkey: r.miner_pubkey,
                total_deployed: r.total_deployed,
                total_sol_earned: r.total_sol_earned,
                total_ore_earned: r.total_ore_earned,
                net_sol_change: r.net_sol_change,
                rounds_played: r.rounds_played,
                rounds_won: r.rounds_won,
                win_rate,
                avg_deployment,
                avg_slots_left: r.avg_slots_left,
            }
        }))
    }
    
    /// Get leaderboard with pagination.
    pub async fn get_leaderboard(
        &self,
        metric: &str,
        round_range: &str,
        offset: u32,
        limit: u32,
        min_rounds: Option<u32>,
    ) -> Result<(Vec<crate::historical_routes::LeaderboardEntry>, u64), ClickHouseError> {
        // Build round filter
        let round_filter = match round_range {
            "last_60" => "round_id >= (SELECT max(round_id) - 60 FROM rounds)".to_string(),
            "last_100" => "round_id >= (SELECT max(round_id) - 100 FROM rounds)".to_string(),
            "today" => "deployed_at >= today()".to_string(),
            _ => "1=1".to_string(), // "all"
        };
        
        // Build min_rounds HAVING filter
        let mut having_conditions = Vec::new();
        if let Some(min) = min_rounds {
            having_conditions.push(format!("count(DISTINCT round_id) >= {}", min));
        }
        
        // sol_cost metric: only miners with negative net_sol AND ore_earned > 0
        let is_sol_cost = metric == "sol_cost";
        if is_sol_cost {
            having_conditions.push("(sum(sol_earned) - sum(amount)) < 0".to_string());
            having_conditions.push("sum(ore_earned) > 0".to_string());
        }
        
        let having_clause = if having_conditions.is_empty() {
            String::new()
        } else {
            format!("HAVING {}", having_conditions.join(" AND "))
        };
        
        // Build ordering based on metric
        // sol_cost = abs(net_sol) / ore_earned - lower is better (ASC)
        // Note: for sol_cost we compute raw ratio, scale to ORE in Rust
        let (value_expr, order) = match metric {
            "sol_earned" => ("sum(sol_earned)", "DESC"),
            "ore_earned" => ("sum(ore_earned)", "DESC"),
            "sol_deployed" => ("sum(amount)", "DESC"),
            "sol_cost" => {
                // cost per atomic ORE = -net_sol / ore_earned (lamports per atomic ORE)
                // We scale to SOL per whole ORE in Rust to avoid overflow
                // Add 1 to denominator to avoid division by zero
                ("-(sum(sol_earned) - sum(amount)) / (sum(ore_earned) + 1)", "ASC")
            },
            _ => ("sum(sol_earned) - sum(amount)", "DESC"), // net_sol (can be negative)
        };
        
        // Get total count (with filters)
        let count_query = if !having_conditions.is_empty() {
            format!(
                r#"SELECT count(*) FROM (
                    SELECT miner_pubkey 
                    FROM deployments 
                    WHERE {} 
                    GROUP BY miner_pubkey 
                    {}
                )"#,
                round_filter, having_clause
            )
        } else {
            format!(
                "SELECT count(DISTINCT miner_pubkey) FROM deployments WHERE {}",
                round_filter
            )
        };
        let total_count: u64 = self.client.query(&count_query).fetch_one().await?;
        
        // Get leaderboard page with all metrics
        // Use prefixed aliases to avoid conflict with column names in value_expr
        let query = format!(
            r#"SELECT 
                   miner_pubkey,
                   {} as value,
                   count(DISTINCT round_id) as rounds_played,
                   sum(amount) as total_deployed,
                   sum(sol_earned) as total_earned,
                   sum(ore_earned) as total_ore,
                   sum(sol_earned) - sum(amount) as net_sol
               FROM deployments
               WHERE {}
               GROUP BY miner_pubkey
               {}
               ORDER BY value {}
               LIMIT {} OFFSET {}"#,
            value_expr, round_filter, having_clause, order, limit, offset
        );
        
        let rows: Vec<LeaderboardRow> = self.client.query(&query).fetch_all().await?;
        
        let entries: Vec<crate::historical_routes::LeaderboardEntry> = rows
            .into_iter()
            .enumerate()
            .map(|(i, r)| {
                // Calculate sol_cost_per_ore: cost in lamports per ORE (with 11 decimals)
                // Only if net_sol < 0 and ore_earned > 0
                let sol_cost_per_ore = if r.net_sol < 0 && r.total_ore > 0 {
                    // cost = -net_sol (the loss) / ore_earned
                    // This gives lamports per atomic ORE unit
                    // To get lamports per 1 whole ORE, multiply by 10^11
                    Some((-r.net_sol as i128 * 100_000_000_000i128 / r.total_ore as i128) as i64)
                } else {
                    None
                };
                
                crate::historical_routes::LeaderboardEntry {
                    rank: (offset + i as u32 + 1),
                    miner_pubkey: r.miner_pubkey,
                    value: r.value,
                    rounds_played: r.rounds_played,
                    sol_deployed: r.total_deployed,
                    sol_earned: r.total_earned,
                    ore_earned: r.total_ore,
                    net_sol: r.net_sol,
                    sol_cost_per_ore,
                }
            })
            .collect();
        
        Ok((entries, total_count))
    }
    
    /// Get leaderboard with search filter (keeps ranking intact).
    pub async fn get_leaderboard_filtered(
        &self,
        metric: &str,
        round_range: &str,
        search: &str,
        limit: u32,
        min_rounds: Option<u32>,
    ) -> Result<(Vec<crate::historical_routes::LeaderboardEntry>, u64), ClickHouseError> {
        // Build round filter
        let round_filter = match round_range {
            "last_60" => "round_id >= (SELECT max(round_id) - 60 FROM rounds)".to_string(),
            "last_100" => "round_id >= (SELECT max(round_id) - 100 FROM rounds)".to_string(),
            "today" => "deployed_at >= today()".to_string(),
            _ => "1=1".to_string(), // "all"
        };
        
        // Build min_rounds HAVING filter
        let mut having_conditions = Vec::new();
        if let Some(min) = min_rounds {
            having_conditions.push(format!("count(DISTINCT round_id) >= {}", min));
        }
        
        // sol_cost metric: only miners with negative net_sol AND ore_earned > 0
        let is_sol_cost = metric == "sol_cost";
        if is_sol_cost {
            having_conditions.push("(sum(sol_earned) - sum(amount)) < 0".to_string());
            having_conditions.push("sum(ore_earned) > 0".to_string());
        }
        
        let having_clause = if having_conditions.is_empty() {
            String::new()
        } else {
            format!("HAVING {}", having_conditions.join(" AND "))
        };
        
        // Build ordering based on metric
        let (value_expr, order) = match metric {
            "sol_earned" => ("sum(sol_earned)", "DESC"),
            "ore_earned" => ("sum(ore_earned)", "DESC"),
            "sol_deployed" => ("sum(amount)", "DESC"),
            "sol_cost" => {
                // cost per atomic ORE, scale in Rust
                ("-(sum(sol_earned) - sum(amount)) / (sum(ore_earned) + 1)", "ASC")
            },
            _ => ("sum(sol_earned) - sum(amount)", "DESC"), // net_sol
        };
        
        // Get total count with search filter and filters
        let count_query = if !having_conditions.is_empty() {
            format!(
                r#"SELECT count(*) FROM (
                    SELECT miner_pubkey 
                    FROM deployments 
                    WHERE {} AND miner_pubkey LIKE '%{}%'
                    GROUP BY miner_pubkey 
                    {}
                )"#,
                round_filter,
                search.replace("'", "''"),
                having_clause
            )
        } else {
            format!(
                "SELECT count(DISTINCT miner_pubkey) FROM deployments WHERE {} AND miner_pubkey LIKE '%{}%'",
                round_filter,
                search.replace("'", "''")
            )
        };
        let total_count: u64 = self.client.query(&count_query).fetch_one().await?;
        
        // Use a subquery with row_number to get rankings, then filter
        let query = format!(
            r#"SELECT 
                   miner_pubkey,
                   value,
                   rounds_played,
                   total_deployed,
                   total_earned,
                   total_ore,
                   net_sol,
                   rank
               FROM (
                   SELECT 
                       miner_pubkey,
                       {} as value,
                       count(DISTINCT round_id) as rounds_played,
                       sum(amount) as total_deployed,
                       sum(sol_earned) as total_earned,
                       sum(ore_earned) as total_ore,
                       sum(sol_earned) - sum(amount) as net_sol,
                       row_number() OVER (ORDER BY {} {}) as rank
                   FROM deployments
                   WHERE {}
                   GROUP BY miner_pubkey
                   {}
               )
               WHERE miner_pubkey LIKE '%{}%'
               ORDER BY rank
               LIMIT {}"#,
            value_expr, value_expr, order, round_filter, having_clause,
            search.replace("'", "''"),
            limit
        );
        
        #[derive(Debug, Clone, clickhouse::Row, serde::Deserialize)]
        struct FilteredLeaderboardRow {
            miner_pubkey: String,
            value: i64,
            rounds_played: u64,
            total_deployed: u64,
            total_earned: u64,
            total_ore: u64,
            net_sol: i64,
            rank: u64,
        }
        
        let rows: Vec<FilteredLeaderboardRow> = self.client.query(&query).fetch_all().await?;
        
        let entries: Vec<crate::historical_routes::LeaderboardEntry> = rows
            .into_iter()
            .map(|r| {
                let sol_cost_per_ore = if r.net_sol < 0 && r.total_ore > 0 {
                    Some((-r.net_sol as i128 * 100_000_000_000i128 / r.total_ore as i128) as i64)
                } else {
                    None
                };
                
                crate::historical_routes::LeaderboardEntry {
                    rank: r.rank as u32,
                    miner_pubkey: r.miner_pubkey,
                    value: r.value,
                    rounds_played: r.rounds_played,
                    sol_deployed: r.total_deployed,
                    sol_earned: r.total_earned,
                    ore_earned: r.total_ore,
                    net_sol: r.net_sol,
                    sol_cost_per_ore,
                }
            })
            .collect();
        
        Ok((entries, total_count))
    }
    
    /// Get treasury history snapshots.
    pub async fn get_treasury_history(
        &self,
        round_id_gte: Option<u64>,
        round_id_lte: Option<u64>,
        cursor: Option<&str>,
        limit: u32,
    ) -> Result<Vec<crate::historical_routes::TreasurySnapshot>, ClickHouseError> {
        let mut conditions = vec!["1=1".to_string()];
        
        if let Some(gte) = round_id_gte {
            conditions.push(format!("round_id >= {}", gte));
        }
        if let Some(lte) = round_id_lte {
            conditions.push(format!("round_id <= {}", lte));
        }
        if let Some(c) = cursor {
            if let Ok(rid) = c.parse::<u64>() {
                conditions.push(format!("round_id < {}", rid));
            }
        }
        
        let query = format!(
            r#"SELECT round_id, balance, motherlode, total_staked, 
                      total_unclaimed, total_refined, 
                      created_at
               FROM treasury_snapshots
               WHERE {}
               ORDER BY round_id DESC
               LIMIT {}"#,
            conditions.join(" AND "), limit
        );
        
        let rows: Vec<TreasurySnapshotRow> = self.client.query(&query).fetch_all().await?;
        
        Ok(rows.into_iter().map(|r| crate::historical_routes::TreasurySnapshot {
            round_id: r.round_id,
            balance: r.balance,
            motherlode: r.motherlode,
            total_staked: r.total_staked,
            total_unclaimed: r.total_unclaimed,
            total_refined: r.total_refined,
            created_at: chrono::DateTime::from_timestamp(r.created_at, 0)
                .map(|dt| dt.to_rfc3339())
                .unwrap_or_else(|| r.created_at.to_string()),
        }).collect())
    }
}

// ========== Row Types ==========

/// Request log entry for HTTP requests.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RequestLog {
    pub endpoint: String,
    pub method: String,
    pub status_code: u16,
    pub duration_ms: u32,
    pub ip_hash: String,
    #[serde(default)]
    pub user_agent: String,
}

/// Server metrics snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ServerMetrics {
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_error: u64,
    pub latency_p50: f32,
    pub latency_p95: f32,
    pub latency_p99: f32,
    pub latency_avg: f32,
    pub active_connections: u32,
    pub memory_used: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Round insert data (finalized round).
/// Used for both live tracking and external API backfill.
/// 
/// For backfill from external API, some fields are defaulted:
/// - slot_hash: all zeros
/// - expires_at: ts + ~24 hours in slots  
/// - top_miner_reward: 100000000000 (1 ORE)
/// - rent_payer: empty string
/// - motherlode_hit: 1 if motherlode > 0
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RoundInsert {
    pub round_id: u64,
    
    // Timing
    pub expires_at: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    
    // Hash data
    pub slot_hash: [u8; 32],
    pub winning_square: u8,
    
    // Participants  
    pub rent_payer: String,
    pub top_miner: String,
    pub top_miner_reward: u64,
    
    // Totals
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    
    // Motherlode
    pub motherlode: u64,
    pub motherlode_hit: u8,
    
    // Stats
    pub total_deployments: u32,
    pub unique_miners: u32,
    
    // Source tracking
    #[serde(default = "default_source")]
    pub source: String,
}

fn default_source() -> String {
    "live".to_string()
}

impl RoundInsert {
    /// Create a RoundInsert from external API data with sensible defaults.
    pub fn from_backfill(
        round_id: u64,
        start_slot: u64,
        end_slot: u64,
        winning_square: u8,
        top_miner: String,
        total_deployed: u64,
        total_vaulted: u64,
        total_winnings: u64,
        motherlode: u64,
        unique_miners: u32,
        _timestamp_secs: u64,
    ) -> Self {
        // Approximate expires_at: ts + 24 hours worth of slots (~216000 at 400ms/slot)
        let expires_at = end_slot.saturating_add(216000);
        
        Self {
            round_id,
            expires_at,
            start_slot,
            end_slot,
            slot_hash: [0u8; 32],  // Unknown for backfill
            winning_square,
            rent_payer: String::new(),  // Unknown for backfill
            top_miner,
            top_miner_reward: 100_000_000_000,  // 1 ORE in atomic units
            total_deployed,
            total_vaulted,
            total_winnings,
            motherlode,
            motherlode_hit: if motherlode > 0 { 1 } else { 0 },
            total_deployments: 0,  // Will be updated after reconstruction
            unique_miners,
            source: "backfill".to_string(),
        }
    }
}

/// Deployment insert data.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct DeploymentInsert {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub square_id: u8,
    pub amount: u64,
    pub deployed_slot: u64,  // 0 if unknown from websocket mismatch
    pub ore_earned: u64,
    pub sol_earned: u64,
    pub is_winner: u8,
    pub is_top_miner: u8,
}

/// Round row for queries.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RoundRow {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub top_miner_reward: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub motherlode: u64,
    pub motherlode_hit: u8,
    pub total_deployments: u32,
    pub unique_miners: u32,
    pub source: String,
    #[serde(default)]
    pub created_at: i64,  // DateTime64(3) as unix timestamp
}

/// Round row with deployment statistics for invalid data queries.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RoundRowWithDeploymentStats {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub top_miner_reward: u64,
    pub total_deployed: u64,
    pub total_vaulted: u64,
    pub total_winnings: u64,
    pub motherlode: u64,
    pub motherlode_hit: u8,
    pub total_deployments: u32,
    pub unique_miners: u32,
    pub source: String,
    #[serde(default)]
    pub created_at: i64,
    pub deployment_count: u64,
    pub deployments_sum: u64,
}

/// Deployment row for queries.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct DeploymentRow {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub square_id: u8,
    pub amount: u64,
    pub deployed_slot: u64,
    pub sol_earned: u64,
    pub ore_earned: u64,
    pub is_winner: u8,
    pub is_top_miner: u8,
}

/// Treasury snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct TreasurySnapshot {
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
    #[serde(default)]
    pub round_id: u64,
}

/// Miner snapshot at round end.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct MinerSnapshot {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub unclaimed_ore: u64,
    pub refined_ore: u64,
    pub lifetime_sol: u64,
    pub lifetime_ore: u64,
}

/// Miner snapshot row for queries (same structure, used for deserialization).
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct MinerSnapshotRow {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub unclaimed_ore: u64,
    pub refined_ore: u64,
    pub lifetime_sol: u64,
    pub lifetime_ore: u64,
}

/// RPC request metrics with detailed tracking.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcRequestInsert {
    // Source identification
    pub program: String,
    pub provider: String,
    pub api_key_id: String,
    
    // Request details
    pub method: String,
    pub target_type: String,         // 'board', 'round', 'treasury', 'miner', 'token', 'slot', 'balance', 'program'
    #[serde(default)]
    pub target_address: String,       // Pubkey being queried (if applicable)
    
    // Batch info
    pub is_batch: u8,
    pub batch_size: u16,              // UInt16 in ClickHouse
    
    // Pagination info
    #[serde(default)]
    pub is_paginated: u8,
    #[serde(default)]
    pub page_number: u16,
    #[serde(default)]
    pub cursor: String,
    
    // Response details
    pub status: String,
    #[serde(default)]
    pub error_code: String,
    #[serde(default)]
    pub error_message: String,
    #[serde(default)]
    pub result_count: u32,            // Number of items returned
    
    // Filter configuration (JSON for complex filters)
    #[serde(default)]
    pub filters_json: String,
    
    // Timing
    pub duration_ms: u32,
    
    // Data sizes
    pub request_size: u32,            // UInt32 in ClickHouse
    pub response_size: u32,           // UInt32 in ClickHouse
    
    // Rate limit info
    pub rate_limit_remaining: i32,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_reset: i32,
}

fn default_rate_limit() -> i32 {
    -1
}

impl RpcRequestInsert {
    /// Create a new RPC request insert with common fields.
    pub fn new(
        program: impl Into<String>,
        provider: impl Into<String>,
        api_key_id: impl Into<String>,
        method: impl Into<String>,
        target_type: impl Into<String>,
    ) -> Self {
        Self {
            program: program.into(),
            provider: provider.into(),
            api_key_id: api_key_id.into(),
            method: method.into(),
            target_type: target_type.into(),
            target_address: String::new(),
            is_batch: 0,
            batch_size: 1,
            is_paginated: 0,
            page_number: 0,
            cursor: String::new(),
            status: "pending".into(),
            error_code: String::new(),
            error_message: String::new(),
            result_count: 0,
            filters_json: String::new(),
            duration_ms: 0,
            request_size: 0,
            response_size: 0,
            rate_limit_remaining: -1,
            rate_limit_reset: -1,
        }
    }
    
    /// Set the target address being queried.
    pub fn with_target(mut self, address: impl Into<String>) -> Self {
        self.target_address = address.into();
        self
    }
    
    /// Mark as a batch request.
    pub fn with_batch(mut self, size: u16) -> Self {
        self.is_batch = if size > 1 { 1 } else { 0 };
        self.batch_size = size;
        self
    }
    
    /// Mark as paginated with cursor.
    pub fn with_pagination(mut self, page: u16, cursor: impl Into<String>) -> Self {
        self.is_paginated = 1;
        self.page_number = page;
        self.cursor = cursor.into();
        self
    }
    
    /// Set filter configuration as JSON string.
    pub fn with_filters(mut self, filters_json: impl Into<String>) -> Self {
        self.filters_json = filters_json.into();
        self
    }
    
    /// Set success result.
    pub fn success(mut self, duration_ms: u32, result_count: u32, response_size: u32) -> Self {
        self.status = "success".into();
        self.duration_ms = duration_ms;
        self.result_count = result_count;
        self.response_size = response_size;
        self
    }
    
    /// Set error result.
    pub fn error(mut self, duration_ms: u32, code: impl Into<String>, message: impl Into<String>) -> Self {
        self.status = "error".into();
        self.duration_ms = duration_ms;
        self.error_code = code.into();
        self.error_message = message.into();
        self
    }
    
    /// Set timeout result.
    pub fn timeout(mut self, duration_ms: u32) -> Self {
        self.status = "timeout".into();
        self.duration_ms = duration_ms;
        self
    }
    
    /// Set rate limited result.
    pub fn rate_limited(mut self, duration_ms: u32) -> Self {
        self.status = "rate_limited".into();
        self.duration_ms = duration_ms;
        self
    }
    
    /// Set not found result.
    pub fn not_found(mut self, duration_ms: u32) -> Self {
        self.status = "not_found".into();
        self.duration_ms = duration_ms;
        self
    }
}

/// Rate limit event for admin monitoring.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RateLimitEvent {
    pub ip_hash: String,
    pub endpoint: String,
    pub requests_in_window: u32,
    pub window_seconds: u16,
}

/// Raw transaction for historical rebuild.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RawTransaction {
    pub signature: String,
    pub slot: u64,
    pub block_time: i64,
    #[serde(default)]
    pub round_id: u64,
    pub tx_type: String,
    pub raw_json: String,
    pub signer: String,
    #[serde(default)]
    pub authority: String,
}

/// Automation state snapshot.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct AutomationStateInsert {
    pub authority: String,
    pub round_id: u64,
    pub active: u8,
    pub executor: String,
    pub amount: u64,
    pub fee: u64,
    pub strategy: u8,
    pub mask: u64,
    pub last_updated_slot: u64,
}

// ============================================================================
// RPC Metrics Query Results
// ============================================================================

/// Summary row for RPC metrics (grouped by program, provider, method, target_type).
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcSummaryRow {
    pub program: String,
    pub provider: String,
    pub method: String,
    pub target_type: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub rate_limited_count: u64,
    pub not_found_count: u64,
    pub total_operations: u64,
    pub total_results: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u32,
    pub min_duration_ms: u32,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
}

/// Provider-level summary row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcProviderRow {
    pub program: String,
    pub provider: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub rate_limited_count: u64,
    pub total_operations: u64,
    pub total_results: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u32,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
}

/// Individual RPC error row.
/// Note: timestamp is DateTime64(3) - i64 milliseconds since epoch
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcErrorRow {
    pub timestamp: i64,  // DateTime64(3) → milliseconds since epoch
    pub program: String,
    pub provider: String,
    pub method: String,
    pub target_type: String,
    pub target_address: String,
    pub status: String,
    pub error_code: String,
    pub error_message: String,
    pub duration_ms: u32,
}

/// Individual RPC request row (all requests, not just errors).
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcRequestRow {
    pub timestamp: i64,  // DateTime64(3) → milliseconds since epoch
    pub program: String,
    pub provider: String,
    pub method: String,
    pub target_type: String,
    pub target_address: String,
    pub is_batch: u8,
    pub batch_size: u16,
    pub status: String,
    pub error_code: String,
    pub error_message: String,
    pub result_count: u32,
    pub filters_json: String,
    pub duration_ms: u32,
    pub request_size: u32,
    pub response_size: u32,
}

/// Time series row for RPC metrics (minute granularity).
/// Note: minute is DateTime - u32 seconds since epoch
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcTimeseriesRow {
    pub minute: u32,  // DateTime → seconds since epoch
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub timeout_count: u64,
    pub total_operations: u64,
    pub total_results: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u32,
}

/// Daily summary row.
/// Note: day is Date - u16 days since epoch (1970-01-01)
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RpcDailyRow {
    pub day: u16,  // Date → days since 1970-01-01
    pub program: String,
    pub provider: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub rate_limited_count: u64,
    pub total_operations: u64,
    pub total_results: u64,
    pub avg_duration_ms: f64,
    pub total_request_bytes: u64,
    pub total_response_bytes: u64,
    pub unique_methods: u64,
}

// ============================================================================
// WebSocket Metrics Types
// ============================================================================

/// WebSocket connection event (connect, disconnect, error, reconnecting)
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct WsEventInsert {
    pub program: String,
    pub provider: String,
    pub subscription_type: String,  // 'account', 'slot', 'program'
    #[serde(default)]
    pub subscription_key: String,   // Pubkey or identifier being watched
    pub event: String,              // 'connected', 'disconnected', 'error', 'reconnecting'
    #[serde(default)]
    pub error_message: String,
    #[serde(default)]
    pub disconnect_reason: String,  // 'timeout', 'server_closed', 'error', 'manual'
    #[serde(default)]
    pub uptime_seconds: u32,        // How long was this connection up
    #[serde(default)]
    pub messages_received: u64,     // Total messages on this connection
    #[serde(default)]
    pub reconnect_count: u16,       // How many times has this reconnected
}

/// WebSocket message throughput sample
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct WsThroughputInsert {
    pub program: String,
    pub provider: String,
    pub subscription_type: String,
    pub messages_received: u32,
    pub bytes_received: u64,
    #[serde(default)]
    pub avg_process_time_us: u32,   // Microseconds to process message
}

/// Query result for WebSocket events
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct WsEventRow {
    pub timestamp: i64,  // DateTime64(3) → milliseconds since epoch
    pub program: String,
    pub provider: String,
    pub subscription_type: String,
    pub subscription_key: String,
    pub event: String,
    pub error_message: String,
    pub disconnect_reason: String,
    pub uptime_seconds: u32,
    pub messages_received: u64,
    pub reconnect_count: u16,
}

/// Query result for WebSocket throughput summary
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct WsThroughputSummary {
    pub program: String,
    pub provider: String,
    pub subscription_type: String,
    pub total_messages: u64,
    pub total_bytes: u64,
    pub avg_process_time_us: f64,
}

// ============================================================================
// Server Metrics Query Results
// ============================================================================

/// Query result for server metrics
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct ServerMetricsRow {
    pub timestamp: u32,  // DateTime → seconds since epoch
    pub requests_total: u64,
    pub requests_success: u64,
    pub requests_error: u64,
    pub latency_p50: f32,
    pub latency_p95: f32,
    pub latency_p99: f32,
    pub latency_avg: f32,
    pub active_connections: u32,
    pub memory_used: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Query result for request logs
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RequestLogRow {
    pub timestamp: i64,  // DateTime64(3) → milliseconds since epoch
    pub endpoint: String,
    pub method: String,
    pub status_code: u16,
    pub duration_ms: u32,
    pub ip_hash: String,
    pub user_agent: String,
}

/// Query result for request log summary by endpoint
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct EndpointSummaryRow {
    pub endpoint: String,
    pub total_requests: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub avg_duration_ms: f64,
    pub max_duration_ms: u32,
    pub p95_duration_ms: f64,
}

/// Query result for rate limit events
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RateLimitEventRow {
    pub timestamp: i64,  // DateTime64(3) → milliseconds since epoch
    pub ip_hash: String,
    pub endpoint: String,
    pub requests_in_window: u32,
    pub window_seconds: u16,
}

/// Query result for IP activity summary
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct IpActivityRow {
    pub ip_hash: String,
    pub total_requests: u64,
    pub error_count: u64,
    pub rate_limit_count: u64,
    pub avg_duration_ms: f64,
}

// ============================================================================
// Database Size Query Results
// ============================================================================

/// ClickHouse database size info
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct DatabaseSizeRow {
    pub database: String,
    pub bytes_on_disk: u64,
    pub total_rows: u64,
    pub table_count: u64,
}

/// ClickHouse table size info (legacy)
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct TableSizeRow {
    pub table: String,
    pub bytes_on_disk: u64,
    pub total_rows: u64,
    pub parts_count: u64,
}

/// Detailed ClickHouse table size with compression info
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct DetailedTableSizeRow {
    pub database: String,
    pub table: String,
    pub bytes_on_disk: u64,
    pub bytes_uncompressed: u64,
    pub total_rows: u64,
    pub parts_count: u64,
    pub last_modified: u32,  // DateTime -> seconds since epoch
}

/// ClickHouse table engine info
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct TableEngineRow {
    pub database: String,
    pub table: String,
    pub engine: String,
    pub partition_key: String,
    pub sorting_key: String,
    pub primary_key: String,
}

/// Recent request statistics (for metrics snapshot)
#[derive(Debug, Clone, Default, Row, Serialize, Deserialize)]
pub struct RecentRequestStats {
    pub total: u64,
    pub success: u64,
    pub errors: u64,
    pub avg_duration: f32,
    pub p50: f32,
    pub p95: f32,
    pub p99: f32,
}

/// Requests per minute time series data point.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RequestsPerMinuteRow {
    pub minute_ts: u32,  // Unix timestamp (start of minute)
    pub request_count: u64,
    pub success_count: u64,
    pub error_count: u64,
    pub avg_latency_ms: f64,
}

/// Simple count result row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RequestCountRow {
    pub cnt: u64,
}

// ============================================================================
// Phase 3: Historical Query Row Types
// ============================================================================

/// Round row with timestamp for historical queries.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct RoundRowWithTimestamp {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub winning_square: u8,
    pub top_miner: String,
    pub total_deployed: u64,
    pub total_winnings: u64,
    pub unique_miners: u32,
    pub motherlode: u64,
    pub motherlode_hit: u8,
    pub created_at: i64,
}

/// Miner stats aggregation row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct MinerStatsRow {
    pub miner_pubkey: String,
    pub total_deployed: u64,
    pub total_sol_earned: u64,
    pub total_ore_earned: u64,
    pub net_sol_change: i64,
    pub rounds_played: u64,
    pub rounds_won: u64,
    pub avg_slots_left: f64,
}

/// Leaderboard row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct LeaderboardRow {
    pub miner_pubkey: String,
    pub value: i64,
    pub rounds_played: u64,
    pub total_deployed: u64,
    pub total_earned: u64,
    pub total_ore: u64,
    pub net_sol: i64,
}

/// Treasury snapshot row.
#[derive(Debug, Clone, Row, Serialize, Deserialize)]
pub struct TreasurySnapshotRow {
    pub round_id: u64,
    pub balance: u64,
    pub motherlode: u64,
    pub total_staked: u64,
    pub total_unclaimed: u64,
    pub total_refined: u64,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_request_log_serialization() {
        let log = RequestLog {
            endpoint: "/api/round".to_string(),
            method: "GET".to_string(),
            status_code: 200,
            duration_ms: 15,
            ip_hash: "abc123".to_string(),
            user_agent: "test-agent".to_string(),
        };
        
        // Just verify it serializes without error
        let _ = serde_json::to_string(&log).unwrap();
    }
}

