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
// Bot Icon Pool
// =============================================================================

/// Pool of unique icons for bots (assigned randomly, no duplicates)
pub const BOT_ICON_POOL: &[&str] = &[
    "🤖", "🎯", "🔥", "⚡", "🌟", "💎", "🎲", "🎰", 
    "🚀", "🌙", "🎪", "🎨", "🎭", "🎵", "🎸",
];

/// Get a unique icon for a bot based on its index
/// Uses modulo to wrap around if more bots than icons
pub fn get_bot_icon(bot_index: usize) -> &'static str {
    BOT_ICON_POOL[bot_index % BOT_ICON_POOL.len()]
}

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
    
    /// Bot miner data updated (full miner struct)
    BotMinerUpdate { bot_index: usize, miner: Miner },
    
    /// Miner deployment data update (from periodic polling)
    MinerDataUpdate { 
        bot_index: usize, 
        deployed: [u64; 25], 
        round_id: u64,
    },
    
    /// Treasury data update (from periodic polling)
    TreasuryUpdate(crate::treasury_tracker::TreasuryData),
    
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
    
    /// Network stats update
    NetworkStatsUpdate {
        slot_ws: Option<ConnectionStatus>,
        board_ws: Option<ConnectionStatus>,
        round_ws: Option<ConnectionStatus>,
        rpc: Option<ConnectionStatus>,
        sender_east_latency_ms: Option<u32>,
        sender_west_latency_ms: Option<u32>,
        rpc_rps: Option<u32>,
        sender_rps: Option<u32>,
    },
    
    /// Increment tx counters
    TxCounterUpdate {
        sent: Option<u64>,
        confirmed: Option<u64>,
        failed: Option<u64>,
    },
    
    /// Bot pause state changed
    BotPauseUpdate { bot_index: usize, is_paused: bool },
}

/// View mode for bottom section (toggled with Tab)
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ViewMode {
    #[default]
    TxLog,
    Board,
}

impl ViewMode {
    pub fn toggle(&mut self) {
        *self = match self {
            ViewMode::TxLog => ViewMode::Board,
            ViewMode::Board => ViewMode::TxLog,
        };
    }
    
    pub fn as_str(&self) -> &'static str {
        match self {
            ViewMode::TxLog => "Tx Log",
            ViewMode::Board => "Board",
        }
    }
}

/// Network connection status
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ConnectionStatus {
    #[default]
    Unknown,
    Connected,
    Reconnecting,
    Disconnected,
}

impl ConnectionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionStatus::Unknown => "?",
            ConnectionStatus::Connected => "✓",
            ConnectionStatus::Reconnecting => "↻",
            ConnectionStatus::Disconnected => "✗",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            ConnectionStatus::Unknown => Color::DarkGray,
            ConnectionStatus::Connected => Color::Green,
            ConnectionStatus::Reconnecting => Color::Yellow,
            ConnectionStatus::Disconnected => Color::Red,
        }
    }
}

/// Network statistics for footer display
#[derive(Clone, Debug, Default)]
pub struct NetworkStats {
    /// WebSocket connection statuses
    pub slot_ws: ConnectionStatus,
    pub board_ws: ConnectionStatus,
    pub round_ws: ConnectionStatus,
    
    /// RPC connection status
    pub rpc: ConnectionStatus,
    
    /// Helius sender latencies (ms)
    pub sender_east_latency_ms: Option<u32>,
    pub sender_west_latency_ms: Option<u32>,
    
    /// RPC requests per second (10s average)
    pub rpc_rps: u32,
    /// Sender HTTP sends per second (10s average)
    pub sender_rps: u32,
    /// Total RPC requests made
    pub rpc_total: u64,
    /// Total sender HTTP sends made
    pub sender_total: u64,
    
    /// Transaction stats
    /// - sent: all transactions submitted
    /// - confirmed: transactions that landed successfully (OK)
    /// - failed: transactions that got an actual error (NoDeployment, AlreadyDeployed, on-chain error)
    /// - missed: transactions that expired/dropped or had RPC errors
    pub txs_sent: u64,
    pub txs_confirmed: u64,
    pub txs_failed: u64,
    pub txs_missed: u64,
}

impl NetworkStats {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Calculate miss rate (missed / sent)
    pub fn miss_rate(&self) -> f64 {
        if self.txs_sent == 0 {
            0.0
        } else {
            (self.txs_missed as f64 / self.txs_sent as f64) * 100.0
        }
    }
    
    /// Calculate fail rate (failed / sent)
    pub fn fail_rate(&self) -> f64 {
        if self.txs_sent == 0 {
            0.0
        } else {
            (self.txs_failed as f64 / self.txs_sent as f64) * 100.0
        }
    }
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
    Paused,
    Loading,
    Idle,
    Waiting,
    Deploying,
    Deployed,
    Skipped,
    Missed,
    Checkpointing,
}

impl BotStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotStatus::Paused => "Paused",
            BotStatus::Loading => "Loading",
            BotStatus::Idle => "Idle",
            BotStatus::Waiting => "Waiting",
            BotStatus::Deploying => "Deploying",
            BotStatus::Deployed => "Deployed",
            BotStatus::Skipped => "Skipped",
            BotStatus::Missed => "Missed",
            BotStatus::Checkpointing => "Checkpointing",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            BotStatus::Paused => Color::Rgb(128, 128, 128),
            BotStatus::Loading => Color::Blue,
            BotStatus::Idle => Color::Gray,
            BotStatus::Waiting => Color::Yellow,
            BotStatus::Deploying => Color::Cyan,
            BotStatus::Deployed => Color::Green,
            BotStatus::Skipped => Color::DarkGray,
            BotStatus::Missed => Color::Red,
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
    
    // Offset values for session reset - subtract from incoming values
    pub rounds_participated_offset: u64,
    pub rounds_won_offset: u64,
    pub rounds_skipped_offset: u64,
    pub rounds_missed_offset: u64,
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
            rounds_participated_offset: 0,
            rounds_won_offset: 0,
            rounds_skipped_offset: 0,
            rounds_missed_offset: 0,
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
            rounds_participated_offset: 0,
            rounds_won_offset: 0,
            rounds_skipped_offset: 0,
            rounds_missed_offset: 0,
        }
    }
    
    /// Get effective rounds participated (after offset)
    pub fn effective_rounds_participated(&self) -> u64 {
        self.rounds_participated.saturating_sub(self.rounds_participated_offset)
    }
    
    /// Get effective rounds won (after offset)
    pub fn effective_rounds_won(&self) -> u64 {
        self.rounds_won.saturating_sub(self.rounds_won_offset)
    }
    
    /// Get effective rounds skipped (after offset)
    pub fn effective_rounds_skipped(&self) -> u64 {
        self.rounds_skipped.saturating_sub(self.rounds_skipped_offset)
    }
    
    /// Get effective rounds missed (after offset)
    pub fn effective_rounds_missed(&self) -> u64 {
        self.rounds_missed.saturating_sub(self.rounds_missed_offset)
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
        let participated = self.effective_rounds_participated();
        let won = self.effective_rounds_won();
        if participated == 0 {
            0.0
        } else {
            (won as f64 / participated as f64) * 100.0
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
    /// Whether this bot is paused
    pub is_paused: bool,
    pub deployed_this_round: u64,
    pub miner: Option<Miner>,
    pub session_stats: BotSessionStats,
    pub signer: Pubkey,
    pub manager: Pubkey,
    pub managed_miner_auth: Pubkey,  // Auth PDA
    /// Signer (fee payer) SOL balance (lamports)
    pub signer_balance: u64,
    // Fee params
    pub priority_fee: u64,     // Compute unit price (micro-lamports)
    pub jito_tip: u64,         // Jito tip amount (lamports)
    // EV strategy params
    pub max_per_square: u64,
    pub min_bet: u64,
    pub ore_value: u64,
    // Percentage strategy params
    pub percentage: u64,       // In basis points (100 = 1%)
    pub squares_count: u64,    // Number of squares
    // Per-square deployment tracking (from miner account)
    /// Deployed amounts per square in current round (from miner polling)
    pub deployed_per_square: [u64; 25],
    /// Round ID the bot last played in (from miner polling)
    pub miner_round_id: u64,
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
        bot_index: usize,
        auth_id: u64,
        strategy: String,
        bankroll: u64,
        slots_left_threshold: u64,
        signer: Pubkey,
        manager: Pubkey,
        managed_miner_auth: Pubkey,
        priority_fee: u64,
        jito_tip: u64,
        max_per_square: u64,
        min_bet: u64,
        ore_value: u64,
        percentage: u64,
        squares_count: u64,
    ) -> Self {
        // Assign unique icon from pool based on bot index
        let icon = get_bot_icon(bot_index);
        
        Self {
            name,
            icon,
            auth_id,
            strategy,
            bankroll,
            slots_left_threshold,
            status: BotStatus::Idle,
            is_paused: false,
            deployed_this_round: 0,
            miner: None,
            session_stats: BotSessionStats::default(),
            signer,
            manager,
            managed_miner_auth,
            signer_balance: 0,
            priority_fee,
            jito_tip,
            max_per_square,
            min_bet,
            ore_value,
            percentage,
            squares_count,
            deployed_per_square: [0; 25],
            miner_round_id: 0,
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
    BotPauseToggle(usize),   // Bot index - toggle pause state
    BotSigner(usize),        // Bot index - copies pubkey
    BotAuthPda(usize),       // Bot index - copies pubkey
    BotConfigReload(usize),  // Bot index - reloads config from file
    BotSessionRefresh(usize),// Bot index - resets session stats
    TxLog(usize),            // Transaction log index (0 = most recent) - copies signature
}

/// Action result from Enter key
#[derive(Clone, Debug)]
pub enum SelectAction {
    Copy(String),           // Copy value to clipboard
    ReloadConfig(usize),    // Reload config for bot at index
    RefreshSession(usize),  // Reset session stats for bot at index
    TogglePause(usize),     // Toggle pause for bot at index
    None,
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
    pub status_msg: Option<(String, Instant, bool)>,  // Message, when it was set, is_error
    
    // Config file path for hot reload
    pub config_path: Option<String>,
    
    // View mode toggle (Tab key)
    pub view_mode: ViewMode,
    
    // Network stats for footer
    pub network_stats: NetworkStats,
    
    // Treasury data
    pub treasury: Option<crate::treasury_tracker::TreasuryData>,
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
            status_msg: None,
            config_path: None,
            view_mode: ViewMode::default(),
            network_stats: NetworkStats::new(),
            treasury: None,
        }
    }
    
    /// Toggle view mode (Board <-> TxLog)
    pub fn toggle_view(&mut self) {
        self.view_mode.toggle();
    }
    
    /// Move selection up
    pub fn select_prev(&mut self) {
        // Navigation order per bot: PauseToggle -> Signer -> AuthPda -> ConfigReload -> SessionRefresh
        // Then tx logs, then wrap to last bot's SessionRefresh
        self.selected = match &self.selected {
            None => {
                if !self.bots.is_empty() {
                    Some(SelectableElement::BotPauseToggle(0))
                } else if !self.tx_log.is_empty() {
                    Some(SelectableElement::TxLog(0))
                } else {
                    None
                }
            }
            Some(SelectableElement::BotPauseToggle(i)) => {
                if *i > 0 {
                    // Go to previous bot's SessionRefresh
                    Some(SelectableElement::BotSessionRefresh(i - 1))
                } else {
                    // Wrap to tx log if available
                    if !self.tx_log.is_empty() {
                        Some(SelectableElement::TxLog(self.tx_log.len().min(30) - 1))
                    } else if !self.bots.is_empty() {
                        Some(SelectableElement::BotSessionRefresh(self.bots.len() - 1))
                    } else {
                        None
                    }
                }
            }
            Some(SelectableElement::BotSigner(i)) => {
                Some(SelectableElement::BotPauseToggle(*i))
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                Some(SelectableElement::BotSigner(*i))
            }
            Some(SelectableElement::BotConfigReload(i)) => {
                Some(SelectableElement::BotAuthPda(*i))
            }
            Some(SelectableElement::BotSessionRefresh(i)) => {
                Some(SelectableElement::BotConfigReload(*i))
            }
            Some(SelectableElement::TxLog(i)) => {
                if *i > 0 {
                    Some(SelectableElement::TxLog(i - 1))
                } else {
                    // Wrap to last bot's SessionRefresh
                    if !self.bots.is_empty() {
                        Some(SelectableElement::BotSessionRefresh(self.bots.len() - 1))
                    } else {
                        Some(SelectableElement::TxLog(0))
                    }
                }
            }
        };
    }
    
    /// Move selection down
    pub fn select_next(&mut self) {
        // Navigation order per bot: PauseToggle -> Signer -> AuthPda -> ConfigReload -> SessionRefresh
        // Then next bot or tx logs
        self.selected = match &self.selected {
            None => {
                if !self.bots.is_empty() {
                    Some(SelectableElement::BotPauseToggle(0))
                } else if !self.tx_log.is_empty() {
                    Some(SelectableElement::TxLog(0))
                } else {
                    None
                }
            }
            Some(SelectableElement::BotPauseToggle(i)) => {
                Some(SelectableElement::BotSigner(*i))
            }
            Some(SelectableElement::BotSigner(i)) => {
                Some(SelectableElement::BotAuthPda(*i))
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                Some(SelectableElement::BotConfigReload(*i))
            }
            Some(SelectableElement::BotConfigReload(i)) => {
                Some(SelectableElement::BotSessionRefresh(*i))
            }
            Some(SelectableElement::BotSessionRefresh(i)) => {
                if *i + 1 < self.bots.len() {
                    Some(SelectableElement::BotPauseToggle(i + 1))
                } else {
                    // Move to tx log
                    if !self.tx_log.is_empty() {
                        Some(SelectableElement::TxLog(0))
                    } else {
                        Some(SelectableElement::BotPauseToggle(0))
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
                        Some(SelectableElement::BotPauseToggle(0))
                    } else {
                        Some(SelectableElement::TxLog(0))
                    }
                }
            }
        };
    }
    
    /// Get the action for current selection when Enter is pressed
    pub fn get_select_action(&self) -> SelectAction {
        match &self.selected {
            Some(SelectableElement::BotPauseToggle(i)) => {
                SelectAction::TogglePause(*i)
            }
            Some(SelectableElement::BotSigner(i)) => {
                self.bots.get(*i).map(|b| SelectAction::Copy(b.signer.to_string()))
                    .unwrap_or(SelectAction::None)
            }
            Some(SelectableElement::BotAuthPda(i)) => {
                self.bots.get(*i).map(|b| SelectAction::Copy(b.managed_miner_auth.to_string()))
                    .unwrap_or(SelectAction::None)
            }
            Some(SelectableElement::BotConfigReload(i)) => {
                SelectAction::ReloadConfig(*i)
            }
            Some(SelectableElement::BotSessionRefresh(i)) => {
                SelectAction::RefreshSession(*i)
            }
            Some(SelectableElement::TxLog(i)) => {
                self.tx_log.iter().rev().nth(*i)
                    .map(|e| SelectAction::Copy(e.signature.to_string()))
                    .unwrap_or(SelectAction::None)
            }
            None => SelectAction::None,
        }
    }
    
    /// Execute action and show status message
    pub fn execute_select_action(&mut self) {
        match self.get_select_action() {
            SelectAction::Copy(value) => {
                match cli_clipboard::set_contents(value) {
                    Ok(_) => {
                        self.status_msg = Some(("Copied!".to_string(), Instant::now(), false));
                    }
                    Err(_) => {
                        self.status_msg = Some(("Copy failed".to_string(), Instant::now(), true));
                    }
                }
            }
            SelectAction::ReloadConfig(bot_idx) => {
                // Config reload handled externally - just signal it
                self.status_msg = Some((format!("Reloading config for bot {}...", bot_idx), Instant::now(), false));
            }
            SelectAction::RefreshSession(bot_idx) => {
                // Reset session stats for this bot by setting offsets
                // This way, incoming BotStatsUpdate won't overwrite the reset
                if let Some(bot) = self.bots.get_mut(bot_idx) {
                    // Set offsets to current values - effective values will become 0
                    bot.session_stats.rounds_participated_offset = bot.session_stats.rounds_participated;
                    bot.session_stats.rounds_won_offset = bot.session_stats.rounds_won;
                    bot.session_stats.rounds_skipped_offset = bot.session_stats.rounds_skipped;
                    bot.session_stats.rounds_missed_offset = bot.session_stats.rounds_missed;
                    
                    // Reset P&L tracking
                    bot.session_stats.started_at = Instant::now();
                    bot.session_stats.starting_claimable_sol = bot.session_stats.current_claimable_sol;
                    bot.session_stats.starting_ore = bot.session_stats.current_ore;
                    bot.session_stats.starting_signer_balance = bot.signer_balance;
                    bot.session_stats.total_claimed_sol = 0;
                    bot.session_stats.total_claimed_ore = 0;
                    
                    self.status_msg = Some((format!("Session reset: {}", bot.name), Instant::now(), false));
                }
            }
            SelectAction::TogglePause(_) => {
                // Toggle pause is handled externally via InputResult - this shouldn't be called
                // but we handle it gracefully
            }
            SelectAction::None => {}
        }
    }
    
    /// Set config path for hot reload
    pub fn set_config_path(&mut self, path: String) {
        self.config_path = Some(path);
    }
    
    /// Set status message
    pub fn set_status(&mut self, msg: String, is_error: bool) {
        self.status_msg = Some((msg, Instant::now(), is_error));
    }
    
    /// Add a bot to the dashboard
    pub fn add_bot(&mut self, bot: BotState) {
        self.bots.push(bot);
    }
    
    /// Get bot icon by name (for tx log display)
    pub fn get_bot_icon(&self, bot_name: &str) -> &'static str {
        self.bots.iter()
            .find(|b| b.name == bot_name)
            .map(|b| b.icon)
            .unwrap_or("🤖")
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
    /// Also updates tx counters for network stats
    /// 
    /// Counter logic:
    /// - Sent: all transactions submitted (counted on Sent, or on Confirmed/Failed for checkpoint/claim)
    /// - Confirmed (OK): successful transactions
    /// - Failed (FAIL): actual errors like NoDeployment, AlreadyDeployed, on-chain errors
    /// - Missed: expired/dropped transactions OR RPC errors
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
        // Update tx counters based on status and error type
        match &status {
            TxStatus::Sent => {
                self.network_stats.txs_sent += 1;
            }
            TxStatus::Confirmed => {
                self.network_stats.txs_confirmed += 1;
                // For checkpoint/claim, we don't log Sent first, so count here
                if matches!(tx_type, TxType::Checkpoint | TxType::ClaimSol | TxType::ClaimOre) {
                    self.network_stats.txs_sent += 1;
                }
            }
            TxStatus::Failed => {
                // For checkpoint/claim, we don't log Sent first, so count here
                if matches!(tx_type, TxType::Checkpoint | TxType::ClaimSol | TxType::ClaimOre) {
                    self.network_stats.txs_sent += 1;
                }
                
                // Distinguish between actual errors (failed) and network issues (missed)
                // Missed = expired/dropped OR RPC errors
                // Failed = actual on-chain errors or expected skip errors
                let is_missed = error.as_ref().map_or(false, |e| {
                    e.contains("expired") || 
                    e.contains("dropped") || 
                    e.contains("RPC") ||
                    e.contains("timeout") ||
                    e.contains("connection")
                });
                
                if is_missed {
                    self.network_stats.txs_missed += 1;
                } else {
                    self.network_stats.txs_failed += 1;
                }
            }
        }
        
        self.tx_log.push(TxLogEntry {
            timestamp: Instant::now(),
            bot_name,
            tx_type,
            status: status.clone(),
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
                    // Update deployed_per_square from full miner update
                    bot.deployed_per_square = miner.deployed;
                    bot.miner_round_id = miner.round_id;
                    bot.miner = Some(miner);
                }
            }
            TuiUpdate::MinerDataUpdate { bot_index, deployed, round_id } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.deployed_per_square = deployed;
                    bot.miner_round_id = round_id;
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
            TuiUpdate::NetworkStatsUpdate {
                slot_ws,
                board_ws,
                round_ws,
                rpc,
                sender_east_latency_ms,
                sender_west_latency_ms,
                rpc_rps,
                sender_rps,
            } => {
                if let Some(s) = slot_ws {
                    self.network_stats.slot_ws = s;
                }
                if let Some(s) = board_ws {
                    self.network_stats.board_ws = s;
                }
                if let Some(s) = round_ws {
                    self.network_stats.round_ws = s;
                }
                if let Some(s) = rpc {
                    self.network_stats.rpc = s;
                }
                if let Some(ms) = sender_east_latency_ms {
                    self.network_stats.sender_east_latency_ms = Some(ms);
                }
                if let Some(ms) = sender_west_latency_ms {
                    self.network_stats.sender_west_latency_ms = Some(ms);
                }
                if let Some(r) = rpc_rps {
                    self.network_stats.rpc_rps = r;
                }
                if let Some(r) = sender_rps {
                    self.network_stats.sender_rps = r;
                }
            }
            TuiUpdate::TxCounterUpdate { sent, confirmed, failed } => {
                if let Some(n) = sent {
                    self.network_stats.txs_sent = n;
                }
                if let Some(n) = confirmed {
                    self.network_stats.txs_confirmed = n;
                }
                if let Some(n) = failed {
                    self.network_stats.txs_failed = n;
                }
            }
            TuiUpdate::TreasuryUpdate(data) => {
                self.treasury = Some(data);
            }
            TuiUpdate::BotPauseUpdate { bot_index, is_paused } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.is_paused = is_paused;
                    if is_paused {
                        bot.status = BotStatus::Paused;
                    }
                }
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
    // Main layout: Header, Bot Blocks, Content (TxLog OR Board), Footer
    // Tab key toggles between TxLog and Board views
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Length(17),  // Bot block(s) - with config info
            Constraint::Min(8),      // Content area (TxLog or Board) - flexible
            Constraint::Length(3),   // Footer with network stats
        ])
        .split(frame.area());
    
    draw_header(frame, chunks[0], app);
    draw_bot_blocks(frame, chunks[1], app);
    
    // Draw either TxLog or Board based on view mode
    match app.view_mode {
        ViewMode::TxLog => draw_tx_log(frame, chunks[2], app),
        ViewMode::Board => draw_board_grid_expanded(frame, chunks[2], app),
    }
    
    draw_footer(frame, chunks[3], app);
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
    
    // Format treasury data
    let treasury_str = app.treasury.as_ref().map(|t| {
        let balance_sol = t.balance as f64 / 1e9;
        let motherlode_ore = t.motherlode as f64 / 1e11; // ORE has 11 decimals
        format!("│ Treasury: {:.2}◎ | ML: {:.0} ORE ", balance_sol, motherlode_ore)
    }).unwrap_or_default();
    
    // Format total deployed from round data
    let total_deployed_str = app.round.as_ref().map(|r| {
        let total: u64 = r.deployed.iter().sum();
        let total_sol = total as f64 / 1e9;
        format!("│ Deployed: {:.4}◎ ", total_sol)
    }).unwrap_or_default();
    
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
        Span::styled(total_deployed_str, Style::default().fg(Color::Green)),
        Span::styled(treasury_str, Style::default().fg(Color::Rgb(255, 165, 0))),
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
        // Status message
        if let Some((msg, _, is_error)) = &app.status_msg {
            let color = if *is_error { Color::Red } else { Color::Green };
            Span::styled(format!("  [{}]", msg), Style::default().fg(color).bold())
        } else {
            Span::styled("", Style::default())
        },
        // Help text
        Span::styled("  ↑↓:nav Tab:view Enter:act P:pause q:quit", Style::default().fg(Color::DarkGray)),
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
    
    let title = format!(" {} {} ({}) ", bot.icon, bot.name, bot.auth_id);
    
    // Check selection state for highlighting
    let pause_selected = app.selected == Some(SelectableElement::BotPauseToggle(bot_index));
    let signer_selected = app.selected == Some(SelectableElement::BotSigner(bot_index));
    let auth_selected = app.selected == Some(SelectableElement::BotAuthPda(bot_index));
    let config_selected = app.selected == Some(SelectableElement::BotConfigReload(bot_index));
    let session_selected = app.selected == Some(SelectableElement::BotSessionRefresh(bot_index));
    
    // Pause/play icon
    let pause_icon = if bot.is_paused { "▶️" } else { "⏸️" };
    let pause_label = if bot.is_paused { " Play" } else { " Pause" };
    
    // Build lines with visual sections
    let mut lines = vec![
        // ═══ PAUSE/PLAY CONTROL ═══
        Line::from(vec![
            if pause_selected { Span::styled("► ", Style::default().fg(Color::White).bold()) } 
            else { Span::styled("  ", Style::default()) },
            Span::styled(
                format!("{}{}", pause_icon, pause_label),
                if pause_selected { Style::default().fg(Color::White).bold().on_blue() }
                else if bot.is_paused { Style::default().fg(Color::Yellow) }
                else { Style::default().fg(Color::DarkGray) }
            ),
            Span::styled("  ", Style::default()),
            // Show status after pause control
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
        // ═══ ACTIONS (selectable) ═══
        Line::from(vec![
            if config_selected { Span::styled("► ", Style::default().fg(Color::White).bold()) } 
            else { Span::styled("  ", Style::default()) },
            Span::styled(
                "🔄 Reload Config", 
                if config_selected { Style::default().fg(Color::White).bold().on_blue() } 
                else { Style::default().fg(Color::DarkGray) }
            ),
            Span::styled("  ", Style::default()),
            if session_selected { Span::styled("► ", Style::default().fg(Color::White).bold()) } 
            else { Span::styled("", Style::default()) },
            Span::styled(
                "🔁 Reset Session", 
                if session_selected { Style::default().fg(Color::White).bold().on_blue() } 
                else { Style::default().fg(Color::DarkGray) }
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
        // Fees line
        Line::from(vec![
            Span::styled("◈ Fees      ", Style::default().fg(Color::DarkGray)),
            Span::styled("prio=", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{}", bot.priority_fee), Style::default().fg(Color::White)),
            Span::styled(" tip=", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4}◎", bot.jito_tip as f64 / 1e9), Style::default().fg(Color::Cyan)),
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

    // Strategy-specific round stats (using effective values to handle session reset)
    let participated = stats.effective_rounds_participated();
    let won = stats.effective_rounds_won();
    let skipped = stats.effective_rounds_skipped();
    let missed = stats.effective_rounds_missed();
    
    match bot.strategy.as_str() {
        "EV" => {
            let total_rounds = participated + skipped + missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total_rounds), Style::default().fg(Color::Cyan)),
                Span::styled(" Deployed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", participated), Style::default().fg(Color::Yellow)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Skip ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<6}", skipped), Style::default().fg(Color::DarkGray)),
                Span::styled(" Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", missed), Style::default().fg(if missed > 0 { Color::Red } else { Color::DarkGray })),
            ]));
        }
        "Percentage" => {
            let total = participated + missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total), Style::default().fg(Color::Cyan)),
                Span::styled(" Deployed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", participated), Style::default().fg(Color::Yellow)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", missed), Style::default().fg(if missed > 0 { Color::Red } else { Color::DarkGray })),
            ]));
        }
        _ => {
            let total = participated + missed;
            lines.push(Line::from(vec![
                Span::styled("Rounds ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{:<4}", total), Style::default().fg(Color::Cyan)),
                Span::styled(" Wins ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ({:.0}%)", won, stats.win_rate()), Style::default().fg(Color::Green)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Missed ", Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{}", missed), Style::default().fg(if missed > 0 { Color::Red } else { Color::DarkGray })),
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

/// Draw expanded board grid (when in Board view mode)
/// Shows total deployed, EV per square, per-bot deployment breakdown, and EV totals
fn draw_board_grid_expanded(frame: &mut Frame, area: Rect, app: &App) {
    use crate::ev_calculator::calculate_board_ev;
    
    let view_indicator = format!(" Board (5×5) + SOL EV [Tab: {}] ", app.view_mode.as_str());
    let block = Block::default()
        .title(view_indicator)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    // Get current board for round comparison
    let current_round_id = app.board.as_ref().map(|b| b.round_id).unwrap_or(0);
    
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
    
    // Calculate EV for all squares
    let board_ev = calculate_board_ev(&round.deployed);
    
    // Calculate max deployment for color scaling
    let max_deploy = round.deployed.iter().max().copied().unwrap_or(1).max(1);
    let total_deployed: u64 = round.deployed.iter().sum();
    
    // Collect per-bot deployment info for this round
    // Only include bots whose miner_round_id matches current round
    let bot_deployments: Vec<(&BotState, bool)> = app.bots.iter()
        .map(|bot| (bot, bot.miner_round_id == current_round_id))
        .collect();
    
    // Build table rows for 5x5 grid with EV and per-bot info
    let mut rows = Vec::new();
    
    for row_idx in 0..5 {
        let mut cells = Vec::new();
        
        for col_idx in 0..5 {
            let idx = row_idx * 5 + col_idx;
            let deployed = round.deployed[idx];
            let sol = deployed as f64 / 1e9;
            let sq_ev = &board_ev.squares[idx];
            
            // Build cell content: total + EV + bot breakdown
            let mut cell_parts: Vec<Span> = Vec::new();
            
            // Total amount
            let total_str = if sol >= 1.0 {
                format!("{:>2}:{:.2}", idx, sol)
            } else if sol >= 0.001 {
                format!("{:>2}:{:.3}", idx, sol)
            } else if deployed > 0 {
                format!("{:>2}:{}", idx, deployed)
            } else {
                format!("{:>2}: ·", idx)
            };
            
            // Color intensity based on deployment
            let intensity = ((deployed as f64 / max_deploy as f64) * 200.0) as u8;
            let fg_color = if deployed > 0 {
                Color::Rgb(200 + intensity / 4, 200 + intensity / 4, 255)
            } else {
                Color::DarkGray
            };
            
            cell_parts.push(Span::styled(format!("{} ", total_str), Style::default().fg(fg_color)));
            
            // EV indicator (compact)
            if sq_ev.is_positive && sq_ev.expected_profit > 0 {
                let ev_sol = sq_ev.expected_profit as f64 / 1e9;
                let ev_str = if ev_sol >= 0.01 {
                    format!("+{:.2}", ev_sol)
                } else if ev_sol >= 0.001 {
                    format!("+{:.3}", ev_sol)
                } else {
                    format!("+{:.4}", ev_sol)
                };
                cell_parts.push(Span::styled(ev_str, Style::default().fg(Color::Green)));
            } else if deployed > 0 {
                cell_parts.push(Span::styled("-EV", Style::default().fg(Color::Red)));
            }
            
            // Add per-bot breakdown (compact) - show if any bot deployed
            let has_bot_deployment = bot_deployments.iter()
                .any(|(bot, is_current)| *is_current && bot.deployed_per_square[idx] > 0);
            
            if has_bot_deployment {
                cell_parts.push(Span::styled(" ", Style::default()));
                for (bot, is_current_round) in &bot_deployments {
                    if *is_current_round && bot.deployed_per_square[idx] > 0 {
                        cell_parts.push(Span::styled(bot.icon, Style::default().fg(Color::Yellow)));
                    }
                }
            }
            
            cells.push(Line::from(cell_parts));
        }
        
        // Convert Lines to Row
        let row_cells: Vec<ratatui::widgets::Cell> = cells.into_iter()
            .map(|line| ratatui::widgets::Cell::from(line))
            .collect();
        rows.push(Row::new(row_cells).height(1));
    }
    
    // Add EV summary row
    let ev_total_sol = board_ev.total_expected_profit as f64 / 1e9;
    let ev_stake_sol = board_ev.total_optimal_stake as f64 / 1e9;
    
    let ev_summary_spans: Vec<Span> = vec![
        Span::styled(" EV Summary: ", Style::default().fg(Color::White).bold()),
        Span::styled(format!("+{} sq ", board_ev.positive_ev_count), Style::default().fg(Color::Green)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Stake: {:.4}◎ ", ev_stake_sol), Style::default().fg(Color::Cyan)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Profit: {:.4}◎", ev_total_sol), 
            if ev_total_sol > 0.0 { Style::default().fg(Color::Green).bold() } 
            else { Style::default().fg(Color::Red) }),
    ];
    rows.push(Row::new(vec![ratatui::widgets::Cell::from(Line::from(ev_summary_spans))]).height(1));
    
    // Add deployed summary row
    let mut summary_spans: Vec<Span> = vec![
        Span::styled(
            format!(" Deployed: {:.4}◎ | ML: {} ORE ", 
                total_deployed as f64 / 1e9, 
                round.motherlode
            ),
            Style::default().fg(Color::Cyan)
        ),
    ];
    
    // Add bot legend if any deployed
    for (bot, is_current_round) in &bot_deployments {
        if *is_current_round {
            let bot_total: u64 = bot.deployed_per_square.iter().sum();
            if bot_total > 0 {
                summary_spans.push(Span::styled(
                    format!("│ {} {:.4}◎ ", bot.icon, bot_total as f64 / 1e9),
                    Style::default().fg(Color::Yellow)
                ));
            }
        }
    }
    
    let summary = Row::new(vec![ratatui::widgets::Cell::from(Line::from(summary_spans))]).height(1);
    rows.push(summary);
    
    let widths = [
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
        Constraint::Ratio(1, 5),
    ];
    
    let table = Table::new(rows, widths)
        .column_spacing(0);
    
    frame.render_widget(table, inner);
}

// =============================================================================
// Footer Section (Network Stats)
// =============================================================================

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let stats = &app.network_stats;

    // Build connection status indicators
    let ws_status = Line::from(vec![
        Span::styled(" WS:", Style::default().fg(Color::DarkGray)),
        Span::styled("s", Style::default().fg(Color::DarkGray)),
        Span::styled(stats.slot_ws.as_str(), Style::default().fg(stats.slot_ws.color())),
        Span::styled("b", Style::default().fg(Color::DarkGray)),
        Span::styled(stats.board_ws.as_str(), Style::default().fg(stats.board_ws.color())),
        Span::styled("r", Style::default().fg(Color::DarkGray)),
        Span::styled(stats.round_ws.as_str(), Style::default().fg(stats.round_ws.color())),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled("RPC:", Style::default().fg(Color::DarkGray)),
        Span::styled(stats.rpc.as_str(), Style::default().fg(stats.rpc.color())),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        // Sender latencies
        Span::styled("E:", Style::default().fg(Color::Cyan)),
        Span::styled(
            stats.sender_east_latency_ms.map_or("--".to_string(), |ms| format!("{}ms", ms)),
            Style::default().fg(if stats.sender_east_latency_ms.map_or(false, |ms| ms < 200) { Color::Green } else { Color::Yellow })
        ),
        Span::styled(" W:", Style::default().fg(Color::Magenta)),
        Span::styled(
            stats.sender_west_latency_ms.map_or("--".to_string(), |ms| format!("{}ms", ms)),
            Style::default().fg(if stats.sender_west_latency_ms.map_or(false, |ms| ms < 200) { Color::Green } else { Color::Yellow })
        ),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        // RPC: rps (total)
        Span::styled("RPC:", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}/s", stats.rpc_rps), Style::default().fg(Color::White)),
        Span::styled(format!("({})", stats.rpc_total), Style::default().fg(Color::DarkGray)),
        Span::styled(" ", Style::default().fg(Color::DarkGray)),
        // Send: rps (total)
        Span::styled("Send:", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}/s", stats.sender_rps), Style::default().fg(Color::Cyan)),
        Span::styled(format!("({})", stats.sender_total), Style::default().fg(Color::DarkGray)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        // Tx stats: OK/FAIL/MISS out of SENT (green/orange/red)
        Span::styled("Tx:", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", stats.txs_confirmed), Style::default().fg(Color::Green)),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", stats.txs_failed), Style::default().fg(Color::Rgb(255, 165, 0))), // Orange
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", stats.txs_missed), Style::default().fg(Color::Red)),
        Span::styled("/", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{}", stats.txs_sent), Style::default().fg(Color::White)),
        Span::styled(" (", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{:.0}%", stats.miss_rate()),
            Style::default().fg(if stats.miss_rate() < 20.0 { Color::Green } else { Color::Red })
        ),
        Span::styled(")", Style::default().fg(Color::DarkGray)),
        // View mode indicator
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("[Tab:{}]", app.view_mode.as_str()), Style::default().fg(Color::Cyan)),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .style(Style::default().bg(Color::Rgb(10, 10, 15)));

    let paragraph = Paragraph::new(ws_status).block(block);
    frame.render_widget(paragraph, area);
}

// =============================================================================
// Transaction Log Section
// =============================================================================

fn draw_tx_log(frame: &mut Frame, area: Rect, app: &App) {
    let view_indicator = format!(" Transaction Log [Tab: {}] ", app.view_mode.as_str());
    let block = Block::default()
        .title(view_indicator)
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
            
            // Get bot icon for this entry
            let bot_icon = app.get_bot_icon(&entry.bot_name);
            
            let mut spans = vec![
                Span::styled(select_indicator, Style::default().fg(Color::White).bold()),
                Span::styled(format!("[{}] ", time_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", bot_icon), Style::default()),
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

/// Input result from handle_input
#[derive(Clone, Debug)]
pub enum InputResult {
    Continue,
    Quit,
    ReloadConfig(usize),  // Bot index to reload config for
    TogglePause(usize),   // Bot index to toggle pause for
}

/// Handle keyboard input
/// Returns InputResult indicating what action to take
pub fn handle_input(app: &mut App) -> io::Result<InputResult> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.running = false;
                        return Ok(InputResult::Quit);
                    }
                    // Arrow key navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.select_next();
                    }
                    // Enter to execute action
                    KeyCode::Enter => {
                        // Check if it's a config reload action
                        if let Some(SelectableElement::BotConfigReload(bot_idx)) = &app.selected {
                            let idx = *bot_idx;
                            app.status_msg = Some((format!("Reloading config..."), Instant::now(), false));
                            return Ok(InputResult::ReloadConfig(idx));
                        }
                        // Check if it's a pause toggle action
                        if let Some(SelectableElement::BotPauseToggle(bot_idx)) = &app.selected {
                            let idx = *bot_idx;
                            return Ok(InputResult::TogglePause(idx));
                        }
                        // Otherwise execute normally (copy or session refresh)
                        app.execute_select_action();
                    }
                    // P to toggle pause for selected bot (if any bot element is selected)
                    KeyCode::Char('p') | KeyCode::Char('P') => {
                        // Get bot index from current selection
                        let bot_idx = match &app.selected {
                            Some(SelectableElement::BotPauseToggle(i)) |
                            Some(SelectableElement::BotSigner(i)) |
                            Some(SelectableElement::BotAuthPda(i)) |
                            Some(SelectableElement::BotConfigReload(i)) |
                            Some(SelectableElement::BotSessionRefresh(i)) => Some(*i),
                            _ => None,
                        };
                        if let Some(idx) = bot_idx {
                            return Ok(InputResult::TogglePause(idx));
                        }
                    }
                    // Tab to toggle view mode
                    KeyCode::Tab => {
                        app.toggle_view();
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

    // Clear status message after 2 seconds
    if let Some((_, instant, _)) = &app.status_msg {
        if instant.elapsed() > Duration::from_secs(2) {
            app.status_msg = None;
        }
    }

    Ok(InputResult::Continue)
}
