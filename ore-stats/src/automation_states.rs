//! Automation State Reconstruction
//!
//! For each deployment instruction, we need to know the automation state at that moment.
//! This affects which squares are deployed on and the amount used.
//!
//! Complex scenarios to handle:
//! - User opens autodeploy, deploys, closes autodeploy, opens new autodeploy with different mask
//! - Multiple automate + deploy instructions in a single transaction
//! - Protocol limit: 1 deploy per square per round (affects how automation mask is applied)

use crate::admin_auth::AuthError;
use crate::app_state::{AppState, AutomationCache, ReconstructedAutomation};
use crate::clickhouse::ClickHouseError;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use evore::ore_api::{self, Automate, OreInstruction, AutomationStrategy};
use serde::{Deserialize, Serialize};
use solana_sdk::{bs58, pubkey::Pubkey};
use std::sync::Arc;
use std::time::Instant;

// ============================================================================
// Structs for ClickHouse
// ============================================================================

#[derive(Debug, Clone, clickhouse::Row, Serialize, Deserialize)]
pub struct DeploymentAutomationStateInsert {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub authority_pubkey: String,
    pub deploy_signature: String,
    pub deploy_ix_index: u8,
    pub deploy_slot: u64,
    
    pub automation_found: bool,
    pub automation_active: bool,
    pub automation_amount: u64,
    pub automation_mask: u64,
    pub automation_strategy: u8,
    pub automation_fee: u64,
    pub automation_executor: String,
    
    pub automate_signature: String,
    pub automate_ix_index: u8,
    pub automate_slot: u64,
    
    pub txns_searched: u32,
    pub pages_fetched: u32,
    pub fetch_duration_ms: u64,
    
    // Balance tracking
    pub automation_balance: u64,      // SOL balance at time of deploy
    pub is_partial_deploy: bool,      // True if balance ran out before all squares
    pub actual_squares_deployed: u8,  // Actual squares deployed (may be < mask count)
    pub actual_mask: u64,             // Actual mask of squares deployed
    pub total_sol_spent: u64,         // Total SOL spent (squares * amount + fee)
}

#[derive(Debug, Clone, clickhouse::Row, Serialize, Deserialize)]
pub struct DeploymentAutomationStateRow {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub authority_pubkey: String,
    pub deploy_signature: String,
    pub deploy_ix_index: u8,
    pub deploy_slot: u64,
    
    pub automation_found: bool,
    pub automation_active: bool,
    pub automation_amount: u64,
    pub automation_mask: u64,
    pub automation_strategy: u8,
    pub automation_fee: u64,
    pub automation_executor: String,
    
    pub automate_signature: String,
    pub automate_ix_index: u8,
    pub automate_slot: u64,
    
    pub txns_searched: u32,
    pub pages_fetched: u32,
    pub fetch_duration_ms: u64,
    
    // Balance tracking
    pub automation_balance: u64,
    pub is_partial_deploy: bool,
    pub actual_squares_deployed: u8,
    pub actual_mask: u64,
    pub total_sol_spent: u64,
    
    pub created_at: i64,
}

// ============================================================================
// PostgreSQL Queue Structs
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationQueueItem {
    pub id: i32,
    pub round_id: i64,
    pub miner_pubkey: String,
    pub authority_pubkey: String,
    pub automation_pda: String,
    pub deploy_signature: String,
    pub deploy_ix_index: i16,
    pub deploy_slot: i64,
    pub status: String,
    pub attempts: i32,
    pub last_error: Option<String>,
    pub txns_searched: Option<i32>,
    pub pages_fetched: Option<i32>,
    pub fetch_duration_ms: Option<i64>,
    pub automation_found: Option<bool>,
    pub priority: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

// ============================================================================
// API Response Types
// ============================================================================

#[derive(Debug, Serialize)]
pub struct AutomationQueueStats {
    pub pending: u64,
    pub processing: u64,
    pub completed: u64,
    pub failed: u64,
    pub total: u64,
    pub avg_fetch_duration_ms: Option<f64>,
    pub avg_txns_searched: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct AutomationQueueResponse {
    pub items: Vec<AutomationQueueItem>,
    pub total: u64,
    pub page: u32,
    pub limit: u32,
}

#[derive(Debug, Deserialize)]
pub struct AutomationQueueQuery {
    pub status: Option<String>,
    pub round_id: Option<u64>,
    pub authority: Option<String>,
    pub page: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct AddToQueueRequest {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub authority_pubkey: String,
    pub deploy_signature: String,
    pub deploy_ix_index: Option<u8>,
    pub deploy_slot: u64,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct AddToQueueResponse {
    pub queued: u32,
    pub already_exists: u32,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProcessResultResponse {
    pub processed: u32,
    pub success: u32,
    pub failed: u32,
    pub details: Vec<ProcessDetail>,
}

#[derive(Debug, Serialize)]
pub struct ProcessDetail {
    pub id: i32,
    pub deploy_signature: String,
    pub success: bool,
    pub automation_found: bool,
    pub automation_active: bool,
    pub txns_searched: u32,
    pub duration_ms: u64,
    pub used_cache: bool,  // Whether we used a previously stored state as starting point
    pub cache_slot: Option<u64>,  // The slot of the cached state we used
    pub error: Option<String>,
}

// ============================================================================
// Database Operations
// ============================================================================

impl crate::clickhouse::ClickHouseClient {
    /// Insert deployment automation state.
    pub async fn insert_deployment_automation_state(
        &self,
        state: DeploymentAutomationStateInsert,
    ) -> Result<(), ClickHouseError> {
        let mut insert = self.client.insert("deployment_automation_states")?;
        insert.write(&state).await?;
        insert.end().await?;
        Ok(())
    }
    
    /// Get automation state for a specific deployment.
    pub async fn get_deployment_automation_state(
        &self,
        deploy_signature: &str,
        deploy_ix_index: u8,
    ) -> Result<Option<DeploymentAutomationStateRow>, ClickHouseError> {
        let row = self.client
            .query(r#"
                SELECT 
                    round_id, miner_pubkey, authority_pubkey, deploy_signature, deploy_ix_index, deploy_slot,
                    automation_found, automation_active, automation_amount, automation_mask,
                    automation_strategy, automation_fee, automation_executor,
                    automate_signature, automate_ix_index, automate_slot,
                    txns_searched, pages_fetched, fetch_duration_ms,
                    automation_balance, is_partial_deploy, actual_squares_deployed, actual_mask, total_sol_spent,
                    created_at
                FROM deployment_automation_states FINAL
                WHERE deploy_signature = ? AND deploy_ix_index = ?
                LIMIT 1
            "#)
            .bind(deploy_signature)
            .bind(deploy_ix_index)
            .fetch_optional::<DeploymentAutomationStateRow>()
            .await?;
        Ok(row)
    }
    
    /// Get deployments missing automation state for a round.
    pub async fn get_deployments_missing_automation(
        &self,
        round_id: u64,
    ) -> Result<Vec<DeploymentMissingState>, ClickHouseError> {
        let rows = self.client
            .query(r#"
                SELECT 
                    d.round_id,
                    d.miner_pubkey,
                    d.slot as deploy_slot,
                    d.signature as deploy_signature,
                    0 as deploy_ix_index
                FROM deployments d
                LEFT JOIN deployment_automation_states das
                    ON d.signature = das.deploy_signature AND das.deploy_ix_index = 0
                WHERE d.round_id = ? AND das.deploy_signature IS NULL
                ORDER BY d.slot
            "#)
            .bind(round_id)
            .fetch_all::<DeploymentMissingState>()
            .await?;
        Ok(rows)
    }
    
    /// Get a previously stored automation state for an authority that we can use
    /// to avoid re-scanning transaction history.
    /// 
    /// The key insight:
    /// - If we have stored state from deploy_slot=800 where automate_slot=300
    /// - And we're processing a deployment at slot 600 (between 300 and 800)
    /// - We KNOW the automation state at 600 is the same as at 300 (no change occurred)
    /// - So we can just copy the data, no transaction scanning needed!
    /// 
    /// Returns the stored state where: automate_slot < target_slot < deploy_slot
    /// Note: Strictly less than deploy_slot because at deploy_slot there could be
    /// an Automate instruction AFTER the Deploy in the same transaction.
    pub async fn get_reusable_automation_state(
        &self,
        authority_pubkey: &str,
        target_deploy_slot: u64,
    ) -> Result<Option<DeploymentAutomationStateRow>, ClickHouseError> {
        // Find a stored state where:
        // - automate_slot < target_deploy_slot (automation was set before our target)
        // - deploy_slot > target_deploy_slot (strictly greater - we need to re-scan deploy_slot)
        let row = self.client
            .query(r#"
                SELECT 
                    round_id, miner_pubkey, authority_pubkey, deploy_signature, deploy_ix_index, deploy_slot,
                    automation_found, automation_active, automation_amount, automation_mask,
                    automation_strategy, automation_fee, automation_executor,
                    automate_signature, automate_ix_index, automate_slot,
                    txns_searched, pages_fetched, fetch_duration_ms,
                    automation_balance, is_partial_deploy, actual_squares_deployed, actual_mask, total_sol_spent,
                    created_at
                FROM deployment_automation_states FINAL
                WHERE authority_pubkey = ? 
                  AND automation_found = true
                  AND automate_slot < ?
                  AND deploy_slot > ?
                ORDER BY deploy_slot DESC
                LIMIT 1
            "#)
            .bind(authority_pubkey)
            .bind(target_deploy_slot)
            .bind(target_deploy_slot)
            .fetch_optional::<DeploymentAutomationStateRow>()
            .await?;
        Ok(row)
    }
    
    /// Get the latest stored automation state for an authority (by automate_slot).
    /// Used when we need to scan backwards but can stop early if we reach a known state.
    pub async fn get_latest_automation_state_for_authority(
        &self,
        authority_pubkey: &str,
    ) -> Result<Option<DeploymentAutomationStateRow>, ClickHouseError> {
        let row = self.client
            .query(r#"
                SELECT 
                    round_id, miner_pubkey, authority_pubkey, deploy_signature, deploy_ix_index, deploy_slot,
                    automation_found, automation_active, automation_amount, automation_mask,
                    automation_strategy, automation_fee, automation_executor,
                    automate_signature, automate_ix_index, automate_slot,
                    txns_searched, pages_fetched, fetch_duration_ms,
                    automation_balance, is_partial_deploy, actual_squares_deployed, actual_mask, total_sol_spent,
                    created_at
                FROM deployment_automation_states FINAL
                WHERE authority_pubkey = ? 
                  AND automation_found = true
                ORDER BY automate_slot DESC
                LIMIT 1
            "#)
            .bind(authority_pubkey)
            .fetch_optional::<DeploymentAutomationStateRow>()
            .await?;
        Ok(row)
    }
    
    /// Get automation state fetch statistics.
    pub async fn get_automation_fetch_stats(&self) -> Result<AutomationFetchStats, ClickHouseError> {
        let row = self.client
            .query(r#"
                SELECT
                    count() as total_fetched,
                    countIf(automation_found) as found_count,
                    countIf(automation_active) as active_count,
                    if(count() > 0, avg(txns_searched), 0) as avg_txns_searched,
                    if(count() > 0, avg(fetch_duration_ms), 0) as avg_duration_ms,
                    max(txns_searched) as max_txns_searched,
                    max(fetch_duration_ms) as max_duration_ms,
                    countIf(is_partial_deploy) as partial_deploy_count,
                    sum(total_sol_spent) as total_sol_tracked
                FROM deployment_automation_states FINAL
            "#)
            .fetch_one::<AutomationFetchStats>()
            .await?;
        Ok(row)
    }
}

#[derive(Debug, Clone, clickhouse::Row, Serialize, Deserialize)]
pub struct DeploymentMissingState {
    pub round_id: u64,
    pub miner_pubkey: String,
    pub deploy_slot: u64,
    pub deploy_signature: String,
    pub deploy_ix_index: u8,
}

#[derive(Debug, Clone, clickhouse::Row, Serialize, Deserialize)]
pub struct AutomationFetchStats {
    pub total_fetched: u64,
    pub found_count: u64,
    pub active_count: u64,
    pub avg_txns_searched: f64,
    pub avg_duration_ms: f64,
    pub max_txns_searched: u32,
    pub max_duration_ms: u64,
    pub partial_deploy_count: u64,
    pub total_sol_tracked: u64,
}

// ============================================================================
// Queue Management
// ============================================================================

/// Get queue statistics from PostgreSQL.
pub async fn get_queue_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AutomationQueueStats>, (StatusCode, Json<AuthError>)> {
    let row = sqlx::query_as::<_, (i64, i64, i64, i64, Option<f64>, Option<f64>)>(r#"
        SELECT 
            COUNT(*) FILTER (WHERE status = 'pending') as pending,
            COUNT(*) FILTER (WHERE status = 'processing') as processing,
            COUNT(*) FILTER (WHERE status = 'completed') as completed,
            COUNT(*) FILTER (WHERE status = 'failed') as failed,
            AVG(fetch_duration_ms) FILTER (WHERE status = 'completed') as avg_duration,
            AVG(txns_searched) FILTER (WHERE status = 'completed') as avg_txns
        FROM automation_state_queue
    "#)
    .fetch_one(&state.postgres)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get queue stats: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: format!("DB error: {}", e) }),
        )
    })?;
    
    Ok(Json(AutomationQueueStats {
        pending: row.0 as u64,
        processing: row.1 as u64,
        completed: row.2 as u64,
        failed: row.3 as u64,
        total: (row.0 + row.1 + row.2 + row.3) as u64,
        avg_fetch_duration_ms: row.4,
        avg_txns_searched: row.5,
    }))
}

/// Get queue items with filtering.
pub async fn get_queue_items(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AutomationQueueQuery>,
) -> Result<Json<AutomationQueueResponse>, (StatusCode, Json<AuthError>)> {
    let page = query.page.unwrap_or(1);
    let limit = query.limit.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * limit;
    
    let mut where_clauses = Vec::new();
    let mut bind_values: Vec<String> = Vec::new();
    
    if let Some(status) = &query.status {
        where_clauses.push(format!("status = ${}", bind_values.len() + 1));
        bind_values.push(status.clone());
    }
    if let Some(round_id) = query.round_id {
        where_clauses.push(format!("round_id = ${}", bind_values.len() + 1));
        bind_values.push(round_id.to_string());
    }
    if let Some(authority) = &query.authority {
        where_clauses.push(format!("authority_pubkey = ${}", bind_values.len() + 1));
        bind_values.push(authority.clone());
    }
    
    let where_sql = if where_clauses.is_empty() {
        "1=1".to_string()
    } else {
        where_clauses.join(" AND ")
    };
    
    // Build a simple query without dynamic bind - for now just get all
    let items = sqlx::query_as::<_, AutomationQueueItemRow>(
        &format!(
            r#"
            SELECT id, round_id, miner_pubkey, authority_pubkey, automation_pda,
                   deploy_signature, deploy_ix_index, deploy_slot, status::text,
                   attempts, last_error, txns_searched, pages_fetched, fetch_duration_ms,
                   automation_found, priority, created_at, updated_at, started_at, completed_at
            FROM automation_state_queue
            ORDER BY priority ASC, created_at ASC
            LIMIT {} OFFSET {}
            "#,
            limit, offset
        )
    )
    .fetch_all(&state.postgres)
    .await
    .map_err(|e| {
        tracing::error!("Failed to get queue items: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: format!("DB error: {}", e) }),
        )
    })?;
    
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM automation_state_queue")
        .fetch_one(&state.postgres)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("DB error: {}", e) }),
            )
        })?;
    
    Ok(Json(AutomationQueueResponse {
        items: items.into_iter().map(|r| r.into()).collect(),
        total: total as u64,
        page,
        limit,
    }))
}

#[derive(Debug, sqlx::FromRow)]
struct AutomationQueueItemRow {
    id: i32,
    round_id: i64,
    miner_pubkey: String,
    authority_pubkey: String,
    automation_pda: String,
    deploy_signature: String,
    deploy_ix_index: i16,
    deploy_slot: i64,
    status: String,
    attempts: i32,
    last_error: Option<String>,
    txns_searched: Option<i32>,
    pages_fetched: Option<i32>,
    fetch_duration_ms: Option<i64>,
    automation_found: Option<bool>,
    priority: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<AutomationQueueItemRow> for AutomationQueueItem {
    fn from(r: AutomationQueueItemRow) -> Self {
        Self {
            id: r.id,
            round_id: r.round_id,
            miner_pubkey: r.miner_pubkey,
            authority_pubkey: r.authority_pubkey,
            automation_pda: r.automation_pda,
            deploy_signature: r.deploy_signature,
            deploy_ix_index: r.deploy_ix_index,
            deploy_slot: r.deploy_slot,
            status: r.status,
            attempts: r.attempts,
            last_error: r.last_error,
            txns_searched: r.txns_searched,
            pages_fetched: r.pages_fetched,
            fetch_duration_ms: r.fetch_duration_ms,
            automation_found: r.automation_found,
            priority: r.priority,
            created_at: r.created_at,
            updated_at: r.updated_at,
            started_at: r.started_at,
            completed_at: r.completed_at,
        }
    }
}

/// Add items to the queue.
pub async fn add_to_queue(
    State(state): State<Arc<AppState>>,
    Json(requests): Json<Vec<AddToQueueRequest>>,
) -> Result<Json<AddToQueueResponse>, (StatusCode, Json<AuthError>)> {
    let mut queued = 0u32;
    let mut already_exists = 0u32;
    let mut errors = Vec::new();
    
    for req in requests {
        // Derive automation PDA
        let authority = match Pubkey::try_from(req.authority_pubkey.as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                errors.push(format!("Invalid authority pubkey: {}", req.authority_pubkey));
                continue;
            }
        };
        let (automation_pda, _) = ore_api::automation_pda(authority);
        
        let result = sqlx::query(r#"
            INSERT INTO automation_state_queue 
            (round_id, miner_pubkey, authority_pubkey, automation_pda, deploy_signature, 
             deploy_ix_index, deploy_slot, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (round_id, deploy_signature, deploy_ix_index) DO NOTHING
        "#)
        .bind(req.round_id as i64)
        .bind(&req.miner_pubkey)
        .bind(&req.authority_pubkey)
        .bind(automation_pda.to_string())
        .bind(&req.deploy_signature)
        .bind(req.deploy_ix_index.unwrap_or(0) as i16)
        .bind(req.deploy_slot as i64)
        .bind(req.priority.unwrap_or(1000))
        .execute(&state.postgres)
        .await;
        
        match result {
            Ok(r) if r.rows_affected() > 0 => queued += 1,
            Ok(_) => already_exists += 1,
            Err(e) => errors.push(format!("Failed to queue {}: {}", req.deploy_signature, e)),
        }
    }
    
    Ok(Json(AddToQueueResponse {
        queued,
        already_exists,
        errors,
    }))
}

/// Queue all deployments missing automation state for a round.
pub async fn queue_missing_for_round(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<AddToQueueResponse>, (StatusCode, Json<AuthError>)> {
    // First get deployments missing automation state
    let missing = state.clickhouse
        .get_deployments_missing_automation(round_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    if missing.is_empty() {
        return Ok(Json(AddToQueueResponse {
            queued: 0,
            already_exists: 0,
            errors: vec![],
        }));
    }
    
    // Get authority for each miner from stored deployments
    // For now we'll need to look up authority from deployments table
    let mut requests = Vec::new();
    
    for m in missing {
        // Query authority from deployments table
        let authority: Option<String> = state.clickhouse.client
            .query(r#"
                SELECT authority FROM deployments FINAL
                WHERE round_id = ? AND signature = ?
                LIMIT 1
            "#)
            .bind(round_id)
            .bind(&m.deploy_signature)
            .fetch_optional::<String>()
            .await
            .ok()
            .flatten();
        
        if let Some(auth) = authority {
            requests.push(AddToQueueRequest {
                round_id,
                miner_pubkey: m.miner_pubkey,
                authority_pubkey: auth,
                deploy_signature: m.deploy_signature,
                deploy_ix_index: Some(m.deploy_ix_index),
                deploy_slot: m.deploy_slot,
                priority: None,
            });
        }
    }
    
    // Add them to queue
    let mut queued = 0u32;
    let mut already_exists = 0u32;
    let mut errors = Vec::new();
    
    for req in requests {
        let authority = match Pubkey::try_from(req.authority_pubkey.as_str()) {
            Ok(pk) => pk,
            Err(_) => {
                errors.push(format!("Invalid authority: {}", req.authority_pubkey));
                continue;
            }
        };
        let (automation_pda, _) = ore_api::automation_pda(authority);
        
        let result = sqlx::query(r#"
            INSERT INTO automation_state_queue 
            (round_id, miner_pubkey, authority_pubkey, automation_pda, deploy_signature, 
             deploy_ix_index, deploy_slot, priority)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (round_id, deploy_signature, deploy_ix_index) DO NOTHING
        "#)
        .bind(req.round_id as i64)
        .bind(&req.miner_pubkey)
        .bind(&req.authority_pubkey)
        .bind(automation_pda.to_string())
        .bind(&req.deploy_signature)
        .bind(req.deploy_ix_index.unwrap_or(0) as i16)
        .bind(req.deploy_slot as i64)
        .bind(req.priority.unwrap_or(1000))
        .execute(&state.postgres)
        .await;
        
        match result {
            Ok(r) if r.rows_affected() > 0 => queued += 1,
            Ok(_) => already_exists += 1,
            Err(e) => errors.push(format!("DB error: {}", e)),
        }
    }
    
    Ok(Json(AddToQueueResponse {
        queued,
        already_exists,
        errors,
    }))
}

// ============================================================================
// Processing
// ============================================================================

/// Process next N items from the queue.
pub async fn process_queue(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ProcessQueueParams>,
) -> Result<Json<ProcessResultResponse>, (StatusCode, Json<AuthError>)> {
    let count = params.count.unwrap_or(5).min(50);
    
    // Get pending items
    let items = sqlx::query_as::<_, AutomationQueueItemRow>(r#"
        UPDATE automation_state_queue
        SET status = 'processing', started_at = NOW(), attempts = attempts + 1
        WHERE id IN (
            SELECT id FROM automation_state_queue
            WHERE status = 'pending'
            ORDER BY priority ASC, created_at ASC
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING *
    "#)
    .bind(count as i32)
    .fetch_all(&state.postgres)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: format!("DB error: {}", e) }),
        )
    })?;
    
    let mut results = ProcessResultResponse {
        processed: items.len() as u32,
        success: 0,
        failed: 0,
        details: Vec::new(),
    };
    
    for item in items {
        let detail = process_single_item(&state, item).await;
        if detail.success {
            results.success += 1;
        } else {
            results.failed += 1;
        }
        results.details.push(detail);
    }
    
    Ok(Json(results))
}

#[derive(Debug, Deserialize)]
pub struct ProcessQueueParams {
    pub count: Option<u32>,
}

async fn process_single_item(
    state: &AppState,
    item: AutomationQueueItemRow,
) -> ProcessDetail {
    let start = Instant::now();
    
    let authority = match Pubkey::try_from(item.authority_pubkey.as_str()) {
        Ok(pk) => pk,
        Err(e) => {
            update_queue_failed(state, item.id, &format!("Invalid authority: {}", e)).await;
            return ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: false,
                automation_active: false,
                txns_searched: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                used_cache: false,
                cache_slot: None,
                error: Some(format!("Invalid authority: {}", e)),
            };
        }
    };
    
    // Optimization: Check if we can reuse an existing automation state directly
    // If we have a stored state where automate_slot < target_slot < deploy_slot, no scanning needed!
    // Note: Strictly less than deploy_slot because at deploy_slot there could be
    // an Automate instruction AFTER the Deploy in the same transaction.
    let reusable = state.clickhouse
        .get_reusable_automation_state(&item.authority_pubkey, item.deploy_slot as u64)
        .await;
    
    if let Ok(Some(existing)) = reusable {
        // Perfect! We can just copy the automation state, no transaction scanning needed
        tracing::debug!(
            "Reusing automation state for {} from deploy_slot {} (automate_slot {}) for new deploy at slot {}",
            item.authority_pubkey, existing.deploy_slot, existing.automate_slot, item.deploy_slot
        );
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        // Store a new record with the same automation data but different deploy info
        // Note: Balance fields are copied - for cached reuse between rounds this is approximate
        let insert = DeploymentAutomationStateInsert {
            round_id: item.round_id as u64,
            miner_pubkey: item.miner_pubkey.clone(),
            authority_pubkey: item.authority_pubkey.clone(),
            deploy_signature: item.deploy_signature.clone(),
            deploy_ix_index: item.deploy_ix_index as u8,
            deploy_slot: item.deploy_slot as u64,
            automation_found: true,
            automation_active: existing.automation_active,
            automation_amount: existing.automation_amount,
            automation_mask: existing.automation_mask,
            automation_strategy: existing.automation_strategy,
            automation_fee: existing.automation_fee,
            automation_executor: existing.automation_executor.clone(),
            automate_signature: existing.automate_signature.clone(),
            automate_ix_index: existing.automate_ix_index,
            automate_slot: existing.automate_slot,
            txns_searched: 0,  // No scanning!
            pages_fetched: 0,
            fetch_duration_ms: duration_ms,
            automation_balance: existing.automation_balance,
            is_partial_deploy: existing.is_partial_deploy,
            actual_squares_deployed: existing.actual_squares_deployed,
            actual_mask: existing.actual_mask,
            total_sol_spent: existing.total_sol_spent,
        };
        
        if let Err(e) = state.clickhouse.insert_deployment_automation_state(insert).await {
            update_queue_failed(state, item.id, &format!("ClickHouse insert failed: {}", e)).await;
            return ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: true,
                automation_active: existing.automation_active,
                txns_searched: 0,
                duration_ms,
                used_cache: true,
                cache_slot: Some(existing.automate_slot),
                error: Some(format!("ClickHouse insert failed: {}", e)),
            };
        }
        
        // Update queue as completed
        let _ = sqlx::query(r#"
            UPDATE automation_state_queue
            SET status = 'completed', completed_at = NOW(),
                fetch_duration_ms = $2, automation_found = true, txns_searched = 0
            WHERE id = $1
        "#)
        .bind(item.id)
        .bind(duration_ms as i64)
        .execute(&state.postgres)
        .await;
        
        return ProcessDetail {
            id: item.id,
            deploy_signature: item.deploy_signature,
            success: true,
            automation_found: true,
            automation_active: existing.automation_active,
            txns_searched: 0,
            duration_ms,
            used_cache: true,
            cache_slot: Some(existing.automate_slot),
            error: None,
        };
    }
    
    // No directly reusable state - need to scan backwards with full balance tracking
    // Check if we have any stored state we can use as an early-stop point
    let fallback_state = state.clickhouse
        .get_latest_automation_state_for_authority(&item.authority_pubkey)
        .await
        .ok()
        .flatten();
    
    // Calculate stop_at_slot: one slot BEFORE the cached deploy_slot
    let stop_at_slot = fallback_state.as_ref().map(|fb| fb.deploy_slot.saturating_sub(1));
    
    // Perform comprehensive backwards scan with balance tracking
    let mut helius = state.helius.write().await;
    let scan_result = helius
        .scan_automation_history_with_balance(
            &authority, 
            item.deploy_slot as u64, 
            stop_at_slot,
        )
        .await;
    drop(helius);
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    match scan_result {
        Ok(scan) => {
            // Find the calculated deployment for our target slot/signature
            let target_deploy = scan.calculated_deploys.iter()
                .find(|d| d.slot == item.deploy_slot as u64 && d.signature == item.deploy_signature);
            
            let (automation_found, automation_active, automation_balance, is_partial_deploy, 
                 actual_squares_deployed, actual_mask, total_sol_spent,
                 automation_amount, automation_mask, automation_strategy, automation_fee, 
                 automation_executor, automate_signature, automate_ix_index, automate_slot) =
            if let Some(calc) = target_deploy {
                // Found exact match with balance tracking
                let open = scan.automate_open.as_ref();
                (
                    true,
                    true,
                    calc.balance_before,
                    calc.is_partial,
                    calc.actual_squares,
                    calc.actual_mask,
                    calc.total_spent,
                    open.map(|o| o.amount).unwrap_or(0),
                    open.map(|o| o.mask).unwrap_or(0),
                    open.map(|o| o.strategy).unwrap_or(0),
                    open.map(|o| o.fee).unwrap_or(0),
                    open.map(|o| o.executor.to_string()).unwrap_or_default(),
                    open.map(|o| o.signature.clone()).unwrap_or_default(),
                    open.map(|o| o.ix_index).unwrap_or(0),
                    open.map(|o| o.slot).unwrap_or(0),
                )
            } else if let Some(open) = &scan.automate_open {
                // Have automate open but no calculated deploy (could be a close after deploy)
                // Use the automation settings with estimated values
                let mask = open.mask;
                let actual_squares = crate::helius_api::count_squares(mask);
                let total_spent = actual_squares as u64 * open.amount + if actual_squares > 0 { open.fee } else { 0 };
                (
                    true,
                    true,
                    0, // Unknown balance
                    false,
                    actual_squares,
                    mask,
                    total_spent,
                    open.amount,
                    open.mask,
                    open.strategy,
                    open.fee,
                    open.executor.to_string(),
                    open.signature.clone(),
                    open.ix_index,
                    open.slot,
                )
            } else if let Some(fb) = &fallback_state {
                // No automation found in scan, but we have fallback from previous scan
                (
                    true,
                    fb.automation_active,
                    fb.automation_balance,
                    fb.is_partial_deploy,
                    fb.actual_squares_deployed,
                    fb.actual_mask,
                    fb.total_sol_spent,
                    fb.automation_amount,
                    fb.automation_mask,
                    fb.automation_strategy,
                    fb.automation_fee,
                    fb.automation_executor.clone(),
                    fb.automate_signature.clone(),
                    fb.automate_ix_index,
                    fb.automate_slot,
                )
            } else {
                // No automation found at all
                (false, false, 0, false, 0, 0, 0, 0, 0, 0, 0, String::new(), String::new(), 0, 0)
            };
            
            // Store in ClickHouse
            let insert = DeploymentAutomationStateInsert {
                round_id: item.round_id as u64,
                miner_pubkey: item.miner_pubkey.clone(),
                authority_pubkey: item.authority_pubkey.clone(),
                deploy_signature: item.deploy_signature.clone(),
                deploy_ix_index: item.deploy_ix_index as u8,
                deploy_slot: item.deploy_slot as u64,
                automation_found,
                automation_active,
                automation_amount,
                automation_mask,
                automation_strategy,
                automation_fee,
                automation_executor,
                automate_signature,
                automate_ix_index,
                automate_slot,
                txns_searched: scan.txns_searched,
                pages_fetched: scan.pages_fetched,
                fetch_duration_ms: duration_ms,
                automation_balance,
                is_partial_deploy,
                actual_squares_deployed,
                actual_mask,
                total_sol_spent,
            };
            
            let used_cache = fallback_state.is_some() && scan.automate_open.is_none();
            let cache_slot = if used_cache { fallback_state.as_ref().map(|f| f.automate_slot) } else { None };
            
            if let Err(e) = state.clickhouse.insert_deployment_automation_state(insert).await {
                update_queue_failed(state, item.id, &format!("ClickHouse insert failed: {}", e)).await;
                return ProcessDetail {
                    id: item.id,
                    deploy_signature: item.deploy_signature,
                    success: false,
                    automation_found,
                    automation_active,
                    txns_searched: 0,
                    duration_ms,
                    used_cache,
                    cache_slot,
                    error: Some(format!("ClickHouse insert failed: {}", e)),
                };
            }
            
            // Update queue as completed
            let _ = sqlx::query(r#"
                UPDATE automation_state_queue
                SET status = 'completed', completed_at = NOW(),
                    fetch_duration_ms = $2, automation_found = $3
                WHERE id = $1
            "#)
            .bind(item.id)
            .bind(duration_ms as i64)
            .bind(automation_found)
            .execute(&state.postgres)
            .await;
            
            ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: true,
                automation_found,
                automation_active,
                txns_searched: scan.txns_searched,
                duration_ms,
                used_cache,
                cache_slot,
                error: None,
            }
        }
        Err(e) => {
            update_queue_failed(state, item.id, &format!("Helius error: {}", e)).await;
            ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: false,
                automation_active: false,
                txns_searched: 0,
                duration_ms,
                used_cache: false,
                cache_slot: None,
                error: Some(format!("Helius error: {}", e)),
            }
        }
    }
}

async fn update_queue_failed(state: &AppState, id: i32, error: &str) {
    let _ = sqlx::query(r#"
        UPDATE automation_state_queue
        SET status = 'failed', last_error = $2
        WHERE id = $1
    "#)
    .bind(id)
    .bind(error)
    .execute(&state.postgres)
    .await;
}

/// Retry failed items.
pub async fn retry_failed(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<AuthError>)> {
    let result = sqlx::query(r#"
        UPDATE automation_state_queue
        SET status = 'pending', last_error = NULL
        WHERE status = 'failed' AND attempts < 5
    "#)
    .execute(&state.postgres)
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(AuthError { error: format!("DB error: {}", e) }),
        )
    })?;
    
    Ok(Json(serde_json::json!({
        "retried": result.rows_affected()
    })))
}

/// Get ClickHouse fetch statistics.
pub async fn get_fetch_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<AutomationFetchStats>, (StatusCode, Json<AuthError>)> {
    let stats = state.clickhouse
        .get_automation_fetch_stats()
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(AuthError { error: format!("ClickHouse error: {}", e) }),
            )
        })?;
    
    Ok(Json(stats))
}

/// Get live task stats.
pub async fn get_live_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<crate::app_state::AutomationTaskStats>, (StatusCode, Json<AuthError>)> {
    let stats = state.automation_task_stats.read().await.clone();
    Ok(Json(stats))
}

// ============================================================================
// Background Task
// ============================================================================

/// Spawn the automation state processing background task.
/// Runs continuously, processing items from the queue in FIFO order.
pub fn spawn_automation_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        tracing::info!("Starting automation state reconstruction background task");
        
        loop {
            // Wait a bit between processing cycles
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            
            // Check for pending items
            let item = get_next_pending_item(&state).await;
            
            match item {
                Ok(Some(item)) => {
                    // Update live stats - starting
                    {
                        let mut stats = state.automation_task_stats.write().await;
                        stats.is_running = true;
                        stats.current_item_id = Some(item.id);
                        stats.current_signature = Some(item.deploy_signature.clone());
                        stats.current_authority = Some(item.authority_pubkey.clone());
                        stats.txns_searched_so_far = 0;
                        stats.pages_fetched_so_far = 0;
                        stats.elapsed_ms = 0;
                        stats.last_updated = chrono::Utc::now();
                    }
                    
                    let start = Instant::now();
                    
                    // Process the item
                    let result = process_single_item_with_stats(&state, item).await;
                    
                    // Update live stats - completed
                    {
                        let mut stats = state.automation_task_stats.write().await;
                        stats.is_running = false;
                        stats.current_item_id = None;
                        stats.current_signature = None;
                        stats.current_authority = None;
                        stats.elapsed_ms = start.elapsed().as_millis() as u64;
                        stats.items_processed_this_session += 1;
                        if result.success {
                            stats.items_succeeded_this_session += 1;
                        } else {
                            stats.items_failed_this_session += 1;
                        }
                        stats.last_updated = chrono::Utc::now();
                    }
                    
                    tracing::debug!(
                        "Processed automation state item {}: {} in {}ms",
                        result.id, if result.success { "success" } else { "failed" }, result.duration_ms
                    );
                }
                Ok(None) => {
                    // No pending items, mark as not running
                    let mut stats = state.automation_task_stats.write().await;
                    stats.is_running = false;
                }
                Err(e) => {
                    tracing::error!("Failed to get next pending item: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
            }
        }
    });
}

async fn get_next_pending_item(state: &AppState) -> Result<Option<AutomationQueueItemRow>, String> {
    // Get and claim the next pending item
    let item = sqlx::query_as::<_, AutomationQueueItemRow>(r#"
        UPDATE automation_state_queue
        SET status = 'processing', started_at = NOW(), attempts = attempts + 1
        WHERE id = (
            SELECT id FROM automation_state_queue
            WHERE status = 'pending'
            ORDER BY priority ASC, created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING *
    "#)
    .fetch_optional(&state.postgres)
    .await
    .map_err(|e| format!("DB error: {}", e))?;
    
    Ok(item)
}

async fn process_single_item_with_stats(
    state: &AppState,
    item: AutomationQueueItemRow,
) -> ProcessDetail {
    let start = Instant::now();
    
    let authority = match Pubkey::try_from(item.authority_pubkey.as_str()) {
        Ok(pk) => pk,
        Err(e) => {
            update_queue_failed(state, item.id, &format!("Invalid authority: {}", e)).await;
            return ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: false,
                automation_active: false,
                txns_searched: 0,
                duration_ms: start.elapsed().as_millis() as u64,
                used_cache: false,
                cache_slot: None,
                error: Some(format!("Invalid authority: {}", e)),
            };
        }
    };
    
    // Optimization: Check if we can reuse an existing automation state directly
    // If we have a stored state where automate_slot < target_slot < deploy_slot, no scanning needed!
    // Note: Strictly less than deploy_slot because at deploy_slot there could be
    // an Automate instruction AFTER the Deploy in the same transaction.
    let reusable = state.clickhouse
        .get_reusable_automation_state(&item.authority_pubkey, item.deploy_slot as u64)
        .await;
    
    if let Ok(Some(existing)) = reusable {
        // Perfect! We can just copy the automation state, no transaction scanning needed
        tracing::info!(
            "Reusing automation state for {} from deploy_slot {} (automate_slot {}) for new deploy at slot {}",
            item.authority_pubkey, existing.deploy_slot, existing.automate_slot, item.deploy_slot
        );
        
        let duration_ms = start.elapsed().as_millis() as u64;
        
        // Store a new record with the same automation data but different deploy info
        // Note: Balance fields are copied - for cached reuse between rounds this is approximate
        let insert = DeploymentAutomationStateInsert {
            round_id: item.round_id as u64,
            miner_pubkey: item.miner_pubkey.clone(),
            authority_pubkey: item.authority_pubkey.clone(),
            deploy_signature: item.deploy_signature.clone(),
            deploy_ix_index: item.deploy_ix_index as u8,
            deploy_slot: item.deploy_slot as u64,
            automation_found: true,
            automation_active: existing.automation_active,
            automation_amount: existing.automation_amount,
            automation_mask: existing.automation_mask,
            automation_strategy: existing.automation_strategy,
            automation_fee: existing.automation_fee,
            automation_executor: existing.automation_executor.clone(),
            automate_signature: existing.automate_signature.clone(),
            automate_ix_index: existing.automate_ix_index,
            automate_slot: existing.automate_slot,
            txns_searched: 0,  // No scanning!
            pages_fetched: 0,
            fetch_duration_ms: duration_ms,
            automation_balance: existing.automation_balance,
            is_partial_deploy: existing.is_partial_deploy,
            actual_squares_deployed: existing.actual_squares_deployed,
            actual_mask: existing.actual_mask,
            total_sol_spent: existing.total_sol_spent,
        };
        
        if let Err(e) = state.clickhouse.insert_deployment_automation_state(insert).await {
            update_queue_failed(state, item.id, &format!("ClickHouse insert failed: {}", e)).await;
            return ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: true,
                automation_active: existing.automation_active,
                txns_searched: 0,
                duration_ms,
                used_cache: true,
                cache_slot: Some(existing.automate_slot),
                error: Some(format!("ClickHouse insert failed: {}", e)),
            };
        }
        
        // Update queue as completed
        let _ = sqlx::query(r#"
            UPDATE automation_state_queue
            SET status = 'completed', completed_at = NOW(),
                fetch_duration_ms = $2, automation_found = true, txns_searched = 0
            WHERE id = $1
        "#)
        .bind(item.id)
        .bind(duration_ms as i64)
        .execute(&state.postgres)
        .await;
        
        return ProcessDetail {
            id: item.id,
            deploy_signature: item.deploy_signature,
            success: true,
            automation_found: true,
            automation_active: existing.automation_active,
            txns_searched: 0,
            duration_ms,
            used_cache: true,
            cache_slot: Some(existing.automate_slot),
            error: None,
        };
    }
    
    // No directly reusable state - need to scan backwards with full balance tracking
    let fallback_state = state.clickhouse
        .get_latest_automation_state_for_authority(&item.authority_pubkey)
        .await
        .ok()
        .flatten();
    
    let stop_at_slot = fallback_state.as_ref().map(|fb| fb.deploy_slot.saturating_sub(1));
    
    if let Some(stop) = stop_at_slot {
        tracing::debug!(
            "Scanning backwards for {} from slot {}, will stop at slot {} (cached deploy_slot {} - 1)",
            item.authority_pubkey, item.deploy_slot, stop, stop + 1
        );
    } else {
        tracing::debug!(
            "Scanning backwards for {} from slot {}, no early stop point",
            item.authority_pubkey, item.deploy_slot
        );
    }
    
    // Perform comprehensive backwards scan with balance tracking
    let mut helius = state.helius.write().await;
    let scan_result = helius
        .scan_automation_history_with_balance(
            &authority, 
            item.deploy_slot as u64, 
            stop_at_slot,
        )
        .await;
    drop(helius);
    
    let duration_ms = start.elapsed().as_millis() as u64;
    
    match scan_result {
        Ok(scan) => {
            // Update live stats
            {
                let mut stats = state.automation_task_stats.write().await;
                stats.txns_searched_so_far += scan.txns_searched;
                stats.pages_fetched_so_far += scan.pages_fetched;
                stats.last_updated = chrono::Utc::now();
            }
            
            // Find the calculated deployment for our target slot/signature
            let target_deploy = scan.calculated_deploys.iter()
                .find(|d| d.slot == item.deploy_slot as u64 && d.signature == item.deploy_signature);
            
            let (automation_found, automation_active, automation_balance, is_partial_deploy, 
                 actual_squares_deployed, actual_mask, total_sol_spent,
                 automation_amount, automation_mask, automation_strategy, automation_fee, 
                 automation_executor, automate_signature, automate_ix_index, automate_slot) =
            if let Some(calc) = target_deploy {
                let open = scan.automate_open.as_ref();
                (
                    true,
                    true,
                    calc.balance_before,
                    calc.is_partial,
                    calc.actual_squares,
                    calc.actual_mask,
                    calc.total_spent,
                    open.map(|o| o.amount).unwrap_or(0),
                    open.map(|o| o.mask).unwrap_or(0),
                    open.map(|o| o.strategy).unwrap_or(0),
                    open.map(|o| o.fee).unwrap_or(0),
                    open.map(|o| o.executor.to_string()).unwrap_or_default(),
                    open.map(|o| o.signature.clone()).unwrap_or_default(),
                    open.map(|o| o.ix_index).unwrap_or(0),
                    open.map(|o| o.slot).unwrap_or(0),
                )
            } else if let Some(open) = &scan.automate_open {
                let mask = open.mask;
                let actual_squares = crate::helius_api::count_squares(mask);
                let total_spent = actual_squares as u64 * open.amount + if actual_squares > 0 { open.fee } else { 0 };
                (
                    true,
                    true,
                    0,
                    false,
                    actual_squares,
                    mask,
                    total_spent,
                    open.amount,
                    open.mask,
                    open.strategy,
                    open.fee,
                    open.executor.to_string(),
                    open.signature.clone(),
                    open.ix_index,
                    open.slot,
                )
            } else if let Some(fb) = &fallback_state {
                tracing::debug!(
                    "Backwards scan found nothing, using cached state from automate_slot {}",
                    fb.automate_slot
                );
                (
                    true,
                    fb.automation_active,
                    fb.automation_balance,
                    fb.is_partial_deploy,
                    fb.actual_squares_deployed,
                    fb.actual_mask,
                    fb.total_sol_spent,
                    fb.automation_amount,
                    fb.automation_mask,
                    fb.automation_strategy,
                    fb.automation_fee,
                    fb.automation_executor.clone(),
                    fb.automate_signature.clone(),
                    fb.automate_ix_index,
                    fb.automate_slot,
                )
            } else {
                (false, false, 0, false, 0, 0, 0, 0, 0, 0, 0, String::new(), String::new(), 0, 0)
            };
            
            let used_cache = fallback_state.is_some() && scan.automate_open.is_none();
            let cache_slot = if used_cache { fallback_state.as_ref().map(|f| f.automate_slot) } else { None };
            
            // Store in ClickHouse
            let insert = DeploymentAutomationStateInsert {
                round_id: item.round_id as u64,
                miner_pubkey: item.miner_pubkey.clone(),
                authority_pubkey: item.authority_pubkey.clone(),
                deploy_signature: item.deploy_signature.clone(),
                deploy_ix_index: item.deploy_ix_index as u8,
                deploy_slot: item.deploy_slot as u64,
                automation_found,
                automation_active,
                automation_amount,
                automation_mask,
                automation_strategy,
                automation_fee,
                automation_executor,
                automate_signature,
                automate_ix_index,
                automate_slot,
                txns_searched: scan.txns_searched,
                pages_fetched: scan.pages_fetched,
                fetch_duration_ms: duration_ms,
                automation_balance,
                is_partial_deploy,
                actual_squares_deployed,
                actual_mask,
                total_sol_spent,
            };
            
            if let Err(e) = state.clickhouse.insert_deployment_automation_state(insert).await {
                update_queue_failed(state, item.id, &format!("ClickHouse insert failed: {}", e)).await;
                return ProcessDetail {
                    id: item.id,
                    deploy_signature: item.deploy_signature,
                    success: false,
                    automation_found,
                    automation_active,
                    txns_searched: scan.txns_searched,
                    duration_ms,
                    used_cache,
                    cache_slot,
                    error: Some(format!("ClickHouse insert failed: {}", e)),
                };
            }
            
            // Update queue as completed
            let _ = sqlx::query(r#"
                UPDATE automation_state_queue
                SET status = 'completed', completed_at = NOW(),
                    fetch_duration_ms = $2, automation_found = $3, txns_searched = $4
                WHERE id = $1
            "#)
            .bind(item.id)
            .bind(duration_ms as i64)
            .bind(automation_found)
            .bind(scan.txns_searched as i32)
            .execute(&state.postgres)
            .await;
            
            ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: true,
                automation_found,
                automation_active,
                txns_searched: scan.txns_searched,
                duration_ms,
                used_cache,
                cache_slot,
                error: None,
            }
        }
        Err(e) => {
            update_queue_failed(state, item.id, &format!("Helius error: {}", e)).await;
            ProcessDetail {
                id: item.id,
                deploy_signature: item.deploy_signature,
                success: false,
                automation_found: false,
                automation_active: false,
                txns_searched: 0,
                duration_ms,
                used_cache: false,
                cache_slot: None,
                error: Some(format!("Helius error: {}", e)),
            }
        }
    }
}

// ============================================================================
// Backfill Integration - Queue-Based System
// ============================================================================

/// Response for queue operations
#[derive(Debug, Serialize)]
pub struct QueueRoundResponse {
    pub success: bool,
    pub round_id: u64,
    pub status: String,
    pub message: String,
}

/// POST /admin/automation/queue-round/{round_id}
/// Instantly queues a round for transaction parsing (no processing, just adds to queue)
pub async fn queue_round_for_parsing(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<QueueRoundResponse>, (StatusCode, Json<AuthError>)> {
    // Check if already queued
    let existing: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM transaction_parse_queue WHERE round_id = $1"
    )
    .bind(round_id as i64)
    .fetch_optional(&state.postgres)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    if let Some((status,)) = existing {
        return Ok(Json(QueueRoundResponse {
            success: false,
            round_id,
            status,
            message: "Round already in queue".to_string(),
        }));
    }
    
    // Add to queue
    sqlx::query(
        "INSERT INTO transaction_parse_queue (round_id, status) VALUES ($1, 'pending')"
    )
    .bind(round_id as i64)
    .execute(&state.postgres)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    tracing::info!("Queued round {} for transaction parsing", round_id);
    
    Ok(Json(QueueRoundResponse {
        success: true,
        round_id,
        status: "pending".to_string(),
        message: "Round queued for processing".to_string(),
    }))
}

/// GET /admin/automation/parse-queue
/// Get status of transaction parse queue
#[derive(Debug, Serialize)]
pub struct ParseQueueStats {
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
}

pub async fn get_parse_queue_stats(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ParseQueueStats>, (StatusCode, Json<AuthError>)> {
    let stats: (i64, i64, i64, i64) = sqlx::query_as(r#"
        SELECT 
            COUNT(*) FILTER (WHERE status = 'pending'),
            COUNT(*) FILTER (WHERE status = 'processing'),
            COUNT(*) FILTER (WHERE status = 'completed'),
            COUNT(*) FILTER (WHERE status = 'failed')
        FROM transaction_parse_queue
    "#)
    .fetch_one(&state.postgres)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    Ok(Json(ParseQueueStats {
        pending: stats.0,
        processing: stats.1,
        completed: stats.2,
        failed: stats.3,
    }))
}

/// GET /admin/automation/parse-queue/items
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ParseQueueItem {
    pub id: i32,
    pub round_id: i64,
    pub status: String,
    pub txns_found: Option<i32>,
    pub deploys_queued: Option<i32>,
    pub errors_count: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_error: Option<String>,
}

pub async fn get_parse_queue_items(
    State(state): State<Arc<AppState>>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ParseQueueItem>>, (StatusCode, Json<AuthError>)> {
    let status_filter = query.get("status").map(|s| s.as_str());
    let limit = query.get("limit").and_then(|s| s.parse().ok()).unwrap_or(50);
    
    let items: Vec<ParseQueueItem> = if let Some(status) = status_filter {
        sqlx::query_as(r#"
            SELECT id, round_id, status, txns_found, deploys_queued, errors_count,
                   created_at, started_at, completed_at, last_error
            FROM transaction_parse_queue
            WHERE status = $1
            ORDER BY created_at DESC
            LIMIT $2
        "#)
        .bind(status)
        .bind(limit)
        .fetch_all(&state.postgres)
        .await
    } else {
        sqlx::query_as(r#"
            SELECT id, round_id, status, txns_found, deploys_queued, errors_count,
                   created_at, started_at, completed_at, last_error
            FROM transaction_parse_queue
            ORDER BY created_at DESC
            LIMIT $1
        "#)
        .bind(limit)
        .fetch_all(&state.postgres)
        .await
    }.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(AuthError { error: e.to_string() })))?;
    
    Ok(Json(items))
}

// ============================================================================
// Background Transaction Parse Worker
// ============================================================================

/// Spawn background task that processes the transaction_parse_queue
pub fn spawn_transaction_parse_task(state: Arc<AppState>) {
    tokio::spawn(async move {
        tracing::info!("Transaction parse worker started");
        
        loop {
            // Check for pending items
            match process_next_parse_item(&state).await {
                Ok(true) => {
                    // Processed one, immediately check for more
                    continue;
                }
                Ok(false) => {
                    // No pending items, wait before checking again
                    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                }
                Err(e) => {
                    tracing::error!("Transaction parse worker error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
                }
            }
        }
    });
}

async fn process_next_parse_item(state: &AppState) -> Result<bool, String> {
    // Claim a pending item
    let item: Option<(i32, i64)> = sqlx::query_as(r#"
        UPDATE transaction_parse_queue
        SET status = 'processing', started_at = NOW()
        WHERE id = (
            SELECT id FROM transaction_parse_queue
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, round_id
    "#)
    .fetch_optional(&state.postgres)
    .await
    .map_err(|e| e.to_string())?;
    
    let (id, round_id) = match item {
        Some(i) => i,
        None => return Ok(false), // No pending items
    };
    
    tracing::info!("Processing transaction parse for round {}", round_id);
    
    // Get transactions from ClickHouse (query by round PDA for v2 tables)
    let round_pda = evore::ore_api::round_pda(round_id as u64).0.to_string();
    let raw_txns = state.clickhouse
        .get_transactions_by_account(&round_pda)
        .await
        .map_err(|e| e.to_string())?;
    
    let txns_found = raw_txns.len() as i32;
    let mut deploys_queued = 0i32;
    let mut errors_count = 0i32;
    
    // Parse each transaction
    for raw_tx in &raw_txns {
        let deploys = match extract_deploy_instructions_fast(&raw_tx.raw_json, round_id as u64) {
            Ok(d) => d,
            Err(_) => {
                errors_count += 1;
                continue;
            }
        };
        
        for deploy in deploys {
            let authority = match Pubkey::try_from(deploy.authority.as_str()) {
                Ok(pk) => pk,
                Err(_) => {
                    errors_count += 1;
                    continue;
                }
            };
            
            // Add to automation state queue
            let result = sqlx::query(r#"
                INSERT INTO automation_state_queue 
                (round_id, miner_pubkey, authority_pubkey, deploy_signature, 
                 deploy_ix_index, deploy_slot, priority)
                VALUES ($1, $2, $3, $4, $5, $6, 1000)
                ON CONFLICT (deploy_signature, deploy_ix_index) DO NOTHING
            "#)
            .bind(round_id)
            .bind(&deploy.miner)
            .bind(&deploy.authority)
            .bind(&raw_tx.signature)
            .bind(deploy.ix_index as i16)
            .bind(raw_tx.slot as i64)
            .execute(&state.postgres)
            .await;
            
            match result {
                Ok(r) if r.rows_affected() > 0 => deploys_queued += 1,
                Ok(_) => {} // Already exists
                Err(_) => errors_count += 1,
            }
        }
    }
    
    // Mark as completed
    sqlx::query(r#"
        UPDATE transaction_parse_queue
        SET status = 'completed', completed_at = NOW(),
            txns_found = $2, deploys_queued = $3, errors_count = $4
        WHERE id = $1
    "#)
    .bind(id)
    .bind(txns_found)
    .bind(deploys_queued)
    .bind(errors_count)
    .execute(&state.postgres)
    .await
    .map_err(|e| e.to_string())?;
    
    tracing::info!(
        "Completed parsing round {}: {} txns, {} deploys queued, {} errors",
        round_id, txns_found, deploys_queued, errors_count
    );
    
    Ok(true)
}

/// Lightweight deploy instruction extraction (much faster than full analysis)
struct FastDeployInfo {
    authority: String,
    miner: String,
    ix_index: u8,
}

fn extract_deploy_instructions_fast(raw_json: &str, _expected_round: u64) -> Result<Vec<FastDeployInfo>, String> {
    use serde_json::Value;
    use evore::ore_api::{OreInstruction, Deploy, PROGRAM_ID};
    
    let tx: Value = serde_json::from_str(raw_json)
        .map_err(|e| format!("JSON parse error: {}", e))?;
    
    let message = tx.get("transaction")
        .and_then(|t| t.get("message"))
        .ok_or("Missing message")?;
    
    let account_keys: Vec<Pubkey> = message.get("accountKeys")
        .and_then(Value::as_array)
        .map(|arr| arr.iter()
            .filter_map(|k| k.as_str())
            .filter_map(|s| Pubkey::try_from(s).ok())
            .collect())
        .unwrap_or_default();
    
    let instructions = message.get("instructions")
        .and_then(Value::as_array)
        .ok_or("Missing instructions")?;
    
    let mut deploys = Vec::new();
    
    for (ix_idx, ix) in instructions.iter().enumerate() {
        // Check program
        let prog_idx = ix.get("programIdIndex")
            .and_then(Value::as_u64)
            .unwrap_or(u64::MAX) as usize;
        
        let program_id = account_keys.get(prog_idx).copied().unwrap_or_default();
        if program_id != PROGRAM_ID {
            continue;
        }
        
        // Decode data
        let data = ix.get("data")
            .and_then(Value::as_str)
            .and_then(|s| bs58::decode(s).into_vec().ok())
            .unwrap_or_default();
        
        if data.is_empty() {
            continue;
        }
        
        // Check if it's a Deploy instruction
        let tag = data[0];
        if let Ok(OreInstruction::Deploy) = OreInstruction::try_from(tag) {
            // Parse Deploy struct
            const DEPLOY_SIZE: usize = std::mem::size_of::<Deploy>();
            if data.len() < 1 + DEPLOY_SIZE {
                continue;
            }
            
            // Get accounts: [signer, automation_info, miner_info, round_info, treasury, system_program]
            let accounts = ix.get("accounts")
                .and_then(Value::as_array);
            
            if accounts.is_none() {
                continue;
            }
            let accounts = accounts.unwrap();
            
            let get_key = |idx: usize| -> Option<Pubkey> {
                let acc_idx = accounts.get(idx)?.as_u64()? as usize;
                account_keys.get(acc_idx).copied()
            };
            
            let signer = get_key(0);
            let miner = get_key(2);
            
            if let (Some(authority), Some(miner_pk)) = (signer, miner) {
                deploys.push(FastDeployInfo {
                    authority: authority.to_string(),
                    miner: miner_pk.to_string(),
                    ix_index: ix_idx as u8,
                });
            }
        }
    }
    
    Ok(deploys)
}

// Keep old function for backwards compatibility but have it use the queue
pub async fn queue_from_round_transactions(
    State(state): State<Arc<AppState>>,
    Path(round_id): Path<u64>,
) -> Result<Json<AddToQueueResponse>, (StatusCode, Json<AuthError>)> {
    // Just add to the parse queue, don't process inline
    let result = queue_round_for_parsing(State(state), Path(round_id)).await?;
    let inner = result.0;
    
    Ok(Json(AddToQueueResponse {
        queued: if inner.success { 1 } else { 0 },
        already_exists: if inner.success { 0 } else { 1 },
        errors: if inner.success { vec![] } else { vec![inner.message] },
    }))
}
