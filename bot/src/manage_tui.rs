//! Manage TUI - Terminal UI for miner management
//!
//! Displays discovered miners in a grid layout with selectable actions:
//! - Checkpoint
//! - Claim SOL
//! - Claim ORE

use std::io::{self, Stdout};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

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
    widgets::{Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame, Terminal,
};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    hash::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::Signer,
};

use crate::config::ManageConfig;
use crate::manage::{DiscoveredMiner, DiscoveryResult};

/// Helper to format pubkey as shortened version (7...7)
pub fn shorten_pubkey(pubkey: &Pubkey) -> String {
    let s = pubkey.to_string();
    format!("{}...{}", &s[..7], &s[s.len()-7..])
}

/// Miner action types
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MinerAction {
    Checkpoint,
    ClaimSol,
    ClaimOre,
}

impl MinerAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            MinerAction::Checkpoint => "Checkpoint",
            MinerAction::ClaimSol => "Claim SOL",
            MinerAction::ClaimOre => "Claim ORE",
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            MinerAction::Checkpoint => "✓",
            MinerAction::ClaimSol => "💰",
            MinerAction::ClaimOre => "⛏️",
        }
    }
}

/// Selection state
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    /// Selected miner by index
    Miner(usize),
    /// Selected action on a miner (miner_index, action)
    Action(usize, MinerAction),
}

/// Transaction result for display
#[derive(Clone, Debug)]
pub struct TxResult {
    pub miner_index: usize,
    pub action: MinerAction,
    pub signature: Option<Signature>,
    pub error: Option<String>,
    pub timestamp: Instant,
}

/// Manage TUI state
pub struct ManageApp {
    pub running: bool,
    pub rpc_url: String,
    pub config: ManageConfig,
    pub discovery: DiscoveryResult,
    pub signers: Vec<(Arc<Keypair>, PathBuf)>,
    
    /// All miners (current + legacy) for display
    pub all_miners: Vec<DiscoveredMiner>,
    
    /// Current selection
    pub selection: Option<Selection>,
    
    /// Scroll offset for miner list
    pub scroll_offset: usize,
    
    /// Status message
    pub status_msg: Option<(String, Instant, bool)>,
    
    /// Transaction results log
    pub tx_log: Vec<TxResult>,
    
    /// Flag indicating refresh is in progress
    pub refreshing: bool,
}

impl ManageApp {
    pub fn new(
        rpc_url: &str,
        config: ManageConfig,
        discovery: DiscoveryResult,
        signers: Vec<(Arc<Keypair>, PathBuf)>,
    ) -> Self {
        // Combine current and legacy miners
        let mut all_miners = discovery.miners.clone();
        all_miners.extend(discovery.legacy_miners.clone());
        
        Self {
            running: true,
            rpc_url: rpc_url.to_string(),
            config,
            discovery,
            signers,
            all_miners,
            selection: None,
            scroll_offset: 0,
            status_msg: None,
            tx_log: Vec::new(),
            refreshing: false,
        }
    }
    
    /// Select next item
    pub fn select_next(&mut self) {
        if self.all_miners.is_empty() {
            return;
        }
        
        self.selection = match &self.selection {
            None => Some(Selection::Miner(0)),
            Some(Selection::Miner(i)) => {
                // Move to first action on this miner
                Some(Selection::Action(*i, MinerAction::Checkpoint))
            }
            Some(Selection::Action(i, action)) => {
                let miner = &self.all_miners[*i];
                let next_action = match action {
                    MinerAction::Checkpoint => {
                        // Skip checkpoint for legacy miners
                        if miner.is_legacy {
                            Some(MinerAction::ClaimSol)
                        } else {
                            Some(MinerAction::ClaimSol)
                        }
                    }
                    MinerAction::ClaimSol => Some(MinerAction::ClaimOre),
                    MinerAction::ClaimOre => None, // Move to next miner
                };
                
                match next_action {
                    Some(a) => Some(Selection::Action(*i, a)),
                    None => {
                        // Move to next miner
                        let next_i = (*i + 1) % self.all_miners.len();
                        Some(Selection::Miner(next_i))
                    }
                }
            }
        };
        
        // Ensure selected miner is visible
        if let Some(Selection::Miner(i)) | Some(Selection::Action(i, _)) = &self.selection {
            if *i >= self.scroll_offset + self.visible_miners() {
                self.scroll_offset = i.saturating_sub(self.visible_miners() - 1);
            }
        }
    }
    
    /// Select previous item
    pub fn select_prev(&mut self) {
        if self.all_miners.is_empty() {
            return;
        }
        
        self.selection = match &self.selection {
            None => Some(Selection::Miner(self.all_miners.len() - 1)),
            Some(Selection::Miner(i)) => {
                if *i == 0 {
                    // Wrap to last miner's last action
                    Some(Selection::Action(self.all_miners.len() - 1, MinerAction::ClaimOre))
                } else {
                    // Go to previous miner's last action
                    Some(Selection::Action(i - 1, MinerAction::ClaimOre))
                }
            }
            Some(Selection::Action(i, action)) => {
                let miner = &self.all_miners[*i];
                let prev_action = match action {
                    MinerAction::Checkpoint => None, // Move to miner header
                    MinerAction::ClaimSol => {
                        // Skip checkpoint for legacy miners
                        if miner.is_legacy {
                            None // Move to miner header
                        } else {
                            Some(MinerAction::Checkpoint)
                        }
                    }
                    MinerAction::ClaimOre => Some(MinerAction::ClaimSol),
                };
                
                match prev_action {
                    Some(a) => Some(Selection::Action(*i, a)),
                    None => Some(Selection::Miner(*i)),
                }
            }
        };
        
        // Ensure selected miner is visible
        if let Some(Selection::Miner(i)) | Some(Selection::Action(i, _)) = &self.selection {
            if *i < self.scroll_offset {
                self.scroll_offset = *i;
            }
        }
    }
    
    /// Get current selection for executing action
    pub fn get_selected_action(&self) -> Option<(usize, MinerAction)> {
        match &self.selection {
            Some(Selection::Action(i, action)) => Some((*i, *action)),
            _ => None,
        }
    }
    
    /// Set status message
    pub fn set_status(&mut self, msg: String, is_error: bool) {
        self.status_msg = Some((msg, Instant::now(), is_error));
    }
    
    /// Log transaction result
    pub fn log_tx(&mut self, miner_index: usize, action: MinerAction, signature: Option<Signature>, error: Option<String>) {
        self.tx_log.push(TxResult {
            miner_index,
            action,
            signature,
            error,
            timestamp: Instant::now(),
        });
        
        // Keep last 50 results
        if self.tx_log.len() > 50 {
            self.tx_log.remove(0);
        }
    }
    
    /// Number of miners visible in the list area
    fn visible_miners(&self) -> usize {
        // Rough estimate based on terminal size
        10
    }
    
    /// Scroll down
    pub fn scroll_down(&mut self) {
        if self.scroll_offset + self.visible_miners() < self.all_miners.len() {
            self.scroll_offset += 1;
        }
    }
    
    /// Scroll up
    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }
}

/// Terminal type alias
pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Initialize terminal
pub fn init() -> io::Result<Tui> {
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    enable_raw_mode()?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

/// Restore terminal
pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

/// Draw the manage TUI
pub fn draw(frame: &mut Frame, app: &ManageApp) {
    // Main layout: Header, Miner List, Footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),   // Header
            Constraint::Min(10),     // Miner list
            Constraint::Length(3),   // Footer/help
        ])
        .split(frame.area());
    
    draw_header(frame, chunks[0], app);
    draw_miner_list(frame, chunks[1], app);
    draw_footer(frame, chunks[2], app);
}

/// Draw header with stats
fn draw_header(frame: &mut Frame, area: Rect, app: &ManageApp) {
    let miner_count = app.discovery.miners.len();
    let legacy_count = app.discovery.legacy_miners.len();
    let manager_count = app.discovery.managers.len();
    let signer_count = app.discovery.signers.len();
    
    // Calculate totals
    let total_sol: u64 = app.all_miners.iter().map(|m| m.claimable_sol()).sum();
    let total_ore: u64 = app.all_miners.iter().map(|m| m.claimable_ore()).sum();
    
    let line1 = Line::from(vec![
        Span::styled("  ⛏️  MINER MANAGEMENT ", Style::default().fg(Color::Cyan).bold()),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Signers: {} ", signer_count), Style::default().fg(Color::White)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Managers: {} ", manager_count), Style::default().fg(Color::White)),
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("Miners: {} ", miner_count), Style::default().fg(Color::Green)),
        if legacy_count > 0 {
            Span::styled(format!("+ {} legacy ", legacy_count), Style::default().fg(Color::Yellow))
        } else {
            Span::styled("", Style::default())
        },
    ]);
    
    let line2 = Line::from(vec![
        Span::styled("  Total Claimable: ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.4} ◎", total_sol as f64 / 1e9), Style::default().fg(Color::Yellow)),
        Span::styled(" │ ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{:.2} ORE", total_ore as f64 / 1e11), Style::default().fg(Color::Rgb(255, 165, 0))),
        // Status message
        if let Some((msg, _, is_error)) = &app.status_msg {
            let color = if *is_error { Color::Red } else { Color::Green };
            Span::styled(format!("  │ {}", msg), Style::default().fg(color))
        } else {
            Span::styled("", Style::default())
        },
    ]);
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .style(Style::default().bg(Color::Rgb(15, 15, 25)));
    
    let paragraph = Paragraph::new(vec![line1, line2]).block(block);
    frame.render_widget(paragraph, area);
}

/// Draw miner list
fn draw_miner_list(frame: &mut Frame, area: Rect, app: &ManageApp) {
    let block = Block::default()
        .title(" Miners ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    if app.all_miners.is_empty() {
        let msg = if app.refreshing {
            "Loading miners..."
        } else {
            "No miners found. Check signers_path in config."
        };
        let paragraph = Paragraph::new(msg)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(paragraph, inner);
        return;
    }
    
    // Build list items
    let items: Vec<ListItem> = app.all_miners
        .iter()
        .enumerate()
        .skip(app.scroll_offset)
        .take(inner.height as usize)
        .map(|(i, miner)| {
            let is_miner_selected = app.selection == Some(Selection::Miner(i));
            
            let mut spans = Vec::new();
            
            // Selection indicator
            if is_miner_selected {
                spans.push(Span::styled("► ", Style::default().fg(Color::White).bold()));
            } else {
                spans.push(Span::styled("  ", Style::default()));
            }
            
            // Miner icon and type
            if miner.is_legacy {
                spans.push(Span::styled("📦 LEGACY ", Style::default().fg(Color::Yellow)));
                let prog_short = format!("[{}] ", &miner.program_id.to_string()[..4]);
                spans.push(Span::styled(prog_short, Style::default().fg(Color::DarkGray)));
            } else {
                spans.push(Span::styled("📦 ", Style::default().fg(Color::Cyan)));
            }
            
            // Signer pubkey
            spans.push(Span::styled(
                format!("Signer:{} ", shorten_pubkey(&miner.signer)),
                if is_miner_selected { Style::default().fg(Color::White).bold() } else { Style::default().fg(Color::Gray) }
            ));
            
            // Auth PDA (the evore authority)
            spans.push(Span::styled(
                format!("Auth:{} ", shorten_pubkey(&miner.authority_pda)),
                Style::default().fg(Color::Cyan)
            ));
            
            // Auth PDA SOL balance
            let auth_balance = miner.auth_pda_balance as f64 / 1e9;
            spans.push(Span::styled(
                format!("Bal:{:.4}◎ ", auth_balance),
                if auth_balance > 0.01 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::DarkGray) }
            ));
            
            // Claimable amounts
            let sol = miner.claimable_sol() as f64 / 1e9;
            let ore = miner.claimable_ore() as f64 / 1e11;
            spans.push(Span::styled(
                format!("Claim:{:.4}◎ ", sol),
                if sol > 0.0 { Style::default().fg(Color::Yellow) } else { Style::default().fg(Color::DarkGray) }
            ));
            spans.push(Span::styled(
                format!("{:.2}ORE ", ore),
                if ore > 0.0 { Style::default().fg(Color::Rgb(255, 165, 0)) } else { Style::default().fg(Color::DarkGray) }
            ));
            
            // Checkpoint indicator
            if miner.needs_checkpoint() && !miner.is_legacy {
                spans.push(Span::styled("⚠", Style::default().fg(Color::Yellow)));
            }
            
            // Actions (inline)
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
            
            // Checkpoint action (not for legacy)
            if !miner.is_legacy {
                let is_selected = app.selection == Some(Selection::Action(i, MinerAction::Checkpoint));
                let style = if is_selected {
                    Style::default().fg(Color::White).bold().on_blue()
                } else if miner.needs_checkpoint() {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                spans.push(Span::styled("[✓Chk] ", style));
            }
            
            // Claim SOL action
            let is_sol_selected = app.selection == Some(Selection::Action(i, MinerAction::ClaimSol));
            let sol_style = if is_sol_selected {
                Style::default().fg(Color::White).bold().on_blue()
            } else if miner.claimable_sol() > 0 {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled("[💰SOL] ", sol_style));
            
            // Claim ORE action
            let is_ore_selected = app.selection == Some(Selection::Action(i, MinerAction::ClaimOre));
            let ore_style = if is_ore_selected {
                Style::default().fg(Color::White).bold().on_blue()
            } else if miner.claimable_ore() > 0 {
                Style::default().fg(Color::Rgb(255, 165, 0))
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled("[⛏ORE]", ore_style));
            
            ListItem::new(Line::from(spans))
        })
        .collect();
    
    let list = List::new(items);
    frame.render_widget(list, inner);
    
    // Scrollbar if needed
    if app.all_miners.len() > inner.height as usize {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        let mut scrollbar_state = ScrollbarState::new(app.all_miners.len())
            .position(app.scroll_offset);
        frame.render_stateful_widget(
            scrollbar,
            area.inner(ratatui::layout::Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }
}

/// Draw footer with help
fn draw_footer(frame: &mut Frame, area: Rect, app: &ManageApp) {
    let help_spans = vec![
        Span::styled(" [↑↓/jk] Navigate ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Enter] Execute ", Style::default().fg(Color::Cyan)),
        Span::styled("[R] Refresh ", Style::default().fg(Color::Yellow)),
        Span::styled("[Q] Quit ", Style::default().fg(Color::Red)),
    ];
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    let paragraph = Paragraph::new(Line::from(help_spans)).block(block);
    frame.render_widget(paragraph, area);
}

/// Input handling result
#[derive(Clone, Debug)]
pub enum InputResult {
    Continue,
    Quit,
    ExecuteAction(usize, MinerAction),
    Refresh,
}

/// Handle keyboard input
pub fn handle_input(app: &mut ManageApp) -> io::Result<InputResult> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                        app.running = false;
                        return Ok(InputResult::Quit);
                    }
                    // Navigation
                    KeyCode::Up | KeyCode::Char('k') => {
                        app.select_prev();
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        app.select_next();
                    }
                    KeyCode::PageUp => {
                        for _ in 0..5 {
                            app.scroll_up();
                        }
                    }
                    KeyCode::PageDown => {
                        for _ in 0..5 {
                            app.scroll_down();
                        }
                    }
                    // Execute action
                    KeyCode::Enter => {
                        if let Some((miner_idx, action)) = app.get_selected_action() {
                            return Ok(InputResult::ExecuteAction(miner_idx, action));
                        }
                    }
                    // Refresh
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        return Ok(InputResult::Refresh);
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Clear status message after 3 seconds
    if let Some((_, instant, _)) = &app.status_msg {
        if instant.elapsed() > Duration::from_secs(3) {
            app.status_msg = None;
        }
    }
    
    Ok(InputResult::Continue)
}
