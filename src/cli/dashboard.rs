use crate::orchestrator::{ActiveAgent, AgentEvent, SubTaskResult, SubTaskStatus, SystemState};
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph},
};
use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

const TICK_MS: u64 = 50;

const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

const LOGO: &str = "
 ██████╗  ██████╗ ██████╗ ████████╗ █████╗ ██╗██╗
 ██╔══██╗██╔═══██╗██╔══██╗╚══██╔══╝██╔══██╗██║██║
 ██████╔╝██║   ██║██████╔╝   ██║   ███████║██║██║
 ██╔═══╝ ██║   ██║██╔══██╗   ██║   ██╔══██║██║██║
 ██║     ╚██████╔╝██║  ██║   ██║   ██║  ██║██║███████╗
 ╚═╝      ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚══════╝
";

pub type Dashboard = OrchestratorDashboard;

pub struct OrchestratorDashboard {
    state: Arc<Mutex<SystemState>>,
    log_buffer: Vec<String>,
    tick_count: u64,
}

impl Default for OrchestratorDashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl OrchestratorDashboard {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SystemState::new())),
            log_buffer: Vec::with_capacity(256),
            tick_count: 0,
        }
    }

    pub fn state_handle(&self) -> Arc<Mutex<SystemState>> {
        Arc::clone(&self.state)
    }

    pub async fn push_log(&mut self, msg: String) {
        self.log_buffer.push(msg);
        if self.log_buffer.len() > 256 {
            self.log_buffer.remove(0);
        }
    }

    pub async fn apply_event(&mut self, event: AgentEvent) {
        let mut state = self.state.lock().await;
        match event {
            AgentEvent::AgentStarted { agent_id, task } => {
                state.active_agents.push(ActiveAgent {
                    agent_id,
                    task,
                    progress: "initializing".into(),
                    started_at: Instant::now(),
                });
            }
            AgentEvent::AgentProgress { agent_id, message } => {
                if let Some(a) = state.active_agents.iter_mut().find(|a| a.agent_id == agent_id) {
                    a.progress = message.clone();
                }
                let log = format!("[{agent_id}] {message}");
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::AgentCompleted { agent_id, result } => {
                state.active_agents.retain(|a| a.agent_id != agent_id);
                state.completed.push(result.clone());
                state.total_tokens += result.token_cost;
                let log = format!("[{agent_id}] completed ({})", result.token_cost);
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::AgentFailed { agent_id, error } => {
                state.active_agents.retain(|a| a.agent_id != agent_id);
                state.completed.push(SubTaskResult {
                    task_id: agent_id.clone(),
                    status: SubTaskStatus::Failed(error.clone()),
                    output: None,
                    files_changed: vec![],
                    token_cost: 0,
                    duration_ms: 0,
                    error: Some(error.clone()),
                });
                let log = format!("[{agent_id}] failed: {error}");
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::OrchestratorLog { message } => {
                let log = format!("[orchestrator] {message}");
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::GoalComplete { .. } => {
                let log = "[orchestrator] goal complete".to_string();
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::AgentCheckedIn { registration } => {
                let log = format!("[fleet] agent '{}' checked in ({})", registration.id, registration.provider);
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
            AgentEvent::AgentCheckedOut { agent_id } => {
                let log = format!("[fleet] agent '{agent_id}' checked out");
                self.log_buffer.push(log);
                if self.log_buffer.len() > 256 {
                    self.log_buffer.remove(0);
                }
            }
        }
    }

    pub fn run_tui(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick_rate = Duration::from_millis(TICK_MS);
        let mut last_tick = Instant::now();
        let mut running = true;

        while running {
            terminal.draw(|f| self.draw(f))?;

            let timeout = tick_rate.saturating_sub(last_tick.elapsed());
            if event::poll(timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        running = false;
                    }
                }
            }

            if last_tick.elapsed() >= tick_rate {
                self.tick_count += 1;
                last_tick = Instant::now();
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        Ok(())
    }

    fn draw(&self, f: &mut Frame) {
        let area = f.area();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),
                Constraint::Min(1),
            ])
            .split(area);

        self.draw_banner(f, chunks[0]);
        self.draw_main_grid(f, chunks[1]);
    }

    fn draw_banner(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let uptime = self.state.blocking_lock().uptime_secs();
        let total = self.state.blocking_lock().total_tokens;
        let active = self.state.blocking_lock().active_agents.len();
        let done = self.state.blocking_lock().completed.len();

        let lines = vec![
            Line::from(vec![
                Span::raw(LOGO).style(Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw(" Uptime: ").style(Style::default().fg(Color::Green)),
                Span::raw(format!("{uptime}s")).style(Style::default().fg(Color::White)),
                Span::raw(" │ Agents: ").style(Style::default().fg(Color::Yellow)),
                Span::raw(format!("{active} active, {done} done")).style(Style::default().fg(Color::White)),
                Span::raw(" │ Tokens: ").style(Style::default().fg(Color::Magenta)),
                Span::raw(format!("{total}")).style(Style::default().fg(Color::White)),
            ]),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }

    fn draw_main_grid(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(60),
            ])
            .split(area);

        self.draw_log_panel(f, chunks[0]);
        self.draw_agent_matrix(f, chunks[1]);
    }

    fn draw_log_panel(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Orchestrator Log ")
            .style(Style::default().fg(Color::Cyan));

        let items: Vec<ListItem> = self.log_buffer.iter().rev().take(32).map(|msg| {
            let prefix = if msg.contains("failed") || msg.contains("error") {
                Color::Red
            } else if msg.contains("completed") {
                Color::Green
            } else if msg.contains("[orchestrator]") {
                Color::Cyan
            } else {
                Color::White
            };
            ListItem::new(Text::from(msg.as_str())).style(Style::default().fg(prefix))
        }).collect();

        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }

    fn draw_agent_matrix(&self, f: &mut Frame, area: Rect) {
        let state = self.state.blocking_lock();

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Agents ({}/{}) ", state.active_agents.len(), state.completed.len()))
            .style(Style::default().fg(Color::Yellow));

        let inner = block.inner(area);
        f.render_widget(block, area);

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints(std::iter::repeat_n(Constraint::Length(4), state.active_agents.len().max(1)))
            .split(inner);

        if state.active_agents.is_empty() {
            let done = state.completed.len();
            let msg = if done > 0 {
                format!(" All tasks complete ({done} finished). Press q to exit.")
            } else {
                " No active agents. Waiting for tasks...".to_string()
            };
            f.render_widget(
                Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
                rows.first().copied().unwrap_or(inner),
            );
        } else {
            let spinner = SPINNER[(self.tick_count as usize / 3) % SPINNER.len()];
            for (i, agent) in state.active_agents.iter().enumerate() {
                if i >= rows.len() { break; }
                let agent_area = rows[i];
                self.draw_agent_card(f, agent_area, agent, spinner);
            }
        }
    }

    fn draw_agent_card(&self, f: &mut Frame, area: Rect, agent: &ActiveAgent, spinner: char) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Min(0)])
            .margin(1)
            .split(area);

        let label = format!(" {spinner} {} — {}", agent.agent_id, agent.task);
        f.render_widget(
            Paragraph::new(label).style(Style::default().fg(Color::Green)),
            chunks[0],
        );

        f.render_widget(
            Paragraph::new(agent.progress.as_str())
                .style(Style::default().fg(Color::DarkGray)),
            chunks[1],
        );

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .percent(50);
        f.render_widget(gauge, chunks[2]);
    }
}
