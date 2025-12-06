//! Bot State Machine - Tracks bot lifecycle through round phases
//!
//! States:
//! - Idle: Round not active (end_slot == MAX)
//! - Waiting: Round active, waiting for deploy window
//! - Deploying: In deploy window, sending transactions
//! - Deployed: Successfully deployed this round
//! - Checkpointing: Processing checkpoint for previous round

use solana_sdk::signature::Signature;

/// Bot phase in the round lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotPhase {
    /// Bot is paused - no activity
    Paused,
    /// Loading data after unpause
    Loading,
    /// No active round (end_slot == MAX)
    Idle,
    /// Round active, waiting for deploy window (slots_left > threshold)
    Waiting,
    /// In deploy window, actively sending transactions
    Deploying,
    /// Successfully deployed this round
    Deployed,
    /// Checkpointing previous round results
    Checkpointing,
    /// Claiming rewards after checkpoint
    Claiming,
}

impl Default for BotPhase {
    fn default() -> Self {
        BotPhase::Idle
    }
}

impl BotPhase {
    /// Get display string for TUI
    pub fn as_str(&self) -> &'static str {
        match self {
            BotPhase::Paused => "Paused",
            BotPhase::Loading => "Loading",
            BotPhase::Idle => "Idle",
            BotPhase::Waiting => "Waiting",
            BotPhase::Deploying => "Deploying",
            BotPhase::Deployed => "Deployed",
            BotPhase::Checkpointing => "Checkpointing",
            BotPhase::Claiming => "Claiming",
        }
    }
}

/// Runtime state for a bot during operation
#[derive(Debug, Clone)]
pub struct BotState {
    /// Current phase in round lifecycle
    pub phase: BotPhase,
    
    /// Whether the bot is paused
    pub is_paused: bool,
    
    /// Flag to trigger data reload on unpause
    pub needs_reload: bool,
    
    /// Current round ID being tracked
    pub current_round_id: u64,
    
    /// Last round where bot successfully deployed
    pub last_deployed_round: Option<u64>,
    
    /// Last round where bot checkpointed
    pub last_checkpointed_round: Option<u64>,
    
    /// Pending transaction signatures awaiting confirmation
    pub pending_signatures: Vec<Signature>,
    
    /// Amount deployed in current round (lamports)
    pub deployed_amount: u64,
    
    /// Session statistics
    pub rounds_participated: u64,
    pub rounds_won: u64,
    pub rounds_skipped: u64,
    pub rounds_missed: u64,
    
    /// P&L tracking
    pub starting_claimable_sol: u64,
    pub current_claimable_sol: u64,
    pub starting_ore: u64,
    pub current_ore: u64,
    
    /// Pre-checkpoint values for delta calculation
    pub pre_checkpoint_sol: u64,
    pub pre_checkpoint_ore: u64,
}

impl Default for BotState {
    fn default() -> Self {
        Self {
            phase: BotPhase::Idle,
            is_paused: false,
            needs_reload: false,
            current_round_id: 0,
            last_deployed_round: None,
            last_checkpointed_round: None,
            pending_signatures: Vec::new(),
            deployed_amount: 0,
            rounds_participated: 0,
            rounds_won: 0,
            rounds_skipped: 0,
            rounds_missed: 0,
            starting_claimable_sol: 0,
            current_claimable_sol: 0,
            starting_ore: 0,
            current_ore: 0,
            pre_checkpoint_sol: 0,
            pre_checkpoint_ore: 0,
        }
    }
}

impl BotState {
    /// Create new bot state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if already deployed to current round
    pub fn already_deployed(&self, round_id: u64) -> bool {
        self.last_deployed_round == Some(round_id)
    }

    /// Check if needs checkpoint for previous round
    pub fn needs_checkpoint(&self) -> bool {
        match (self.last_deployed_round, self.last_checkpointed_round) {
            (Some(deployed), Some(checkpointed)) => deployed > checkpointed,
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Record a successful deployment
    pub fn record_deployment(&mut self, round_id: u64, amount: u64) {
        self.last_deployed_round = Some(round_id);
        self.deployed_amount = amount;
        self.rounds_participated += 1;
        self.pending_signatures.clear();
    }

    /// Store pre-checkpoint values for delta calculation
    pub fn store_pre_checkpoint(&mut self, rewards_sol: u64, rewards_ore: u64) {
        self.pre_checkpoint_sol = rewards_sol;
        self.pre_checkpoint_ore = rewards_ore;
    }

    /// Process checkpoint result and update stats
    pub fn process_checkpoint(&mut self, round_id: u64, rewards_sol: u64, rewards_ore: u64) {
        self.last_checkpointed_round = Some(round_id);
        
        // Calculate deltas
        let sol_delta = rewards_sol.saturating_sub(self.pre_checkpoint_sol);
        let ore_delta = rewards_ore.saturating_sub(self.pre_checkpoint_ore);
        
        // Count as win if gained anything
        if sol_delta > 0 || ore_delta > 0 {
            self.rounds_won += 1;
        }
        
        // Update current values for P&L
        self.current_claimable_sol = rewards_sol;
        self.current_ore = rewards_ore;
    }

    /// Initialize starting values for P&L (called on first stats update)
    pub fn init_starting_values(&mut self, claimable_sol: u64, ore: u64) {
        if self.starting_claimable_sol == 0 && self.starting_ore == 0 {
            self.starting_claimable_sol = claimable_sol;
            self.current_claimable_sol = claimable_sol;
            self.starting_ore = ore;
            self.current_ore = ore;
        }
    }

    /// Calculate SOL P&L (can be negative)
    pub fn sol_pnl(&self) -> i64 {
        self.current_claimable_sol as i64 - self.starting_claimable_sol as i64
    }

    /// Calculate ORE P&L (can be negative)
    pub fn ore_pnl(&self) -> i64 {
        self.current_ore as i64 - self.starting_ore as i64
    }

    /// Transition to new phase
    pub fn set_phase(&mut self, phase: BotPhase) {
        self.phase = phase;
    }

    /// Reset for new round
    pub fn reset_for_round(&mut self, round_id: u64) {
        self.current_round_id = round_id;
        self.deployed_amount = 0;
        self.pending_signatures.clear();
    }
    
    /// Pause the bot
    pub fn pause(&mut self) {
        self.is_paused = true;
        self.phase = BotPhase::Paused;
    }
    
    /// Unpause the bot and trigger reload
    pub fn unpause(&mut self) {
        self.is_paused = false;
        self.needs_reload = true;
        self.phase = BotPhase::Loading;
    }
    
    /// Toggle pause state
    pub fn toggle_pause(&mut self) {
        if self.is_paused {
            self.unpause();
        } else {
            self.pause();
        }
    }
    
    /// Check if reload is needed (and clear the flag)
    pub fn take_needs_reload(&mut self) -> bool {
        let needs = self.needs_reload;
        self.needs_reload = false;
        needs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bot_state_default() {
        let state = BotState::new();
        assert_eq!(state.phase, BotPhase::Idle);
        assert!(!state.already_deployed(1));
        assert!(!state.needs_checkpoint());
    }

    #[test]
    fn test_deployment_tracking() {
        let mut state = BotState::new();
        
        state.record_deployment(5, 1_000_000);
        
        assert!(state.already_deployed(5));
        assert!(!state.already_deployed(6));
        assert!(state.needs_checkpoint());
        assert_eq!(state.rounds_participated, 1);
    }

    #[test]
    fn test_checkpoint_processing() {
        let mut state = BotState::new();
        state.init_starting_values(0, 0);
        
        state.record_deployment(5, 1_000_000);
        state.store_pre_checkpoint(0, 0);
        state.process_checkpoint(5, 500_000, 1_000);
        
        assert!(!state.needs_checkpoint());
        assert_eq!(state.rounds_won, 1);
        assert_eq!(state.sol_pnl(), 500_000);
        assert_eq!(state.ore_pnl(), 1_000);
    }

    #[test]
    fn test_negative_pnl() {
        let mut state = BotState::new();
        state.init_starting_values(1_000_000, 100);
        
        // After losing round, less rewards
        state.current_claimable_sol = 500_000;
        state.current_ore = 50;
        
        assert_eq!(state.sol_pnl(), -500_000);
        assert_eq!(state.ore_pnl(), -50);
    }
}
