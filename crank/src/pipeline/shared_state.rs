//! Shared state for the pipeline architecture
//!
//! Contains thread-safe state that is shared between pipeline systems.

use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

use crate::lut::LutRegistry;
use crate::miner_cache::MinerCache;

/// Current phase of the round
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RoundPhase {
    /// end_slot == u64::MAX, waiting for first deploy to start round
    WaitingForFirstDeploy,
    /// Round active, deployments open (our safety net: >= MIN_SLOTS_TO_DEPLOY slots remaining)
    DeploymentWindow { slots_remaining: u64 },
    /// Round active but too late to deploy (< MIN_SLOTS_TO_DEPLOY slots remaining, but current_slot < end_slot)
    LateDeploymentWindow { slots_remaining: u64 },
    /// Round ended, 35 slot intermission period (current_slot >= end_slot, < end_slot + 35)
    Intermission { slots_into_intermission: u64 },
    /// Intermission over, waiting for reset transaction (current_slot >= end_slot + 35)
    WaitingForReset,
}

/// Minimum slots remaining before we stop attempting deployments (safety net)
pub const MIN_SLOTS_TO_DEPLOY: u64 = 20;

/// Intermission duration in slots after round ends
pub const INTERMISSION_SLOTS: u64 = 35;

impl std::fmt::Display for RoundPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RoundPhase::WaitingForFirstDeploy => write!(f, "WaitingForFirstDeploy"),
            RoundPhase::DeploymentWindow { slots_remaining } => {
                write!(f, "DeploymentWindow({} slots)", slots_remaining)
            }
            RoundPhase::LateDeploymentWindow { slots_remaining } => {
                write!(f, "LateDeploymentWindow({} slots)", slots_remaining)
            }
            RoundPhase::Intermission { slots_into_intermission } => {
                write!(f, "Intermission({}/{})", slots_into_intermission, INTERMISSION_SLOTS)
            }
            RoundPhase::WaitingForReset => write!(f, "WaitingForReset"),
        }
    }
}

/// Board state from on-chain, updated by BoardStateMonitor
#[derive(Debug)]
pub struct BoardState {
    /// Current round ID
    pub round_id: u64,
    /// Round PDA address (derived from round_id)
    pub round_address: Pubkey,
    /// Slot when the round started
    pub start_slot: u64,
    /// Slot when the round ends (u64::MAX if waiting for first deploy)
    pub end_slot: u64,
    /// Current slot from the cluster
    pub current_slot: u64,
    /// Calculated phase based on slots
    pub phase: RoundPhase,
    /// When this state was last updated
    pub last_updated: Instant,
}

impl Default for BoardState {
    fn default() -> Self {
        Self {
            round_id: 0,
            round_address: Pubkey::default(),
            start_slot: 0,
            end_slot: u64::MAX,
            current_slot: 0,
            phase: RoundPhase::WaitingForFirstDeploy,
            last_updated: Instant::now(),
        }
    }
}

impl BoardState {
    /// Calculate current phase based on slots
    /// 
    /// Phase progression:
    /// 1. WaitingForFirstDeploy: end_slot == u64::MAX (after reset, before first deploy)
    /// 2. DeploymentWindow: Round active, slots_remaining >= MIN_SLOTS_TO_DEPLOY
    /// 3. LateDeploymentWindow: Round active, slots_remaining < MIN_SLOTS_TO_DEPLOY (safety net)
    /// 4. Intermission: current_slot >= end_slot, within 35 slots after end
    /// 5. WaitingForReset: current_slot >= end_slot + 35
    pub fn calculate_phase(&self) -> RoundPhase {
        // After reset, end_slot is u64::MAX until first deploy
        if self.end_slot == u64::MAX {
            return RoundPhase::WaitingForFirstDeploy;
        }

        // Round is active (end_slot is valid)
        if self.current_slot < self.end_slot {
            let slots_remaining = self.end_slot.saturating_sub(self.current_slot);
            
            if slots_remaining >= MIN_SLOTS_TO_DEPLOY {
                RoundPhase::DeploymentWindow { slots_remaining }
            } else {
                // Too close to end, our safety net kicks in
                RoundPhase::LateDeploymentWindow { slots_remaining }
            }
        } else {
            // Round ended (current_slot >= end_slot)
            let slots_since_end = self.current_slot.saturating_sub(self.end_slot);
            
            if slots_since_end < INTERMISSION_SLOTS {
                // In 35-slot intermission period
                RoundPhase::Intermission { slots_into_intermission: slots_since_end }
            } else {
                // Past intermission, waiting for reset
                RoundPhase::WaitingForReset
            }
        }
    }

    /// Check if we can deploy
    /// Returns true for:
    /// - WaitingForFirstDeploy: We can be the first deployer to start the round
    /// - DeploymentWindow: Round is active with enough slots remaining
    pub fn can_deploy(&self) -> bool {
        matches!(
            self.phase,
            RoundPhase::WaitingForFirstDeploy | RoundPhase::DeploymentWindow { .. }
        )
    }

    /// Update the phase based on current slot info
    pub fn update_phase(&mut self) {
        self.phase = self.calculate_phase();
        self.last_updated = Instant::now();
    }
}

/// Pipeline statistics for monitoring and logging
#[derive(Debug, Default)]
pub struct PipelineStats {
    // Round timing (ms since UNIX epoch, 0 = not set)
    pub round_pipeline_start_ms: AtomicU64,        // When first miner was sent to pipeline
    pub round_last_deploy_confirmed_ms: AtomicU64, // When last deploy txn was confirmed
    pub miners_sent_to_pipeline: AtomicU64,        // Total miners sent to pipeline this round

    // Processing counts
    pub miners_processed: AtomicU64,
    pub miners_skipped_wrong_fee: AtomicU64,
    pub miners_skipped_low_balance: AtomicU64,
    pub miners_skipped_no_slots: AtomicU64,
    pub miners_skipped_already_deployed: AtomicU64,

    // Miner outcome counts (individual miners, not transactions)
    pub miners_deployed: AtomicU64,           // Miners successfully deployed
    pub miners_deploy_failed: AtomicU64,      // Miners whose deploy txn failed
    pub miners_checkpointed: AtomicU64,       // Miners successfully checkpointed
    pub miners_checkpoint_failed: AtomicU64,  // Miners whose checkpoint txn failed

    // Deploy transaction stats
    pub deploys_sent: AtomicU64,
    pub deploys_confirmed: AtomicU64,
    pub deploys_failed: AtomicU64,
    pub deploy_total_time_ms: AtomicU64,
    pub deploy_count_for_avg: AtomicU64,

    // Checkpoint transaction stats
    pub checkpoints_sent: AtomicU64,
    pub checkpoints_confirmed: AtomicU64,
    pub checkpoints_failed: AtomicU64,
    pub checkpoint_total_time_ms: AtomicU64,
    pub checkpoint_count_for_avg: AtomicU64,

    // Fee update transaction stats
    pub fee_updates_sent: AtomicU64,
    pub fee_updates_confirmed: AtomicU64,
    pub fee_updates_failed: AtomicU64,
    pub fee_update_total_time_ms: AtomicU64,
    pub fee_update_count_for_avg: AtomicU64,

    // System timing
    pub lut_check_total_time_ms: AtomicU64,
    pub lut_check_count: AtomicU64,
    pub deployment_check_total_time_ms: AtomicU64,
    pub deployment_check_count: AtomicU64,
    pub confirmation_batch_total_time_ms: AtomicU64,
    pub confirmation_batch_count: AtomicU64,
}

impl PipelineStats {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get current timestamp in ms since UNIX epoch
    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    /// Record when pipeline started processing miners for this round
    pub fn record_pipeline_start(&self) {
        self.round_pipeline_start_ms.store(Self::now_ms(), Ordering::Relaxed);
    }

    /// Record when a deploy was confirmed (updates last confirmed timestamp)
    pub fn record_deploy_confirmed(&self) {
        self.round_last_deploy_confirmed_ms.store(Self::now_ms(), Ordering::Relaxed);
    }

    /// Get total round deployment time in milliseconds (from pipeline start to last deploy confirmed)
    pub fn round_total_deploy_time_ms(&self) -> Option<u64> {
        let start = self.get(&self.round_pipeline_start_ms);
        let end = self.get(&self.round_last_deploy_confirmed_ms);
        if start == 0 || end == 0 || end < start {
            None
        } else {
            Some(end - start)
        }
    }

    /// Increment a counter
    pub fn increment(&self, counter: &AtomicU64) {
        counter.fetch_add(1, Ordering::Relaxed);
    }

    /// Add to a counter
    pub fn add(&self, counter: &AtomicU64, value: u64) {
        counter.fetch_add(value, Ordering::Relaxed);
    }

    /// Get a counter value
    pub fn get(&self, counter: &AtomicU64) -> u64 {
        counter.load(Ordering::Relaxed)
    }

    /// Calculate average time for deploys
    pub fn deploy_avg_time_ms(&self) -> f64 {
        let count = self.get(&self.deploy_count_for_avg);
        if count == 0 {
            return 0.0;
        }
        self.get(&self.deploy_total_time_ms) as f64 / count as f64
    }

    /// Calculate average time for checkpoints
    pub fn checkpoint_avg_time_ms(&self) -> f64 {
        let count = self.get(&self.checkpoint_count_for_avg);
        if count == 0 {
            return 0.0;
        }
        self.get(&self.checkpoint_total_time_ms) as f64 / count as f64
    }

    /// Calculate average time for fee updates
    pub fn fee_update_avg_time_ms(&self) -> f64 {
        let count = self.get(&self.fee_update_count_for_avg);
        if count == 0 {
            return 0.0;
        }
        self.get(&self.fee_update_total_time_ms) as f64 / count as f64
    }

    /// Reset all counters (call at start of new round)
    pub fn reset(&self) {
        // Round timing
        self.round_pipeline_start_ms.store(0, Ordering::Relaxed);
        self.round_last_deploy_confirmed_ms.store(0, Ordering::Relaxed);
        self.miners_sent_to_pipeline.store(0, Ordering::Relaxed);
        // Processing counts
        self.miners_processed.store(0, Ordering::Relaxed);
        self.miners_skipped_wrong_fee.store(0, Ordering::Relaxed);
        self.miners_skipped_low_balance.store(0, Ordering::Relaxed);
        self.miners_skipped_no_slots.store(0, Ordering::Relaxed);
        self.miners_skipped_already_deployed.store(0, Ordering::Relaxed);
        self.miners_deployed.store(0, Ordering::Relaxed);
        self.miners_deploy_failed.store(0, Ordering::Relaxed);
        self.miners_checkpointed.store(0, Ordering::Relaxed);
        self.miners_checkpoint_failed.store(0, Ordering::Relaxed);
        self.deploys_sent.store(0, Ordering::Relaxed);
        self.deploys_confirmed.store(0, Ordering::Relaxed);
        self.deploys_failed.store(0, Ordering::Relaxed);
        self.deploy_total_time_ms.store(0, Ordering::Relaxed);
        self.deploy_count_for_avg.store(0, Ordering::Relaxed);
        self.checkpoints_sent.store(0, Ordering::Relaxed);
        self.checkpoints_confirmed.store(0, Ordering::Relaxed);
        self.checkpoints_failed.store(0, Ordering::Relaxed);
        self.checkpoint_total_time_ms.store(0, Ordering::Relaxed);
        self.checkpoint_count_for_avg.store(0, Ordering::Relaxed);
        self.fee_updates_sent.store(0, Ordering::Relaxed);
        self.fee_updates_confirmed.store(0, Ordering::Relaxed);
        self.fee_updates_failed.store(0, Ordering::Relaxed);
        self.fee_update_total_time_ms.store(0, Ordering::Relaxed);
        self.fee_update_count_for_avg.store(0, Ordering::Relaxed);
        self.lut_check_total_time_ms.store(0, Ordering::Relaxed);
        self.lut_check_count.store(0, Ordering::Relaxed);
        self.deployment_check_total_time_ms.store(0, Ordering::Relaxed);
        self.deployment_check_count.store(0, Ordering::Relaxed);
        self.confirmation_batch_total_time_ms.store(0, Ordering::Relaxed);
        self.confirmation_batch_count.store(0, Ordering::Relaxed);
    }

    /// Log a summary of stats
    pub fn log_summary(&self, round_id: u64, phase: &RoundPhase) {
        let miners_sent = self.get(&self.miners_sent_to_pipeline);
        let miners_deployed = self.get(&self.miners_deployed);
        let miners_failed = self.get(&self.miners_deploy_failed);
        let miners_checkpointed = self.get(&self.miners_checkpointed);
        
        let skipped_wrong_fee = self.get(&self.miners_skipped_wrong_fee);
        let skipped_low_balance = self.get(&self.miners_skipped_low_balance);
        let skipped_no_slots = self.get(&self.miners_skipped_no_slots);
        let skipped_already_deployed = self.get(&self.miners_skipped_already_deployed);
        let total_skipped = skipped_wrong_fee + skipped_low_balance + skipped_no_slots + skipped_already_deployed;

        // Calculate total deployment time
        let total_time_str = match self.round_total_deploy_time_ms() {
            Some(ms) => {
                let secs = ms as f64 / 1000.0;
                format!("{:.2}s ({} ms)", secs, ms)
            }
            None => "N/A".to_string(),
        };

        tracing::info!(
            "[Stats] Round {} | Phase: {}",
            round_id,
            phase
        );
        tracing::info!(
            "        Total deployment time: {} (from pipeline start to last deploy confirmed)",
            total_time_str
        );
        tracing::info!(
            "        Miners: {} sent to pipeline, {} deployed, {} failed, {} checkpointed",
            miners_sent,
            miners_deployed,
            miners_failed,
            miners_checkpointed
        );
        tracing::info!(
            "        Skipped: {} total (wrong_fee: {}, low_balance: {}, no_slots: {}, already_deployed: {})",
            total_skipped,
            skipped_wrong_fee,
            skipped_low_balance,
            skipped_no_slots,
            skipped_already_deployed
        );
        tracing::info!(
            "        Txns Deploy:     {} sent, {} confirmed, {} failed (avg {:.1}ms)",
            self.get(&self.deploys_sent),
            self.get(&self.deploys_confirmed),
            self.get(&self.deploys_failed),
            self.deploy_avg_time_ms()
        );
        tracing::info!(
            "        Txns Checkpoint: {} sent, {} confirmed, {} failed (avg {:.1}ms)",
            self.get(&self.checkpoints_sent),
            self.get(&self.checkpoints_confirmed),
            self.get(&self.checkpoints_failed),
            self.checkpoint_avg_time_ms()
        );
        tracing::info!(
            "        Txns FeeUpdate:  {} sent, {} confirmed, {} failed (avg {:.1}ms)",
            self.get(&self.fee_updates_sent),
            self.get(&self.fee_updates_confirmed),
            self.get(&self.fee_updates_failed),
            self.fee_update_avg_time_ms()
        );
    }
}

/// Shared state accessible by all pipeline systems
pub struct SharedState {
    /// Cache of miner account data
    pub miner_cache: RwLock<MinerCache>,
    /// LUT registry for address lookup tables
    pub lut_cache: RwLock<LutRegistry>,
    /// Current board/round state
    pub board_state: RwLock<BoardState>,
    /// Pipeline statistics
    pub stats: PipelineStats,
}

impl SharedState {
    /// Create new shared state
    pub fn new(rpc_url: &str, authority: Pubkey) -> Self {
        Self {
            miner_cache: RwLock::new(MinerCache::new()),
            lut_cache: RwLock::new(LutRegistry::new(rpc_url, authority)),
            board_state: RwLock::new(BoardState::default()),
            stats: PipelineStats::new(),
        }
    }
}

