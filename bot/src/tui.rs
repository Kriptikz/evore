//! Dashboard TUI for Evore Bot
//! 
//! Architecture:
//! - UI Layer: TUI rendering, keyboard input
//! - Data Layer: Bot tasks run independently, send updates via channels
//! - TuiUpdate: Message enum bridging bot tasks → TUI

use std::{
    io::{self, Stdout},
    time::{Duration, Instant},
};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table},
    Frame, Terminal,
};
use solana_sdk::{hash::Hash, pubkey::Pubkey, signature::Signature};
use cli_clipboard;

use evore::ore_api::{Board, Miner, Round, INTERMISSION_SLOTS};

// =============================================================================
// TUI Update Messages (Bot → TUI)
// =============================================================================

/// Messages sent from bot tasks to the TUI for state updates
#[derive(Debug, Clone)]
pub enum TuiUpdate {
    /// Update slot and blockhash from tracker
    SlotUpdate { slot: u64, blockhash: Hash },
    
    /// Update board data
    BoardUpdate(Board),
    
    /// Update round data
    RoundUpdate(Round),
    
    /// Bot status changed
    BotStatusUpdate { bot_index: usize, status: BotStatus },
    
    /// Bot miner data updated
    BotMinerUpdate { bot_index: usize, miner: Miner },
    
    /// Bot deployed this round (amount deployed)
    BotDeployedUpdate { bot_index: usize, amount: u64, round_id: u64 },
    
    /// Bot session stats updated (with P&L tracking)
    BotStatsUpdate { 
        bot_index: usize, 
        rounds_participated: u64,
        rounds_won: u64,
        rounds_skipped: u64,
        rounds_missed: u64,
        current_claimable_sol: u64,
        current_ore: u64,
    },
    
    /// Bot signer (fee payer) balance updated
    BotSignerBalanceUpdate { bot_index: usize, balance: u64 },
    
    /// Bot successfully claimed SOL (for P&L tracking)
    BotClaimedSol { bot_index: usize, amount: u64 },
    
    /// Bot successfully claimed ORE (for P&L tracking)
    BotClaimedOre { bot_index: usize, amount: u64 },
    
    /// Transaction event (for tx log) - legacy format
    TxEvent {
        bot_name: String,
        action: TxAction,
        signature: Signature,
        error: Option<String>,
    },
    
    /// Transaction event with type info (new format)
    TxEventTyped {
        bot_name: String,
        tx_type: TxType,
        status: TxStatus,
        signature: Signature,
        error: Option<String>,
        /// Slot when tx was sent/confirmed
        slot: Option<u64>,
        /// Round ID the tx relates to
        round_id: Option<u64>,
        /// Amount involved (deployed lamports, claimed lamports, etc.)
        amount: Option<u64>,
        /// Attempt number (for deploy retries)
        attempt: Option<u64>,
    },
    
    /// Error message
    Error(String),
}

/// Round lifecycle phase
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RoundPhase {
    /// end_slot == u64::MAX - waiting for first deployer to start round
    WaitingStart,
    /// current_slot < end_slot - round is active, deployments allowed
    Active,
    /// current_slot >= end_slot && slots_past < 35 - intermission period
    Intermission,
    /// current_slot >= end_slot + 35 - ready for reset
    WaitingReset,
}

impl RoundPhase {
    pub fn calculate(current_slot: u64, board: &Board) -> Self {
        if board.end_slot == u64::MAX {
            return RoundPhase::WaitingStart;
        }
        
        if current_slot < board.end_slot {
            return RoundPhase::Active;
        }
        
        let slots_past = current_slot.saturating_sub(board.end_slot);
        if slots_past < INTERMISSION_SLOTS {
            RoundPhase::Intermission
        } else {
            RoundPhase::WaitingReset
        }
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            RoundPhase::WaitingStart => "Waiting Start",
            RoundPhase::Active => "Active",
            RoundPhase::Intermission => "Intermission",
            RoundPhase::WaitingReset => "Waiting Reset",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            RoundPhase::WaitingStart => Color::Yellow,
            RoundPhase::Active => Color::Green,
            RoundPhase::Intermission => Color::Cyan,
            RoundPhase::WaitingReset => Color::Magenta,
        }
    }
}

/// Bot deployment status
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BotStatus {
    Idle,
    Waiting,
    Deploying,
    Deployed,
    Checkpointing,
}

impl BotStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotStatus::Idle => "Idle",
            BotStatus::Waiting => "Waiting",
            BotStatus::Deploying => "Deploying",
            BotStatus::Deployed => "Deployed",
            BotStatus::Checkpointing => "Checkpointing",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            BotStatus::Idle => Color::Gray,
            BotStatus::Waiting => Color::Yellow,
            BotStatus::Deploying => Color::Cyan,
            BotStatus::Deployed => Color::Green,
            BotStatus::Checkpointing => Color::Magenta,
        }
    }
}

/// Session statistics (in-memory, resets on restart)
#[derive(Clone, Debug)]
pub struct SessionStats {
    pub started_at: Instant,
}

impl Default for SessionStats {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl SessionStats {
    pub fn running_time(&self) -> Duration {
        self.started_at.elapsed()
    }
    
    pub fn running_time_str(&self) -> String {
        let elapsed = self.running_time();
        let secs = elapsed.as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        
        if hours > 0 {
            format!("{}h {}m", hours, mins % 60)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs % 60)
        } else {
            format!("{}s", secs)
        }
    }
}

/// Per-bot session statistics with P&L tracking
#[derive(Clone, Debug)]
pub struct BotSessionStats {
    pub started_at: Instant,
    pub rounds_participated: u64,
    pub rounds_won: u64,
    /// Rounds skipped (EV was negative, didn't deploy) - EV strategy only
    pub rounds_skipped: u64,
    /// Rounds missed (tx failed for reason other than EV skip)
    pub rounds_missed: u64,
    /// Starting claimable SOL (rewards_sol at session start)
    pub starting_claimable_sol: u64,
    /// Current claimable SOL (updated after each checkpoint)
    pub current_claimable_sol: u64,
    /// Starting ORE balance for delta tracking
    pub starting_ore: u64,
    /// Current ORE balance
    pub current_ore: u64,
    /// Starting signer balance (for fee tracking)
    pub starting_signer_balance: u64,
    /// Total SOL claimed this session (to accurately track P&L after claims)
    pub total_claimed_sol: u64,
    /// Total ORE claimed this session
    pub total_claimed_ore: u64,
    /// Flag to track if starting balances have been set
    pub initialized: bool,
}

impl Default for BotSessionStats {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            rounds_participated: 0,
            rounds_won: 0,
            rounds_skipped: 0,
            rounds_missed: 0,
            starting_claimable_sol: 0,
            current_claimable_sol: 0,
            starting_ore: 0,
            current_ore: 0,
            starting_signer_balance: 0,
            total_claimed_sol: 0,
            total_claimed_ore: 0,
            initialized: false,
        }
    }
}

impl BotSessionStats {
    /// Initialize with starting balances
    pub fn with_starting_balances(claimable_sol: u64, ore: u64) -> Self {
        Self {
            started_at: Instant::now(),
            rounds_participated: 0,
            rounds_won: 0,
            rounds_skipped: 0,
            rounds_missed: 0,
            starting_claimable_sol: claimable_sol,
            current_claimable_sol: claimable_sol,
            starting_ore: ore,
            current_ore: ore,
            starting_signer_balance: 0, // Set on first signer balance update
            total_claimed_sol: 0,
            total_claimed_ore: 0,
            initialized: true,
        }
    }
    
    /// Calculate SOL P&L (can be negative)
    /// P&L = (total_claimed + current_claimable) - starting_claimable
    pub fn sol_pnl(&self) -> i64 {
        (self.total_claimed_sol + self.current_claimable_sol) as i64 - self.starting_claimable_sol as i64
    }
    
    /// Calculate ORE earned
    /// P&L = (total_claimed + current) - starting
    pub fn ore_pnl(&self) -> i64 {
        (self.total_claimed_ore + self.current_ore) as i64 - self.starting_ore as i64
    }
    
    pub fn running_time(&self) -> Duration {
        self.started_at.elapsed()
    }
    
    pub fn running_time_str(&self) -> String {
        let elapsed = self.running_time();
        let secs = elapsed.as_secs();
        let mins = secs / 60;
        let hours = mins / 60;
        
        if hours > 0 {
            format!("{}h {}m", hours, mins % 60)
        } else if mins > 0 {
            format!("{}m {}s", mins, secs % 60)
        } else {
            format!("{}s", secs)
        }
    }
    
    pub fn win_rate(&self) -> f64 {
        if self.rounds_participated == 0 {
            0.0
        } else {
            (self.rounds_won as f64 / self.rounds_participated as f64) * 100.0
        }
    }
}

/// Transaction log entry with detailed info
#[derive(Clone, Debug)]
pub struct TxLogEntry {
    pub timestamp: Instant,
    pub bot_name: String,
    pub tx_type: TxType,
    pub status: TxStatus,
    pub signature: Signature,
    pub error: Option<String>,
    /// Slot when tx was sent/confirmed
    pub slot: Option<u64>,
    /// Round ID the tx relates to
    pub round_id: Option<u64>,
    /// Amount involved (deployed lamports, claimed lamports, etc.)
    pub amount: Option<u64>,
    /// Attempt number (for deploy retries)
    pub attempt: Option<u64>,
}

/// Type of transaction
#[derive(Clone, Debug)]
pub enum TxType {
    Deploy,
    Checkpoint,
    ClaimSol,
    ClaimOre,
}

impl TxType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxType::Deploy => "DEPLOY",
            TxType::Checkpoint => "CHECKPOINT",
            TxType::ClaimSol => "CLAIM_SOL",
            TxType::ClaimOre => "CLAIM_ORE",
        }
    }
}

/// Status of transaction
#[derive(Clone, Debug)]
pub enum TxStatus {
    Sent,
    Confirmed,
    Failed,
}

impl TxStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxStatus::Sent => "SENT",
            TxStatus::Confirmed => "OK",
            TxStatus::Failed => "FAIL",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            TxStatus::Sent => Color::Cyan,
            TxStatus::Confirmed => Color::Green,
            TxStatus::Failed => Color::Red,
        }
    }
}

// Legacy TxAction for backwards compatibility
#[derive(Clone, Debug)]
pub enum TxAction {
    Sent,
    Confirmed,
    Failed,
}

impl TxAction {
    pub fn to_status(&self) -> TxStatus {
        match self {
            TxAction::Sent => TxStatus::Sent,
            TxAction::Confirmed => TxStatus::Confirmed,
            TxAction::Failed => TxStatus::Failed,
        }
    }
}

/// Single bot state (will be Vec<BotState> for multi-bot)
#[derive(Clone, Debug)]
pub struct BotState {
    pub name: String,
    pub icon: &'static str,
    pub auth_id: u64,
    pub strategy: String,
    pub bankroll: u64,
    pub slots_left_threshold: u64,
    pub status: BotStatus,
    pub deployed_this_round: u64,
    pub miner: Option<Miner>,
    pub session_stats: BotSessionStats,
    pub signer: Pubkey,
    pub manager: Pubkey,
    pub managed_miner_auth: Pubkey,  // Auth PDA
    /// Signer (fee payer) SOL balance (lamports)
    pub signer_balance: u64,
    // EV strategy params
    pub max_per_square: u64,
    pub min_bet: u64,
    pub ore_value: u64,
    // Percentage strategy params
    pub percentage: u64,       // In basis points (100 = 1%)
    pub squares_count: u64,    // Number of squares
}

/// Helper to format pubkey as shortened version (7...7)
pub fn shorten_pubkey(pubkey: &Pubkey) -> String {
    let s = pubkey.to_string();
    format!("{}...{}", &s[..7], &s[s.len()-7..])
}

/// Helper to format signature as shortened version (7...7)
pub fn shorten_signature(sig: &Signature) -> String {
    let s = sig.to_string();
    if s.len() > 14 {
        format!("{}...{}", &s[..7], &s[s.len()-7..])
    } else {
        s
    }
}

impl BotState {
    pub fn new(
        name: String,
        auth_id: u64,
        strategy: String,
        bankroll: u64,
        slots_left_threshold: u64,
        signer: Pubkey,
        manager: Pubkey,
        managed_miner_auth: Pubkey,
        max_per_square: u64,
        min_bet: u64,
        ore_value: u64,
        percentage: u64,
        squares_count: u64,
    ) -> Self {
        // Pick icon based on strategy
        let icon = match strategy.as_str() {
            "EV" => "📊",
            "Percentage" => "📐",
            "Manual" => "✋",
            _ => "🤖",
        };
        
        Self {
            name,
            icon,
            auth_id,
            strategy,
            bankroll,
            slots_left_threshold,
            status: BotStatus::Idle,
            deployed_this_round: 0,
            miner: None,
            session_stats: BotSessionStats::default(),
            signer,
            manager,
            managed_miner_auth,
            signer_balance: 0,
            max_per_square,
            min_bet,
            ore_value,
            percentage,
            squares_count,
        }
    }
    
    pub fn rewards_sol(&self) -> u64 {
        self.miner.as_ref().map(|m| m.rewards_sol).unwrap_or(0)
    }
    
    pub fn rewards_ore(&self) -> u64 {
        self.miner.as_ref().map(|m| m.rewards_ore).unwrap_or(0)
    }
}

/// Selectable element type for cursor navigation
#[derive(Clone, Debug, PartialEq)]
pub enum SelectableElement {
    BotSigner(usize),        // Bot index
    BotAuthPda(usize),       // Bot index
    TxLog(usize),            // Transaction log index (0 = most recent)
}

/// App state for the TUI
pub struct App {
    pub running: bool,
    
    // Connection info
    pub rpc_name: String,
    
    // Session stats
    pub session_stats: SessionStats,
    
    // Live chain state
    pub current_slot: u64,
    pub latest_blockhash: Hash,
    pub board: Option<Board>,
    pub round: Option<Round>,
    
    // Bots (single for now, Vec for multi-bot later)
    pub bots: Vec<BotState>,
    
    // Transaction log
    pub tx_log: Vec<TxLogEntry>,
    
    // Cursor/selection state
    pub selected: Option<SelectableElement>,
    pub clipboard_msg: Option<(String, Instant)>,  // Message and when it was set
}

impl App {
    pub fn new(rpc_url: &str) -> Self {
        // Extract RPC name from URL for display
        let rpc_name = if rpc_url.contains("helius") {
            "helius".to_string()
        } else if rpc_url.contains("quicknode") {
            "quicknode".to_string()
        } else if rpc_url.contains("mainnet-beta") {
            "mainnet".to_string()
        } else if rpc_url.contains("devnet") {
            "devnet".to_string()
        } else {
            "custom".to_string()
        };
        
        Self {
            running: true,
            rpc_name,
            session_stats: SessionStats::default(),
            current_slot: 0,
            latest_blockhash: Hash::default(),
            board: None,
            round: None,
            bots: Vec::new(),
            tx_log: Vec::new(),
            selected: None,
            clipboard_msg: None,
        }
    }
    
    /// Move selection up
    pub fn select_prev(&mut self) {
        self.selected = match &self.selected {
            None => {
                // Start at first bot signer if available
                if !self.bots.is_empty() {
                    Some(SelectableElement::BotSigner(0))
                } else if !self.tx_log.is_empty() {
                    Some(SelectableElement::TxLog(0))
                } else {
                    None
                }
            }
            Some(SelectableElement::BotSigner(i)) => {
                if *i > 0 {
                    Some(SelectableElement::BotAuthPda(i - 1))
                } else {
                    // Wrap to tx log if available
                    if !self.tx_log.is_empty() {
                        Some(SelectableElement::TxLog(self.tx_log.len().min(30) - 1))
                    } else {
                        Some(SelectableElement::BotSigner(0))
                    }
                }
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                Some(SelectableElement::BotSigner(*i))
            }
            Some(SelectableElement::TxLog(i)) => {
                if *i > 0 {
                    Some(SelectableElement::TxLog(i - 1))
                } else {
                    // Wrap to last bot auth pda
                    if !self.bots.is_empty() {
                        Some(SelectableElement::BotAuthPda(self.bots.len() - 1))
                    } else {
                        Some(SelectableElement::TxLog(0))
                    }
                }
            }
        };
    }
    
    /// Move selection down
    pub fn select_next(&mut self) {
        self.selected = match &self.selected {
            None => {
                // Start at first bot signer if available
                if !self.bots.is_empty() {
                    Some(SelectableElement::BotSigner(0))
                } else if !self.tx_log.is_empty() {
                    Some(SelectableElement::TxLog(0))
                } else {
                    None
                }
            }
            Some(SelectableElement::BotSigner(i)) => {
                Some(SelectableElement::BotAuthPda(*i))
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                if *i + 1 < self.bots.len() {
                    Some(SelectableElement::BotSigner(i + 1))
                } else {
                    // Move to tx log
                    if !self.tx_log.is_empty() {
                        Some(SelectableElement::TxLog(0))
                    } else {
                        Some(SelectableElement::BotSigner(0))
                    }
                }
            }
            Some(SelectableElement::TxLog(i)) => {
                let max_log = self.tx_log.len().min(30);
                if *i + 1 < max_log {
                    Some(SelectableElement::TxLog(i + 1))
                } else {
                    // Wrap to first bot
                    if !self.bots.is_empty() {
                        Some(SelectableElement::BotSigner(0))
                    } else {
                        Some(SelectableElement::TxLog(0))
                    }
                }
            }
        };
    }
    
    /// Get the copyable value for current selection
    pub fn get_selected_value(&self) -> Option<String> {
        match &self.selected {
            Some(SelectableElement::BotSigner(i)) => {
                self.bots.get(*i).map(|b| b.signer.to_string())
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                self.bots.get(*i).map(|b| b.managed_miner_auth.to_string())
            }
            Some(SelectableElement::TxLog(i)) => {
                self.tx_log.iter().rev().nth(*i).map(|e| e.signature.to_string())
            }
            None => None,
        }
    }
    
    /// Copy selected value to clipboard
    pub fn copy_selected(&mut self) {
        if let Some(value) = self.get_selected_value() {
            match cli_clipboard::set_contents(value) {
                Ok(_) => {
                    self.clipboard_msg = Some(("Copied!".to_string(), Instant::now()));
                }
                Err(_) => {
                    self.clipboard_msg = Some(("Copy failed".to_string(), Instant::now()));
                }
            }
        }
    }
    
    /// Add a bot to the dashboard
    pub fn add_bot(&mut self, bot: BotState) {
        self.bots.push(bot);
    }
    
    /// Get current round phase
    pub fn round_phase(&self) -> RoundPhase {
        self.board
            .as_ref()
            .map(|b| RoundPhase::calculate(self.current_slot, b))
            .unwrap_or(RoundPhase::WaitingStart)
    }
    
    /// Get slots remaining in current round
    pub fn slots_remaining(&self) -> u64 {
        self.board
            .as_ref()
            .map(|b| {
                if b.end_slot == u64::MAX {
                    0
                } else {
                    b.end_slot.saturating_sub(self.current_slot)
                }
            })
            .unwrap_or(0)
    }
    
    /// Get round ID
    pub fn round_id(&self) -> u64 {
        self.board.as_ref().map(|b| b.round_id).unwrap_or(0)
    }
    
    /// Get end slot
    pub fn end_slot(&self) -> u64 {
        self.board.as_ref().map(|b| b.end_slot).unwrap_or(0)
    }
    
    /// Log a transaction (new format with type and details)
    pub fn log_tx_typed(
        &mut self,
        bot_name: String,
        tx_type: TxType,
        status: TxStatus,
        signature: Signature,
        error: Option<String>,
        slot: Option<u64>,
        round_id: Option<u64>,
        amount: Option<u64>,
        attempt: Option<u64>,
    ) {
        self.tx_log.push(TxLogEntry {
            timestamp: Instant::now(),
            bot_name,
            tx_type,
            status,
            signature,
            error,
            slot,
            round_id,
            amount,
            attempt,
        });
        
        // Keep last 100 entries
        if self.tx_log.len() > 100 {
            self.tx_log.remove(0);
        }
    }
    
    /// Log a transaction (legacy format - converts to Deploy type)
    pub fn log_tx(&mut self, bot_name: String, action: TxAction, signature: Signature, error: Option<String>) {
        self.log_tx_typed(bot_name, TxType::Deploy, action.to_status(), signature, error, None, None, None, None);
    }
    
    /// Update slot
    pub fn update_slot(&mut self, slot: u64) {
        self.current_slot = slot;
    }
    
    /// Update blockhash
    pub fn update_blockhash(&mut self, blockhash: Hash) {
        self.latest_blockhash = blockhash;
    }
    
    /// Apply a TuiUpdate message from a bot task
    pub fn apply_update(&mut self, update: TuiUpdate) {
        match update {
            TuiUpdate::SlotUpdate { slot, blockhash } => {
                self.current_slot = slot;
                self.latest_blockhash = blockhash;
            }
            TuiUpdate::BoardUpdate(board) => {
                self.board = Some(board);
            }
            TuiUpdate::RoundUpdate(round) => {
                self.round = Some(round);
            }
            TuiUpdate::BotStatusUpdate { bot_index, status } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.status = status;
                }
            }
            TuiUpdate::BotMinerUpdate { bot_index, miner } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.miner = Some(miner);
                }
            }
            TuiUpdate::BotDeployedUpdate { bot_index, amount, round_id: _ } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.deployed_this_round = amount;
                }
            }
            TuiUpdate::BotStatsUpdate { 
                bot_index, 
                rounds_participated,
                rounds_won,
                rounds_skipped,
                rounds_missed,
                current_claimable_sol,
                current_ore,
            } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    // Only set starting values once (on first update when not initialized)
                    if !bot.session_stats.initialized {
                        bot.session_stats.starting_claimable_sol = current_claimable_sol;
                        bot.session_stats.starting_ore = current_ore;
                        bot.session_stats.initialized = true;
                    }
                    bot.session_stats.rounds_participated = rounds_participated;
                    bot.session_stats.rounds_won = rounds_won;
                    bot.session_stats.rounds_skipped = rounds_skipped;
                    bot.session_stats.rounds_missed = rounds_missed;
                    bot.session_stats.current_claimable_sol = current_claimable_sol;
                    bot.session_stats.current_ore = current_ore;
                }
            }
            TuiUpdate::BotSignerBalanceUpdate { bot_index, balance } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    // Set starting balance on first update
                    if bot.session_stats.starting_signer_balance == 0 {
                        bot.session_stats.starting_signer_balance = balance;
                    }
                    bot.signer_balance = balance;
                }
            }
            TuiUpdate::BotClaimedSol { bot_index, amount } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.session_stats.total_claimed_sol += amount;
                }
            }
            TuiUpdate::BotClaimedOre { bot_index, amount } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.session_stats.total_claimed_ore += amount;
                }
            }
            TuiUpdate::TxEvent { bot_name, action, signature, error } => {
                self.log_tx(bot_name, action, signature, error);
            }
            TuiUpdate::TxEventTyped { bot_name, tx_type, status, signature, error, slot, round_id, amount, attempt } => {
                self.log_tx_typed(bot_name, tx_type, status, signature, error, slot, round_id, amount, attempt);
            }
            TuiUpdate::Error(msg) => {
                // Log error as a failed tx entry for now
                self.log_tx_typed("system".to_string(), TxType::Deploy, TxStatus::Failed, Signature::default(), Some(msg), None, None, None, None);
            }
        }
    }
}

// =============================================================================
// Terminal Management
// =============================================================================

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> io::Result<Tui> {
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

// =============================================================================
// Main Draw Function
// =============================================================================

pub fn draw(frame: &mut Frame, app: &App) {
    // Main layout: Header, Bot Blocks, Transaction Log, Board
    // Prioritize bot stats and logs over board display
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Length(17),  // Bot block(s) - with config info
            Constraint::Min(10),     // Transaction log - flexible
            Constraint::Length(3),   // Board grid - minimal
        ])
        .split(frame.area());
    
    draw_header(frame, chunks[0], app);
    draw_bot_blocks(frame, chunks[1], app);
    draw_tx_log(frame, chunks[2], app);
    draw_board_grid(frame, chunks[3], app);
}

// =============================================================================
// Header Section
// =============================================================================

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let phase = app.round_phase();
    let slots_left = app.slots_remaining();
    
    // Format blockhash (truncated)
    let blockhash_str = if app.latest_blockhash == Hash::default() {
        "...".to_string()
    } else {
        let s = app.latest_blockhash.to_string();
        format!("{}...", &s[..8])
    };
    
    // Format end_slot
    let end_slot_str = if app.end_slot() == u64::MAX {
        "MAX".to_string()
    } else {
        app.end_slot().to_string()
    };
    
    let line1 = Line::from(vec![
        Span::styled("  ⚡ EVORE ", Style::default().fg(Color::Magenta).bold()),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Round: {} ", app.round_id()), Style::default().fg(Color::White)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Slot: {} / {} ", app.current_slot, end_slot_str), Style::default().fg(Color::Cyan)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} left ", slots_left),
            if slots_left <= 2 && phase == RoundPhase::Active {
                Style::default().fg(Color::Red).bold()
            } else {
                Style::default().fg(Color::Yellow)
            }
        ),
    ]);
    
    let line2 = Line::from(vec![
        Span::styled("  Phase: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ", phase.as_str()), Style::default().fg(phase.color()).bold()),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Session: {} ", app.session_stats.running_time_str()), Style::default().fg(Color::White)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("RPC: {} ", app.rpc_name), Style::default().fg(Color::Cyan)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Hash: {}", blockhash_str), Style::default().fg(Color::DarkGray)),
        // Clipboard status
        if let Some((msg, _)) = &app.clipboard_msg {
            Span::styled(format!("  [{}]", msg), Style::default().fg(Color::Green).bold())
        } else {
            Span::styled("", Style::default())
        },
        // Help text
        Span::styled("  ↑↓:nav Enter:copy q:quit", Style::default().fg(Color::DarkGray)),
    ]);
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));
    
    let paragraph = Paragraph::new(vec![line1, line2]).block(block);
    frame.render_widget(paragraph, area);
}

// =============================================================================
// Bot Blocks Section
// =============================================================================

fn draw_bot_blocks(frame: &mut Frame, area: Rect, app: &App) {
    if app.bots.is_empty() {
        let block = Block::default()
            .title(" No Bots Configured ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray));
        let paragraph = Paragraph::new("Add bots via config file")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(paragraph, area);
        return;
    }
    
    // For now, single bot - later will split horizontally for multiple bots
    let bot_areas = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(
            app.bots.iter().map(|_| Constraint::Ratio(1, app.bots.len() as u32)).collect::<Vec<_>>()
        )
        .split(area);
    
    for (i, bot) in app.bots.iter().enumerate() {
        draw_single_bot_block(frame, bot_areas[i], bot, app, i);
    }
}

fn draw_single_bot_block(frame: &mut Frame, area: Rect, bot: &BotState, app: &App, bot_index: usize) {
    let phase = app.round_phase();
    let slots_left = app.slots_remaining();
    
    // Status with countdown if waiting
    let status_str = if bot.status == BotStatus::Waiting && phase == RoundPhase::Active {
        let until_deploy = slots_left.saturating_sub(bot.slots_left_threshold);
        format!("{} ({} slots)", bot.status.as_str(), until_deploy)
    } else {
        bot.status.as_str().to_string()
    };
    
    // Format amounts
    let bankroll_sol = bot.bankroll as f64 / 1e9;
    let deployed_sol = bot.deployed_this_round as f64 / 1e9;
    let signer_sol = bot.signer_balance as f64 / 1e9;
    
    // Session stats
    let stats = &bot.session_stats;
    
    // Simple P&L: current signer balance - starting signer balance
    let sol_pnl = if stats.starting_signer_balance > 0 {
        (bot.signer_balance as i64 - stats.starting_signer_balance as i64) as f64 / 1e9
    } else {
        0.0
    };
    let ore_pnl = stats.ore_pnl() as f64 / 1e11;
    
    // Format P&L with sign
    let (sol_pnl_str, sol_pnl_color) = if sol_pnl >= 0.0 {
        (format!("+{:.4}", sol_pnl), Color::Green)
    } else {
        (format!("{:.4}", sol_pnl), Color::Red)
    };
    let (ore_pnl_str, ore_pnl_color) = if ore_pnl >= 0.0 {
        (format!("+{:.2}", ore_pnl), Color::Rgb(255, 165, 0))
    } else {
        (format!("{:.2}", ore_pnl), Color::Red)
    };
    
    // Cost per ORE calculation
    let ore_earned = stats.ore_pnl() as f64 / 1e11;
    let cost_per_ore = if ore_earned > 0.001 {
        let sol_cost = -sol_pnl;
        if sol_cost > 0.0 { sol_cost / ore_earned } else { 0.0 }
    } else {
        0.0
    };
    
    let title = format!(" {} {} ", bot.icon, bot.name);
    
    // Check selection state for highlighting
    let signer_selected = app.selected == Some(SelectableElement::BotSigner(bot_index));
    let auth_selected = app.selected == Some(SelectableElement::BotAuthPda(bot_index));
    
    // Build lines with visual sections
    let mut lines = vec![
        // ═══ STATUS BAR ═══
        Line::from(vec![
            Span::styled("▶ ", Style::default().fg(bot.status.color())),
            Span::styled(status_str, Style::default().fg(bot.status.color()).bold()),
        ]),
        // ═══ PUBKEYS (selectable) ═══
        Line::from(vec![
            if signer_selected { Span::styled("► ", Style::default().fg(Color::White).bold()) } 
            else { Span::styled("  ", Style::default()) },
            Span::styled("Signer ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                shorten_pubkey(&bot.signer), 
                if signer_selected { Style::default().fg(Color::White).bold().on_blue() } 
                else { Style::default().fg(Color::Gray) }
            ),
            Span::styled(format!("  {:.4} ◎", signer_sol), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            if auth_selected { Span::styled("► ", Style::default().fg(Color::White).bold()) } 
            else { Span::styled("  ", Style::default()) },
            Span::styled("Auth   ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                shorten_pubkey(&bot.managed_miner_auth), 
                if auth_selected { Style::default().fg(Color::White).bold().on_blue() } 
                else { Style::default().fg(Color::Gray) }
            ),
        ]),
        // ═══ BALANCES ═══
        Line::from(vec![
            Span::styled("◈ Bankroll  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} ◎", bankroll_sol), Style::default().fg(Color::Cyan)),
            Span::styled("   Deployed  ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} ◎", deployed_sol), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("◈ Claimable ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.2} ORE", bot.rewards_ore() as f64 / 1e11), Style::default().fg(Color::Rgb(255, 165, 0))),
        ]),
    ];
    
    // Strategy-specific config
    match bot.strategy.as_str() {
        "EV" => {
            let max_sq = bot.max_per_square as f64 / 1e9;
            let min_b = bot.min_bet as f64 / 1e9;
            let ore_val = bot.ore_value as f64 / 1e9;
            lines.push(Line::from(vec![
                Span::styled("◈ Config   ", Style::default().fg(Color::DarkGray)),
                Span::styled("max=", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:.2}", max_sq), Style::default().fg(Color::White)),
                Span::styled(" min=", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:.4}", min_b), Style::default().fg(Color::White)),
                Span::styled(" ore=", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:.2}", ore_val), Style::default().fg(Color::Rgb(255, 165, 0))),
                Span::styled(" @", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}slots", bot.slots_left_threshold), Style::default().fg(Color::Yellow)),
            ]));
        }
        "Percentage" => {
            let pct = bot.percentage as f64 / 100.0;
            lines.push(Line::from(vec![
                Span::styled("◈ Config   ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}%", pct), Style::default().fg(Color::Magenta)),
                Span::styled(" × ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} sq", bot.squares_count), Style::default().fg(Color::Cyan)),
                Span::styled(" @", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}slots", bot.slots_left_threshold), Style::default().fg(Color::Yellow)),
            ]));
        }
        _ => {
            lines.push(Line::from(vec![
                Span::styled("◈ Config   ", Style::default().fg(Color::DarkGray)),
                Span::styled("@", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}slots", bot.slots_left_threshold), Style::default().fg(Color::Yellow)),
            ]));
        }
    };
    
    // Session section
    lines.push(Line::from(vec![
        Span::styled("━━━ Session ", Style::default().fg(Color::Blue)),
        Span::styled(stats.running_time_str(), Style::default().fg(Color::White).bold()),
        Span::styled(" ━━━━━━━━━━━━━━━", Style::default().fg(Color::Blue)),
    ]));

    // Strategy-specific round stats
    match bot.strategy.as_str() {
        "EV" => {
            let total_rounds = stats.rounds_participated + stats.rounds_skipped + stats.rounds_missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total_rounds), Style::default().fg(Color::Cyan)),
                Span::styled(" Deployed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", stats.rounds_participated), Style::default().fg(Color::Yellow)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", stats.rounds_won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Skip ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<6}", stats.rounds_skipped), Style::default().fg(Color::DarkGray)),
                Span::styled(" Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", stats.rounds_missed), Style::default().fg(if stats.rounds_missed > 0 { Color::Red } else { Color::DarkGray })),
            ]));
        }
        "Percentage" => {
            let total = stats.rounds_participated + stats.rounds_missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total), Style::default().fg(Color::Cyan)),
                Span::styled(" Deployed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", stats.rounds_participated), Style::default().fg(Color::Yellow)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", stats.rounds_won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", stats.rounds_missed), Style::default().fg(if stats.rounds_missed > 0 { Color::Red } else { Color::DarkGray })),
            ]));
        }
        _ => {
            let total = stats.rounds_participated + stats.rounds_missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total), Style::default().fg(Color::Cyan)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", stats.rounds_won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", stats.rounds_missed), Style::default().fg(if stats.rounds_missed > 0 { Color::Red } else { Color::DarkGray })),
            ]));
        }
    };

    // P&L section
    lines.push(Line::from(vec![
        Span::styled("P&L  ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ◎", sol_pnl_str), Style::default().fg(sol_pnl_color).bold()),
        Span::styled("   ", Style::default()),
        Span::styled(format!("{} ORE", ore_pnl_str), Style::default().fg(ore_pnl_color)),
    ]));
    
    // SOL Spent (total cost, only if negative P&L)
    let sol_spent = if sol_pnl < 0.0 { -sol_pnl } else { 0.0 };
    if sol_spent > 0.0 {
        lines.push(Line::from(vec![
            Span::styled("Spent ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} ◎", sol_spent), Style::default().fg(Color::Yellow)),
            // Cost per ORE (if earned ORE)
            if cost_per_ore > 0.0 {
                Span::styled(format!("  ({:.4} ◎/ORE)", cost_per_ore), Style::default().fg(Color::Cyan))
            } else {
                Span::styled("", Style::default())
            },
        ]));
    } else if cost_per_ore > 0.0 {
        // Show cost per ORE even if in profit
        lines.push(Line::from(vec![
            Span::styled("Cost ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} ◎/ORE", cost_per_ore), Style::default().fg(Color::Cyan)),
        ]));
    }
    
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(Color::White).bold())
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue));
    
    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

// =============================================================================
// Board Grid Section
// =============================================================================

fn draw_board_grid(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Board (5×5) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    // If no round data, show placeholder
    let round = match &app.round {
        Some(r) => r,
        None => {
            let placeholder = Paragraph::new("No round data")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(placeholder, inner);
            return;
        }
    };
    
    // Calculate max deployment for color scaling
    let max_deploy = round.deployed.iter().max().copied().unwrap_or(1).max(1);
    
    // Build table rows for 5x5 grid
    let mut rows = Vec::new();
    
    for row_idx in 0..5 {
        let mut cells = Vec::new();
        
        for col_idx in 0..5 {
            let idx = row_idx * 5 + col_idx;
            let deployed = round.deployed[idx];
            let sol = deployed as f64 / 1e9;
            
            // Format cell content
            let text = if sol >= 1.0 {
                format!("{}: {:.2}", idx, sol)
            } else if sol >= 0.01 {
                format!("{}: {:.3}", idx, sol)
            } else if deployed > 0 {
                format!("{}: ◆", idx)
            } else {
                format!("{}: ·", idx)
            };
            
            // Color intensity based on deployment
            let intensity = ((deployed as f64 / max_deploy as f64) * 200.0) as u8;
            let fg_color = if deployed > 0 {
                Color::Rgb(200 + intensity / 4, 200 + intensity / 4, 255)
            } else {
                Color::DarkGray
            };
            
            cells.push(Span::styled(text, Style::default().fg(fg_color)));
        }
        
        rows.push(Row::new(cells).height(1));
    }
    
    let widths = [
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
    ];
    
    let table = Table::new(rows, widths)
        .column_spacing(1);
    
    frame.render_widget(table, inner);
}

// =============================================================================
// Transaction Log Section
// =============================================================================

fn draw_tx_log(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Transaction Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    if app.tx_log.is_empty() {
        let paragraph = Paragraph::new("No transactions yet")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(block);
        frame.render_widget(paragraph, area);
        return;
    }
    
    let items: Vec<ListItem> = app.tx_log
        .iter()
        .rev()
        .take(30)  // Show more logs
        .enumerate()
        .map(|(idx, entry)| {
            let is_selected = app.selected == Some(SelectableElement::TxLog(idx));
            
            let elapsed = entry.timestamp.elapsed().as_secs();
            let time_str = if elapsed < 60 {
                format!("{:>3}s", elapsed)
            } else {
                format!("{:>3}m", elapsed / 60)
            };
            
            let sig_str = if entry.signature == Signature::default() {
                "--------".to_string()
            } else {
                entry.signature.to_string()[..8].to_string()
            };
            
            // Color for tx_type
            let tx_type_color = match entry.tx_type {
                TxType::Deploy => Color::Yellow,
                TxType::Checkpoint => Color::Magenta,
                TxType::ClaimSol => Color::Green,
                TxType::ClaimOre => Color::Rgb(255, 165, 0),
            };
            
            // Truncate bot name to 8 chars for consistent column alignment
            let bot_name_display = if entry.bot_name.len() > 8 {
                format!("{:.8}", entry.bot_name)
            } else {
                entry.bot_name.clone()
            };
            
            // Check if this is an expected "skip" error (gray out entire entry)
            let is_skip_error = entry.error.as_ref().map_or(false, |e| 
                e.contains("AlreadyDeployed") || e.contains("NoDeployments"));
            
            // For skip errors, show "N/A" status in gray; otherwise normal status
            let status_display = if is_skip_error { "N/A " } else { entry.status.as_str() };
            let status_color = if is_skip_error { Color::DarkGray } else { entry.status.color() };
            
            // Selection indicator
            let select_indicator = if is_selected { "► " } else { "  " };
            
            let mut spans = vec![
                Span::styled(select_indicator, Style::default().fg(Color::White).bold()),
                Span::styled(format!("[{}] ", time_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<8} ", bot_name_display), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:<10} ", entry.tx_type.as_str()), Style::default().fg(tx_type_color)),
                Span::styled(format!("{:<4} ", status_display), Style::default().fg(status_color)),
            ];
            
            // Add slot info if available
            if let Some(slot) = entry.slot {
                spans.push(Span::styled(format!("s:{} ", slot), Style::default().fg(Color::DarkGray)));
            }
            
            // Add round info if available  
            if let Some(round_id) = entry.round_id {
                spans.push(Span::styled(format!("r:{} ", round_id), Style::default().fg(Color::Blue)));
            }
            
            // Add amount info based on tx type
            if let Some(amount) = entry.amount {
                let amount_str = match entry.tx_type {
                    TxType::Deploy => format!("{:.4}◎ ", amount as f64 / 1e9),
                    TxType::ClaimSol => format!("+{:.4}◎ ", amount as f64 / 1e9),
                    TxType::ClaimOre => format!("+{:.4}ORE ", amount as f64 / 1e11),
                    TxType::Checkpoint => format!("r:{} ", amount),
                };
                let amount_color = match entry.tx_type {
                    TxType::Deploy => Color::Yellow,
                    TxType::ClaimSol | TxType::ClaimOre => Color::Green,
                    TxType::Checkpoint => Color::Magenta,
                };
                spans.push(Span::styled(amount_str, Style::default().fg(amount_color)));
            }
            
            // Add attempt number for deploys (1-indexed for display)
            if let Some(attempt) = entry.attempt {
                spans.push(Span::styled(format!("#{} ", attempt + 1), Style::default().fg(Color::DarkGray)));
            }
            
            // Add signature (shortened) - highlight if selected
            let sig_style = if is_selected {
                Style::default().fg(Color::White).bold().on_blue()
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(format!("{}... ", sig_str), sig_style));
            
            // Add error if present
            if let Some(error) = &entry.error {
                let err_display = if error.len() > 25 {
                    format!("{:.25}...", error)
                } else {
                    error.clone()
                };
                // Gray for expected skip errors, red for actual errors
                let err_color = if is_skip_error { Color::DarkGray } else { Color::Red };
                spans.push(Span::styled(err_display, Style::default().fg(err_color)));
            }
            
            ListItem::new(Line::from(spans))
        })
        .collect();
    
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

// =============================================================================
// Input Handling
// =============================================================================

/// Handle keyboard input
/// Returns true if the app should quit
pub fn handle_input(app: &mut App) -> io::Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.running = false;
                        return Ok(true);
                    }
                    // Arrow key navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.select_next();
                    }
                    // Enter to copy selected value
                    KeyCode::Enter => {
                        app.copy_selected();
                    }
                    // Clear selection
                    KeyCode::Char('c') => {
                        app.selected = None;
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Clear clipboard message after 2 seconds
    if let Some((_, instant)) = &app.clipboard_msg {
        if instant.elapsed() > Duration::from_secs(2) {
            app.clipboard_msg = None;
        }
    }
    
    Ok(false)
}
