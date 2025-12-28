use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use evore::ore_api::{AutomationStrategy, Board, Miner, Round, Treasury};
use serde::{Deserialize, Serialize};
use steel::{Numeric, Pubkey};
use tokio::sync::{broadcast, RwLock};

use crate::app_rpc::AppRpc;
use crate::clickhouse::ClickHouseClient;
use crate::evore_cache::EvoreCache;
use crate::helius_api::HeliusApi;

// ============================================================================
// Utility Functions
// ============================================================================

/// Infer the actual refined ore balance based on treasury rewards factor.
/// The miner's `refined_ore` field is stale unless they've recently interacted.
/// This calculates the up-to-date value using the treasury's current rewards factor.
/// 
/// IMPORTANT: Call this immediately after fetching miner data, before caching.
pub fn infer_refined_ore(miner: &Miner, treasury: &Treasury) -> u64 {
    let delta = treasury.miner_rewards_factor - miner.rewards_factor;
    if delta < Numeric::ZERO {
        // Defensive: shouldn't happen, but keep behavior sane.
        return miner.refined_ore;
    }
    let accrued = (delta * Numeric::from_u64(miner.rewards_ore)).to_u64();
    miner.refined_ore.saturating_add(accrued)
}

/// Apply refined_ore calculation to a miner and return a new miner with updated value
pub fn apply_refined_ore_fix(miner: &Miner, treasury: &Treasury) -> Miner {
    let mut fixed = *miner;
    fixed.refined_ore = infer_refined_ore(miner, treasury);
    fixed
}

// ============================================================================
// Application State
// ============================================================================

/// Central application state shared across all handlers and background tasks.
pub struct AppState {
    // Server start time for uptime tracking
    pub start_time: Instant,
    
    // Admin password hash (Argon2, hashed at startup from ADMIN_PASSWORD env)
    pub admin_password_hash: String,
    
    // Database connections
    pub clickhouse: Arc<ClickHouseClient>,
    pub postgres: sqlx::Pool<sqlx::Postgres>,
    
    // RPC client
    pub rpc: Arc<AppRpc>,
    
    // Helius API for bulk fetching (miners, token holders)
    pub helius: Arc<RwLock<HeliusApi>>,
    
    // Live caches (updated by polling task)
    pub board_cache: Arc<RwLock<Option<Board>>>,
    pub treasury_cache: Arc<RwLock<Option<Treasury>>>,
    pub round_cache: Arc<RwLock<Option<LiveRound>>>,
    /// Miners cache sorted by authority (base58 string) for consistent pagination
    pub miners_cache: Arc<RwLock<BTreeMap<String, Miner>>>,
    pub miners_last_slot: Arc<RwLock<u64>>,
    
    // Slot cache (updated by WebSocket)
    pub slot_cache: Arc<RwLock<u64>>,
    
    // ORE token holders cache (updated periodically)
    pub ore_holders_cache: Arc<RwLock<HashMap<Pubkey, u64>>>,
    pub ore_holders_last_slot: Arc<RwLock<u64>>,
    
    // EVORE program accounts cache (Managers, Deployers, Auth balances)
    pub evore_cache: Arc<RwLock<EvoreCache>>,
    
    // SSE broadcast channels
    pub round_broadcast: broadcast::Sender<LiveBroadcastData>,
    pub deployment_broadcast: broadcast::Sender<LiveBroadcastData>,
    
    // Per-round deployment tracking for Phase 2 finalization
    // Maps: miner_pubkey -> { square_id -> (amount, slot) }
    // Tracks when each square was deployed for accurate slot data
    // Cleared on new round, used during round finalization
    pub pending_deployments: Arc<RwLock<HashMap<String, HashMap<u8, (u64, u64)>>>>,
    pub pending_round_id: Arc<RwLock<u64>>,
    
    // Deployments cache: Updated by miner cache comparisons
    // Maps: miner_pubkey -> { square_id -> (amount, slot) }
    // More reliable than WebSocket: detects deployments on each miner cache update
    // Cleared when round transitions, used for finalization + live display
    pub deployments_cache: Arc<RwLock<HashMap<String, HashMap<u8, (u64, u64)>>>>,
    pub deployments_cache_round_id: Arc<RwLock<u64>>,
    
    // Round finalization: Snapshot captured when round ends, used after reset
    pub round_snapshot: Arc<RwLock<Option<RoundSnapshot>>>,
}

/// Snapshot of round state captured when round ends (slots_left <= 0)
/// Used for finalization after the round resets
#[derive(Debug, Clone)]
pub struct RoundSnapshot {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    
    /// Per-miner, per-square deployments with slot timing
    /// miner_pubkey -> { square_id -> (amount, slot) }
    pub deployments: HashMap<String, HashMap<u8, (u64, u64)>>,
    
    /// Miners who deployed THIS round (for deployment tracking)
    pub miners: HashMap<String, Miner>,
    
    /// ALL miners from GPA snapshot (for full historical tracking ~1/min)
    /// Stores complete state of every miner account at round end
    pub all_miners: HashMap<String, Miner>,
    
    /// Treasury state at round end
    pub treasury: Treasury,
    
    /// Round state (without slot_hash - that comes after reset)
    pub round: Round,
    
    /// Timestamp when snapshot was captured
    pub captured_at: u64,
}

impl AppState {
    pub fn new(
        admin_password_hash: String,
        clickhouse: Arc<ClickHouseClient>,
        postgres: sqlx::Pool<sqlx::Postgres>,
        rpc: Arc<AppRpc>,
        helius: Arc<RwLock<HeliusApi>>,
    ) -> Self {
        let (round_tx, _) = broadcast::channel(100);
        let (deployment_tx, _) = broadcast::channel(1000);
        
        Self {
            start_time: Instant::now(),
            admin_password_hash,
            clickhouse,
            postgres,
            rpc,
            helius,
            board_cache: Arc::new(RwLock::new(None)),
            treasury_cache: Arc::new(RwLock::new(None)),
            round_cache: Arc::new(RwLock::new(None)),
            miners_cache: Arc::new(RwLock::new(BTreeMap::new())),
            miners_last_slot: Arc::new(RwLock::new(0)),
            slot_cache: Arc::new(RwLock::new(0)),
            ore_holders_cache: Arc::new(RwLock::new(HashMap::new())),
            ore_holders_last_slot: Arc::new(RwLock::new(0)),
            evore_cache: Arc::new(RwLock::new(EvoreCache::new())),
            round_broadcast: round_tx,
            deployment_broadcast: deployment_tx,
            pending_deployments: Arc::new(RwLock::new(HashMap::new())),
            pending_round_id: Arc::new(RwLock::new(0)),
            round_snapshot: Arc::new(RwLock::new(None)),
            deployments_cache: Arc::new(RwLock::new(HashMap::new())),
            deployments_cache_round_id: Arc::new(RwLock::new(0)),
        }
    }
    
    /// Get server uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
    
    /// Subscribe to round updates for SSE
    pub fn subscribe_rounds(&self) -> broadcast::Receiver<LiveBroadcastData> {
        self.round_broadcast.subscribe()
    }
    
    /// Subscribe to deployment updates for SSE
    pub fn subscribe_deployments(&self) -> broadcast::Receiver<LiveBroadcastData> {
        self.deployment_broadcast.subscribe()
    }
}

// ============================================================================
// Live Data Types
// ============================================================================

/// Live round data with unique miners tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveRound {
    pub round_id: u64,
    pub start_slot: u64,
    pub end_slot: u64,
    pub slots_remaining: i64,
    pub deployed: [u64; 25],
    pub count: [u64; 25],
    pub total_deployed: u64,
    pub unique_miners: u32,
}

impl LiveRound {
    pub fn from_board_and_round(board: &Board, round: &Round) -> Self {
        Self {
            round_id: round.id,
            start_slot: board.start_slot,
            end_slot: board.end_slot,
            slots_remaining: 0, // Will be calculated with current slot
            deployed: round.deployed,
            count: round.count,
            total_deployed: round.total_deployed,
            unique_miners: round.total_miners as u32,
        }
    }
    
    pub fn update_slots_remaining(&mut self, current_slot: u64) {
        self.slots_remaining = self.end_slot.saturating_sub(current_slot) as i64;
    }
}

/// Data broadcast over SSE channels
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum LiveBroadcastData {
    /// Round state update (throttled to ~500ms)
    Round(LiveRound),
    
    /// Single deployment event
    Deployment(LiveDeployment),
    
    /// Winning square announcement at round end
    WinningSquare {
        round_id: u64,
        winning_square: u8,
        motherlode_hit: bool,
    },
}

/// Live deployment from WebSocket
/// Batched: all squares for one miner at one slot in a single event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveDeployment {
    pub round_id: u64,
    pub miner_pubkey: String,
    /// Array of 25 amounts, index = square_id, value = amount deployed on that square
    pub amounts: [u64; 25],
    /// The slot when this deployment occurred
    pub slot: u64,
}

// ============================================================================
// Existing Types (kept for compatibility)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppMiner {
    /// The authority of this miner account.
    pub authority: String,

    /// The miner's prospects in the current round.
    pub deployed: [u64; 25],
    /// Total deployed (Sum of miners prospects)
    pub total_deployed: u64,

    /// The cumulative amount of SOL deployed on each square prior to this miner's move.
    pub cumulative: [u64; 25],

    /// SOL witheld in reserve to pay for checkpointing.
    pub checkpoint_fee: u64,

    /// The last round that this miner checkpointed.
    pub checkpoint_id: u64,

    /// The last time this miner claimed ORE rewards.
    pub last_claim_ore_at: i64,

    /// The last time this miner claimed SOL rewards.
    pub last_claim_sol_at: i64,

    /// The amount of SOL this miner can claim.
    pub rewards_sol: u64,

    /// The amount of ORE this miner can claim.
    pub rewards_ore: u64,

    /// The amount of ORE this miner has earned from claim fees.
    pub refined_ore: u64,

    /// The ID of the round this miner last played in.
    pub round_id: u64,

    /// The total amount of SOL this miner has mined across all blocks.
    pub lifetime_rewards_sol: u64,

    /// The total amount of ORE this miner has mined across all blocks.
    pub lifetime_rewards_ore: u64,
}

impl From<Miner> for AppMiner {
    fn from(miner: Miner) -> Self {
        let mut total = 0;
        for m in miner.deployed.iter() {
            total = total + m;
        }
        AppMiner {
            authority: miner.authority.to_string(),
            deployed: miner.deployed,
            total_deployed: total,
            cumulative: miner.cumulative,
            checkpoint_fee: miner.checkpoint_fee,
            checkpoint_id: miner.checkpoint_id,
            last_claim_ore_at: miner.last_claim_ore_at,
            last_claim_sol_at: miner.last_claim_sol_at,
            rewards_sol: miner.rewards_sol,
            rewards_ore: miner.rewards_ore,
            refined_ore: miner.refined_ore,
            round_id: miner.round_id,
            lifetime_rewards_sol: miner.lifetime_rewards_sol,
            lifetime_rewards_ore: miner.lifetime_rewards_ore,
        }
    }
}

#[derive(Debug)]
pub struct ReconstructedRound {
    pub round: AppRound,
    pub deployments: Vec<AppDeployment>,
}

#[derive(Debug, Clone)]
pub struct AppRound {
    pub round_id: i64,
    pub winning_square: i64,
    pub motherlode: i64,
    pub top_miner: String,
    pub total_deployed: i64,
    pub total_vaulted: i64,
    pub total_winnings: i64,
    pub created_at: i64,
}

impl From<Round> for AppRound {
    fn from(round: Round) -> Self {
        if let Some(r) = round.rng() {
            AppRound {
                round_id: round.id as i64,
                winning_square: round.winning_square(r) as i64,
                motherlode: round.motherlode as i64,
                top_miner: round.top_miner.to_string(),
                total_deployed: round.total_deployed as i64,
                total_vaulted: round.total_vaulted as i64,
                total_winnings: round.total_winnings as i64,
                created_at: Utc::now().timestamp(),
            }
        } else {
            AppRound {
                round_id: round.id as i64,
                winning_square: 100,
                motherlode: 0,
                top_miner: Pubkey::default().to_string(),
                total_deployed: 0,
                total_vaulted: 0,
                total_winnings: 0,
                created_at: Utc::now().timestamp(),
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppDeployment {
    pub pubkey: String,
    pub round_id: i64,
    pub deployments: [AppDeployedSquare; 25],
    pub total_deployed: i64,
    pub total_sol_earned: i64,
    pub total_ore_earned: i64,
    pub winner: bool
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct AppDeployedSquare {
    pub amount: i64,
    pub square_id: i64,
    pub slot: i64
}

#[derive(Debug, Clone)]
pub struct ReconstructedAutomation {
    pub amount: u64,
    pub authority: Pubkey,
    pub executor: Pubkey,
    pub fee: u64,
    pub strategy: AutomationStrategy,
    pub mask: u64,
}

#[derive(Debug, Clone)]
pub struct AutomationCache {
    pub authority: Pubkey,
    pub active: bool,
    pub mask: u64,
    pub strategy: u64,
    pub amount: u64,
    pub fee: u64,
    pub executor: Pubkey,
    pub last_updated_slot: u64,
}

impl AutomationCache {
    pub fn new(authority: Pubkey) -> Self {
        Self {
            authority,
            active: false,
            mask: 0,
            strategy: 0,
            amount: 0,
            fee: 0,
            executor: Pubkey::default(),
            last_updated_slot: 0,
        }
    }
}

impl AppDeployment {
    pub fn new(pubkey: String, round_id: i64) -> Self {
        Self {
            pubkey,
            round_id,
            deployments: [AppDeployedSquare::default(); 25],
            total_deployed: 0,
            total_sol_earned: 0,
            total_ore_earned: 0,
            winner: false,
        }
    }
}


impl Default for AppDeployedSquare {
    fn default() -> Self {
        Self {
            amount: 0,
            square_id: 0,
            slot: 0,
        }
    }
}

