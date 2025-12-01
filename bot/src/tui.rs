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
        current_claimable_sol: u64,
        current_ore: u64,
    },
    
    /// Bot signer (fee payer) balance updated
    BotSignerBalanceUpdate { bot_index: usize, balance: u64 },
    
    /// Transaction event (for tx log)
    TxEvent {
        bot_name: String,
        action: TxAction,
        signature: Signature,
        error: Option<String>,
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
    /// Starting claimable SOL (rewards_sol at session start)
    pub starting_claimable_sol: u64,
    /// Current claimable SOL (updated after each checkpoint)
    pub current_claimable_sol: u64,
    /// Total ORE earned this session
    pub ore_earned: u64,
    /// Starting ORE balance for delta tracking
    pub starting_ore: u64,
    /// Current ORE balance
    pub current_ore: u64,
}

impl Default for BotSessionStats {
    fn default() -> Self {
        Self {
            started_at: Instant::now(),
            rounds_participated: 0,
            rounds_won: 0,
            starting_claimable_sol: 0,
            current_claimable_sol: 0,
            ore_earned: 0,
            starting_ore: 0,
            current_ore: 0,
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
            starting_claimable_sol: claimable_sol,
            current_claimable_sol: claimable_sol,
            ore_earned: 0,
            starting_ore: ore,
            current_ore: ore,
        }
    }
    
    /// Calculate SOL P&L (can be negative)
    pub fn sol_pnl(&self) -> i64 {
        self.current_claimable_sol as i64 - self.starting_claimable_sol as i64
    }
    
    /// Calculate ORE earned
    pub fn ore_pnl(&self) -> i64 {
        self.current_ore as i64 - self.starting_ore as i64
    }
}

impl BotSessionStats {
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

/// Transaction log entry
#[derive(Clone, Debug)]
pub struct TxLogEntry {
    pub timestamp: Instant,
    pub bot_name: String,
    pub action: TxAction,
    pub signature: Signature,
    pub error: Option<String>,
}

#[derive(Clone, Debug)]
pub enum TxAction {
    Sent,
    Confirmed,
    Failed,
}

impl TxAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            TxAction::Sent => "SENT",
            TxAction::Confirmed => "OK",
            TxAction::Failed => "FAIL",
        }
    }
    
    pub fn color(&self) -> Color {
        match self {
            TxAction::Sent => Color::Cyan,
            TxAction::Confirmed => Color::Green,
            TxAction::Failed => Color::Red,
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
    /// Signer (fee payer) SOL balance (lamports)
    pub signer_balance: u64,
    // EV strategy params
    pub max_per_square: u64,
    pub min_bet: u64,
    pub ore_value: u64,
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
        max_per_square: u64,
        min_bet: u64,
        ore_value: u64,
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
            signer_balance: 0,
            max_per_square,
            min_bet,
            ore_value,
        }
    }
    
    pub fn rewards_sol(&self) -> u64 {
        self.miner.as_ref().map(|m| m.rewards_sol).unwrap_or(0)
    }
    
    pub fn rewards_ore(&self) -> u64 {
        self.miner.as_ref().map(|m| m.rewards_ore).unwrap_or(0)
    }
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
    
    /// Log a transaction
    pub fn log_tx(&mut self, bot_name: String, action: TxAction, signature: Signature, error: Option<String>) {
        self.tx_log.push(TxLogEntry {
            timestamp: Instant::now(),
            bot_name,
            action,
            signature,
            error,
        });
        
        // Keep last 100 entries
        if self.tx_log.len() > 100 {
            self.tx_log.remove(0);
        }
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
                current_claimable_sol,
                current_ore,
            } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    // If this is the first update (starting values are 0), initialize starting values
                    if bot.session_stats.starting_claimable_sol == 0 && bot.session_stats.starting_ore == 0 {
                        bot.session_stats.starting_claimable_sol = current_claimable_sol;
                        bot.session_stats.starting_ore = current_ore;
                    }
                    bot.session_stats.rounds_participated = rounds_participated;
                    bot.session_stats.rounds_won = rounds_won;
                    bot.session_stats.current_claimable_sol = current_claimable_sol;
                    bot.session_stats.current_ore = current_ore;
                }
            }
            TuiUpdate::BotSignerBalanceUpdate { bot_index, balance } => {
                if let Some(bot) = self.bots.get_mut(bot_index) {
                    bot.signer_balance = balance;
                }
            }
            TuiUpdate::TxEvent { bot_name, action, signature, error } => {
                self.log_tx(bot_name, action, signature, error);
            }
            TuiUpdate::Error(msg) => {
                // Log error as a failed tx entry for now
                self.log_tx("system".to_string(), TxAction::Failed, Signature::default(), Some(msg));
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
    // Main layout: Header, Bot Blocks, Board, Transaction Log
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Length(10),  // Bot block(s)
            Constraint::Min(12),     // Board grid
            Constraint::Length(8),   // Transaction log
        ])
        .split(frame.area());
    
    draw_header(frame, chunks[0], app);
    draw_bot_blocks(frame, chunks[1], app);
    draw_board_grid(frame, chunks[2], app);
    draw_tx_log(frame, chunks[3], app);
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
        draw_single_bot_block(frame, bot_areas[i], bot, app);
    }
}

fn draw_single_bot_block(frame: &mut Frame, area: Rect, bot: &BotState, app: &App) {
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
    let rewards_sol = bot.rewards_sol() as f64 / 1e9;
    let rewards_ore = bot.rewards_ore() as f64 / 1e11; // ORE has 11 decimals
    let signer_sol = bot.signer_balance as f64 / 1e9;
    
    // Session stats with P&L
    let stats = &bot.session_stats;
    let sol_pnl = stats.sol_pnl() as f64 / 1e9;
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
    
    let title = format!(" {} {} (auth_id={}) ", bot.icon, bot.name, bot.auth_id);
    
    let lines = vec![
        Line::from(vec![
            Span::styled("Strategy: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&bot.strategy, Style::default().fg(Color::White)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Bankroll: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} SOL", bankroll_sol), Style::default().fg(Color::Cyan)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Signer: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} SOL", signer_sol), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(status_str, Style::default().fg(bot.status.color()).bold()),
        ]),
        Line::from(vec![
            Span::styled("This Round: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} SOL", deployed_sol), Style::default().fg(Color::Yellow)),
            Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
            Span::styled("Claimable: ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.4} SOL", rewards_sol), Style::default().fg(Color::Green)),
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            Span::styled(format!("{:.2} ORE", rewards_ore), Style::default().fg(Color::Rgb(255, 165, 0))),
        ]),
        Line::from(vec![
            Span::styled("───── Session Stats ─────", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(vec![
            Span::styled(format!("{}  ", stats.running_time_str()), Style::default().fg(Color::White)),
            Span::styled(format!("Rounds: {}  ", stats.rounds_participated), Style::default().fg(Color::Cyan)),
            Span::styled(format!("Wins: {} ({:.0}%)  ", stats.rounds_won, stats.win_rate()), Style::default().fg(Color::Green)),
            Span::styled(format!("{} SOL  ", sol_pnl_str), Style::default().fg(sol_pnl_color)),
            Span::styled(format!("{} ORE", ore_pnl_str), Style::default().fg(ore_pnl_color)),
        ]),
    ];
    
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
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
        
        rows.push(Row::new(cells).height(2));
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
        .take(6)
        .map(|entry| {
            let elapsed = entry.timestamp.elapsed().as_secs();
            let time_str = if elapsed < 60 {
                format!("{:>3}s", elapsed)
            } else {
                format!("{:>3}m", elapsed / 60)
            };
            
            let sig_str = &entry.signature.to_string()[..8];
            
            let mut spans = vec![
                Span::styled(format!("[{}] ", time_str), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("{} ", entry.bot_name), Style::default().fg(Color::Cyan)),
                Span::styled(format!("{:<4} ", entry.action.as_str()), Style::default().fg(entry.action.color())),
                Span::styled(format!("{}...", sig_str), Style::default().fg(Color::White)),
            ];
            
            if let Some(error) = &entry.error {
                spans.push(Span::styled(format!(" {}", error), Style::default().fg(Color::Red)));
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
                    // No deploy trigger - TUI is monitoring only
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}
