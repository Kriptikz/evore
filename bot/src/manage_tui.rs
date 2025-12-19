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

use cli_clipboard;
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
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signature},
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
    /// Selected signer pubkey for miner at index
    Signer(usize),
    /// Selected auth pubkey for miner at index
    Auth(usize),
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
    
    /// Skip preflight simulation when sending transactions
    pub skip_preflight: bool,
    
    /// Flag indicating an async operation is in progress (non-blocking)
    pub operation_in_progress: bool,
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
            skip_preflight: false,
            operation_in_progress: false,
        }
    }
    
    /// Get the pubkey of the currently selected element (if a pubkey is selected)
    pub fn get_selected_pubkey(&self) -> Option<Pubkey> {
        match &self.selection {
            Some(Selection::Signer(i)) => {
                self.all_miners.get(*i).map(|m| m.signer)
            }
            Some(Selection::Auth(i)) => {
                self.all_miners.get(*i).map(|m| m.authority_pda)
            }
            _ => None,
        }
    }
    
    /// Toggle skip preflight setting
    pub fn toggle_skip_preflight(&mut self) {
        self.skip_preflight = !self.skip_preflight;
        let state = if self.skip_preflight { "ON" } else { "OFF" };
        self.set_status(format!("Skip preflight: {}", state), false);
    }
    
    /// Get miner index from current selection
    fn get_selection_miner_index(&self) -> Option<usize> {
        match &self.selection {
            Some(Selection::Signer(i)) | Some(Selection::Auth(i)) | Some(Selection::Action(i, _)) => Some(*i),
            None => None,
        }
    }
    
    /// Select next item
    /// Navigation order per miner: Signer -> Auth -> Checkpoint (if not legacy) -> ClaimSol -> ClaimOre -> next miner
    pub fn select_next(&mut self) {
        if self.all_miners.is_empty() {
            return;
        }
        
        self.selection = match &self.selection {
            None => Some(Selection::Signer(0)),
            Some(Selection::Signer(i)) => {
                Some(Selection::Auth(*i))
            }
            Some(Selection::Auth(i)) => {
                let miner = &self.all_miners[*i];
                if miner.is_legacy {
                    // Skip checkpoint for legacy miners
                    Some(Selection::Action(*i, MinerAction::ClaimSol))
                } else {
                    Some(Selection::Action(*i, MinerAction::Checkpoint))
                }
            }
            Some(Selection::Action(i, action)) => {
                let miner = &self.all_miners[*i];
                let next_action = match action {
                    MinerAction::Checkpoint => Some(MinerAction::ClaimSol),
                    MinerAction::ClaimSol => Some(MinerAction::ClaimOre),
                    MinerAction::ClaimOre => None, // Move to next miner
                };
                
                match next_action {
                    Some(a) => Some(Selection::Action(*i, a)),
                    None => {
                        // Move to next miner's signer
                        let next_i = (*i + 1) % self.all_miners.len();
                        Some(Selection::Signer(next_i))
                    }
                }
            }
        };
        
        // Ensure selected miner is visible
        if let Some(i) = self.get_selection_miner_index() {
            if i >= self.scroll_offset + self.visible_miners() {
                self.scroll_offset = i.saturating_sub(self.visible_miners() - 1);
            }
        }
    }
    
    /// Select previous item
    /// Navigation order per miner (reverse): ClaimOre -> ClaimSol -> Checkpoint (if not legacy) -> Auth -> Signer -> prev miner
    pub fn select_prev(&mut self) {
        if self.all_miners.is_empty() {
            return;
        }
        
        self.selection = match &self.selection {
            None => {
                // Start at last miner's last action
                Some(Selection::Action(self.all_miners.len() - 1, MinerAction::ClaimOre))
            }
            Some(Selection::Signer(i)) => {
                if *i == 0 {
                    // Wrap to last miner's last action
                    Some(Selection::Action(self.all_miners.len() - 1, MinerAction::ClaimOre))
                } else {
                    // Go to previous miner's last action
                    Some(Selection::Action(i - 1, MinerAction::ClaimOre))
                }
            }
            Some(Selection::Auth(i)) => {
                Some(Selection::Signer(*i))
            }
            Some(Selection::Action(i, action)) => {
                let miner = &self.all_miners[*i];
                let prev = match action {
                    MinerAction::Checkpoint => None, // Move to Auth
                    MinerAction::ClaimSol => {
                        if miner.is_legacy {
                            None // Move to Auth (skip checkpoint)
                        } else {
                            Some(MinerAction::Checkpoint)
                        }
                    }
                    MinerAction::ClaimOre => Some(MinerAction::ClaimSol),
                };
                
                match prev {
                    Some(a) => Some(Selection::Action(*i, a)),
                    None => Some(Selection::Auth(*i)),
                }
            }
        };
        
        // Ensure selected miner is visible
        if let Some(i) = self.get_selection_miner_index() {
            if i < self.scroll_offset {
                self.scroll_offset = i;
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
            let is_signer_selected = app.selection == Some(Selection::Signer(i));
            let is_auth_selected = app.selection == Some(Selection::Auth(i));
            let is_any_on_this_row = is_signer_selected || is_auth_selected 
                || app.selection == Some(Selection::Action(i, MinerAction::Checkpoint))
                || app.selection == Some(Selection::Action(i, MinerAction::ClaimSol))
                || app.selection == Some(Selection::Action(i, MinerAction::ClaimOre));
            
            let mut spans = Vec::new();
            
            // Selection indicator
            if is_any_on_this_row {
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
            
            // Signer pubkey (selectable)
            let signer_style = if is_signer_selected {
                Style::default().fg(Color::White).bold().on_blue()
            } else {
                Style::default().fg(Color::Gray)
            };
            spans.push(Span::styled(
                format!("Signer:{} ", shorten_pubkey(&miner.signer)),
                signer_style
            ));
            
            // Auth PDA (selectable)
            let auth_style = if is_auth_selected {
                Style::default().fg(Color::White).bold().on_blue()
            } else {
                Style::default().fg(Color::Cyan)
            };
            spans.push(Span::styled(
                format!("Auth:{} ", shorten_pubkey(&miner.authority_pda)),
                auth_style
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
    // Skip preflight status indicator
    let preflight_style = if app.skip_preflight {
        Style::default().fg(Color::Green).bold()
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let preflight_text = if app.skip_preflight { "[S]kip Preflight: ON " } else { "[S]kip Preflight: OFF " };
    
    // Operation in progress indicator
    let busy_indicator = if app.operation_in_progress {
        Span::styled("⏳ ", Style::default().fg(Color::Yellow))
    } else {
        Span::styled("", Style::default())
    };
    
    let help_spans = vec![
        busy_indicator,
        Span::styled(" [↑↓/jk] Navigate ", Style::default().fg(Color::DarkGray)),
        Span::styled("[Enter] Execute/Copy ", Style::default().fg(Color::Cyan)),
        Span::styled("[R] Refresh ", Style::default().fg(Color::Yellow)),
        Span::styled(preflight_text, preflight_style),
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
    CopyPubkey(Pubkey),
    ToggleSkipPreflight,
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
                    // Execute action or copy pubkey
                    KeyCode::Enter => {
                        // If an action is selected, execute it
                        if let Some((miner_idx, action)) = app.get_selected_action() {
                            return Ok(InputResult::ExecuteAction(miner_idx, action));
                        }
                        // If a pubkey is selected, copy it to clipboard
                        if let Some(pubkey) = app.get_selected_pubkey() {
                            return Ok(InputResult::CopyPubkey(pubkey));
                        }
                    }
                    // Refresh
                    KeyCode::Char('r') | KeyCode::Char('R') => {
                        return Ok(InputResult::Refresh);
                    }
                    // Toggle skip preflight
                    KeyCode::Char('s') | KeyCode::Char('S') => {
                        return Ok(InputResult::ToggleSkipPreflight);
                    }
                    _ => {}
                }
            }
        }
    }
    
    // Clear status message after 5 seconds (longer for error messages)
    if let Some((_, instant, is_error)) = &app.status_msg {
        let timeout = if *is_error { Duration::from_secs(8) } else { Duration::from_secs(3) };
        if instant.elapsed() > timeout {
            app.status_msg = None;
        }
    }
    
    Ok(InputResult::Continue)
}

/// Copy pubkey to clipboard and show status
pub fn copy_to_clipboard(app: &mut ManageApp, pubkey: &Pubkey) {
    let pubkey_str = pubkey.to_string();
    match cli_clipboard::set_contents(pubkey_str.clone()) {
        Ok(_) => {
            app.set_status(format!("Copied: {}", shorten_pubkey(pubkey)), false);
        }
        Err(e) => {
            app.set_status(format!("Copy failed: {}", e), true);
        }
    }
}

/// Parse RPC error to extract meaningful error message
pub fn format_rpc_error(error: &str) -> String {
    // Try to extract custom program error
    if let Some(idx) = error.find("Custom(") {
        if let Some(end_idx) = error[idx..].find(')') {
            let custom_code = &error[idx+7..idx+end_idx];
            return format!("Program error: Custom({})", custom_code);
        }
    }
    
    // Try to extract instruction error
    if error.contains("InstructionError") {
        if let Some(start) = error.find("InstructionError") {
            // Find closing bracket or end
            let snippet = &error[start..];
            let end = snippet.find(']').unwrap_or(snippet.len().min(80));
            return format!("Instruction error: {}", &snippet[..end]);
        }
    }
    
    // Try to extract simulation failed message
    if error.contains("Transaction simulation failed") {
        if let Some(start) = error.find("Transaction simulation failed") {
            let snippet = &error[start..];
            // Try to find the actual error within
            if snippet.contains("custom program error:") {
                if let Some(err_start) = snippet.find("custom program error:") {
                    let err_part = &snippet[err_start..];
                    let end = err_part.find('\n').unwrap_or(err_part.len().min(50));
                    return format!("Simulation failed: {}", &err_part[..end]);
                }
            }
            let end = snippet.find('\n').unwrap_or(snippet.len().min(100));
            return format!("{}", &snippet[..end]);
        }
    }
    
    // Try to extract preflight check message  
    if error.contains("Preflight check failed") {
        return "Preflight failed - try toggling [S]kip Preflight".to_string();
    }
    
    // Try to extract blockhash not found
    if error.contains("BlockhashNotFound") {
        return "Blockhash expired - try again".to_string();
    }
    
    // Try to extract insufficient funds
    if error.contains("insufficient funds") || error.contains("InsufficientFunds") {
        return "Insufficient funds for transaction".to_string();
    }
    
    // Account not found
    if error.contains("AccountNotFound") {
        return "Account not found on chain".to_string();
    }
    
    // Default: truncate long errors
    if error.len() > 80 {
        format!("{}...", &error[..77])
    } else {
        error.to_string()
    }
}
