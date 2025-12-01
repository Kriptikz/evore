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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs},
    Frame, Terminal,
};
use solana_sdk::{pubkey::Pubkey, signature::Signature};

use evore::ore_api::{Board, Round};

/// App state for the TUI
pub struct App {
    pub running: bool,
    pub tab_index: usize,
    pub tabs: Vec<&'static str>,
    
    // Connection info
    pub rpc_url: String,
    pub signer: Pubkey,
    pub manager: Pubkey,
    pub auth_id: u64,
    pub managed_miner_auth: Pubkey,
    
    // Live state
    pub current_slot: u64,
    pub board: Option<Board>,
    pub round: Option<Round>,
    pub slot_history: Vec<u64>,
    
    // Deploy state
    pub deploy_status: DeployStatus,
    pub bankroll: u64,
    pub transactions_sent: u32,
    pub transactions_confirmed: u32,
    pub last_confirmed_sig: Option<Signature>,
    
    // Event log
    pub events: Vec<(Instant, String, EventType)>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum DeployStatus {
    Idle,
    WaitingForWindow,
    Deploying,
    Confirmed,
    Failed,
}

#[derive(Clone, Copy)]
pub enum EventType {
    Info,
    Success,
    Warning,
    Error,
}

impl App {
    pub fn new(
        rpc_url: String,
        signer: Pubkey,
        manager: Pubkey,
        auth_id: u64,
    ) -> Self {
        let (managed_miner_auth, _) = evore::state::managed_miner_auth_pda(manager, auth_id);
        
        Self {
            running: true,
            tab_index: 0,
            tabs: vec!["Dashboard", "Deployments", "Config"],
            rpc_url,
            signer,
            manager,
            auth_id,
            managed_miner_auth,
            current_slot: 0,
            board: None,
            round: None,
            slot_history: Vec::with_capacity(60),
            deploy_status: DeployStatus::Idle,
            bankroll: 0,
            transactions_sent: 0,
            transactions_confirmed: 0,
            last_confirmed_sig: None,
            events: Vec::new(),
        }
    }
    
    pub fn log(&mut self, msg: impl Into<String>, event_type: EventType) {
        self.events.push((Instant::now(), msg.into(), event_type));
        // Keep last 100 events
        if self.events.len() > 100 {
            self.events.remove(0);
        }
    }
    
    pub fn update_slot(&mut self, slot: u64) {
        self.current_slot = slot;
        self.slot_history.push(slot);
        if self.slot_history.len() > 60 {
            self.slot_history.remove(0);
        }
    }
    
    pub fn slots_remaining(&self) -> u64 {
        self.board
            .as_ref()
            .map(|b| b.end_slot.saturating_sub(self.current_slot))
            .unwrap_or(0)
    }
    
    pub fn round_progress(&self) -> f64 {
        if let Some(board) = &self.board {
            let total = board.end_slot.saturating_sub(board.start_slot);
            if total == 0 {
                return 0.0;
            }
            let elapsed = self.current_slot.saturating_sub(board.start_slot);
            (elapsed as f64 / total as f64).min(1.0)
        } else {
            0.0
        }
    }
}

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

pub fn draw(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Tabs
            Constraint::Min(10),    // Main content
            Constraint::Length(8),  // Event log
        ])
        .split(frame.area());
    
    draw_header(frame, chunks[0], app);
    draw_tabs(frame, chunks[1], app);
    
    match app.tab_index {
        0 => draw_dashboard(frame, chunks[2], app),
        1 => draw_deployments(frame, chunks[2], app),
        2 => draw_config(frame, chunks[2], app),
        _ => {}
    }
    
    draw_event_log(frame, chunks[3], app);
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let status_color = match app.deploy_status {
        DeployStatus::Idle => Color::Gray,
        DeployStatus::WaitingForWindow => Color::Yellow,
        DeployStatus::Deploying => Color::Cyan,
        DeployStatus::Confirmed => Color::Green,
        DeployStatus::Failed => Color::Red,
    };
    
    let status_text = match app.deploy_status {
        DeployStatus::Idle => "IDLE",
        DeployStatus::WaitingForWindow => "WAITING",
        DeployStatus::Deploying => "DEPLOYING",
        DeployStatus::Confirmed => "CONFIRMED",
        DeployStatus::Failed => "FAILED",
    };
    
    let title = Line::from(vec![
        Span::styled("  ⚡ EVORE ", Style::default().fg(Color::Magenta).bold()),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" Slot: {} ", app.current_slot), Style::default().fg(Color::Cyan)),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {} left ", app.slots_remaining()), 
            if app.slots_remaining() <= 2 {
                Style::default().fg(Color::Red).bold()
            } else {
                Style::default().fg(Color::Yellow)
            }
        ),
        Span::styled("│", Style::default().fg(Color::DarkGray)),
        Span::styled(format!(" {} ", status_text), Style::default().fg(status_color).bold()),
    ]);
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .style(Style::default().bg(Color::Rgb(20, 20, 30)));
    
    let paragraph = Paragraph::new(title).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<Line> = app.tabs.iter().map(|t| Line::from(*t)).collect();
    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
        .select(app.tab_index)
        .style(Style::default().fg(Color::Gray))
        .highlight_style(Style::default().fg(Color::Magenta).bold().add_modifier(Modifier::UNDERLINED));
    frame.render_widget(tabs, area);
}

fn draw_dashboard(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);
    
    // Left side - Round info and grid
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(10)])
        .split(chunks[0]);
    
    draw_round_progress(frame, left_chunks[0], app);
    draw_deployment_grid(frame, left_chunks[1], app);
    
    // Right side - Stats
    draw_stats(frame, chunks[1], app);
}

fn draw_round_progress(frame: &mut Frame, area: Rect, app: &App) {
    let progress = (app.round_progress() * 100.0) as u16;
    let round_id = app.board.as_ref().map(|b| b.round_id).unwrap_or(0);
    
    let gauge = Gauge::default()
        .block(Block::default()
            .title(format!(" Round {} ", round_id))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue)))
        .gauge_style(Style::default().fg(Color::Magenta).bg(Color::Rgb(40, 40, 50)))
        .percent(progress)
        .label(format!("{}% ({} slots left)", progress, app.slots_remaining()));
    
    frame.render_widget(gauge, area);
}

fn draw_deployment_grid(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Deployment Grid (5x5) ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    if let Some(round) = &app.round {
        // Calculate max deployment for color scaling
        let max_deploy = round.deployed.iter().max().copied().unwrap_or(1).max(1);
        
        let cell_width = (inner.width / 5).max(1);
        let cell_height = (inner.height / 5).max(1);
        
        for row in 0..5u16 {
            for col in 0..5u16 {
                let idx = (row * 5 + col) as usize;
                if idx >= round.deployed.len() {
                    continue;
                }
                let deployed = round.deployed[idx];
                
                // Color intensity based on deployment amount
                let intensity = ((deployed as f64 / max_deploy as f64) * 200.0) as u8;
                let color = if deployed > 0 {
                    Color::Rgb(intensity, 50, intensity / 2)
                } else {
                    Color::Rgb(30, 30, 40)
                };
                
                let cell_area = Rect {
                    x: inner.x + col * cell_width,
                    y: inner.y + row * cell_height,
                    width: cell_width,
                    height: cell_height,
                };
                
                // Format deployment in SOL
                let sol = deployed as f64 / 1_000_000_000.0;
                let text = if sol >= 1.0 {
                    format!("{:.1}", sol)
                } else if sol >= 0.01 {
                    format!("{:.2}", sol)
                } else if deployed > 0 {
                    "◆".to_string()
                } else {
                    "·".to_string()
                };
                
                let cell = Paragraph::new(text)
                    .style(Style::default().fg(Color::White).bg(color))
                    .alignment(ratatui::layout::Alignment::Center);
                
                frame.render_widget(cell, cell_area);
            }
        }
    }
}

fn draw_stats(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Stats ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let total_deployed: u64 = app.round.as_ref().map(|r| r.deployed.iter().sum()).unwrap_or(0);
    let sol_deployed = total_deployed as f64 / 1_000_000_000.0;
    let bankroll_sol = app.bankroll as f64 / 1_000_000_000.0;
    
    let items = vec![
        format!("Total Deployed: {:.4} SOL", sol_deployed),
        format!("Bankroll: {:.4} SOL", bankroll_sol),
        String::new(),
        format!("Txns Sent: {}", app.transactions_sent),
        format!("Txns Confirmed: {}", app.transactions_confirmed),
        String::new(),
        format!("Auth ID: {}", app.auth_id),
    ];
    
    let list_items: Vec<ListItem> = items
        .into_iter()
        .map(|s| {
            ListItem::new(Line::from(s))
                .style(Style::default().fg(Color::White))
        })
        .collect();
    
    let list = List::new(list_items).block(block);
    frame.render_widget(list, area);
}

fn draw_deployments(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Recent Transactions ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let content = if let Some(sig) = app.last_confirmed_sig {
        format!("Last confirmed: {}", sig)
    } else {
        "No transactions yet".to_string()
    };
    
    let paragraph = Paragraph::new(content).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_config(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Configuration ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Blue));
    
    let text = vec![
        Line::from(vec![
            Span::styled("RPC: ", Style::default().fg(Color::Gray)),
            Span::styled(&app.rpc_url, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Signer: ", Style::default().fg(Color::Gray)),
            Span::styled(app.signer.to_string(), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Manager: ", Style::default().fg(Color::Gray)),
            Span::styled(app.manager.to_string(), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Managed Miner Auth: ", Style::default().fg(Color::Gray)),
            Span::styled(app.managed_miner_auth.to_string(), Style::default().fg(Color::Green)),
        ]),
    ];
    
    let paragraph = Paragraph::new(text).block(block);
    frame.render_widget(paragraph, area);
}

fn draw_event_log(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .title(" Event Log ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    let items: Vec<ListItem> = app.events
        .iter()
        .rev()
        .take(6)
        .map(|(instant, msg, event_type)| {
            let color = match event_type {
                EventType::Info => Color::Cyan,
                EventType::Success => Color::Green,
                EventType::Warning => Color::Yellow,
                EventType::Error => Color::Red,
            };
            let elapsed = instant.elapsed().as_secs();
            let time_str = if elapsed < 60 {
                format!("{}s ago", elapsed)
            } else {
                format!("{}m ago", elapsed / 60)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("[{}] ", time_str), Style::default().fg(Color::DarkGray)),
                Span::styled(msg, Style::default().fg(color)),
            ]))
        })
        .collect();
    
    let list = List::new(items).block(block);
    frame.render_widget(list, area);
}

/// Handle keyboard input
pub fn handle_input(app: &mut App) -> io::Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.running = false;
                        return Ok(true);
                    }
                    KeyCode::Tab | KeyCode::Right => {
                        app.tab_index = (app.tab_index + 1) % app.tabs.len();
                    }
                    KeyCode::BackTab | KeyCode::Left => {
                        app.tab_index = if app.tab_index == 0 {
                            app.tabs.len() - 1
                        } else {
                            app.tab_index - 1
                        };
                    }
                    KeyCode::Char('d') => {
                        // Trigger deploy
                        if app.deploy_status == DeployStatus::Idle {
                            app.deploy_status = DeployStatus::WaitingForWindow;
                            app.log("Deploy initiated...", EventType::Info);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(false)
}

