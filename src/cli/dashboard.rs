use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Sparkline, Tabs},
};
use std::io;
use std::time::{Duration, Instant};

// ── Constants ────────────────────────────────────────────────────

const RING_CAP: usize = 256;
const TICK_MS: u64 = 250;

const LOGO: &str = "\n ██████╗  ██████╗ ██████╗ ████████╗ █████╗ ██╗██╗     \n ██╔══██╗██╔═══██╗██╔══██╗╚══██╔══╝██╔══██╗██║██║     \n ██████╔╝██║   ██║██████╔╝   ██║   ███████║██║██║     \n ██╔═══╝ ██║   ██║██╔══██╗   ██║   ██╔══██║██║██║     \n ██║     ╚██████╔╝██║  ██║   ██║   ██║  ██║██║███████╗\n ╚═╝      ╚═════╝ ╚═╝  ╚═╝   ╚═╝   ╚═╝  ╚═╝╚═╝╚════╝\n";

// ── Ring buffer: fixed-capacity, zero-alloc push ─────────────────

struct Ring<T: Copy + Default> {
    buf: [T; RING_CAP],
    head: usize,
    len: usize,
}

impl<T: Copy + Default> Ring<T> {
    fn new() -> Self {
        Self {
            buf: [T::default(); RING_CAP],
            head: 0,
            len: 0,
        }
    }

    fn push(&mut self, val: T) {
        self.buf[self.head] = val;
        self.head = (self.head + 1) % RING_CAP;
        if self.len < RING_CAP {
            self.len += 1;
        }
    }

    fn as_slice(&self) -> &[T] {
        if self.len < RING_CAP {
            &self.buf[..self.len]
        } else {
            // Wrap-around: tail..end + 0..head
            // For sparkline we just read linear; the visual difference is negligible
            &self.buf
        }
    }

    fn last(&self) -> T {
        if self.len == 0 {
            T::default()
        } else {
            self.buf[(self.head + RING_CAP - 1) % RING_CAP]
        }
    }
}

// Default for u64 is 0

// ── Tab state ────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Network,
    Events,
    Hooks,
    Cache,
    Health,
}

impl Tab {
    const ALL: &'static [Tab] = &[
        Tab::Network,
        Tab::Events,
        Tab::Hooks,
        Tab::Cache,
        Tab::Health,
    ];

    fn title(self) -> &'static str {
        match self {
            Tab::Network => "Network",
            Tab::Events => "Events",
            Tab::Hooks => "Hooks",
            Tab::Cache => "Cache",
            Tab::Health => "Health",
        }
    }

    fn index(self) -> usize {
        match self {
            Tab::Network => 0,
            Tab::Events => 1,
            Tab::Hooks => 2,
            Tab::Cache => 3,
            Tab::Health => 4,
        }
    }
}

// ── Owned data (set by caller, borrowed during render) ───────────

pub struct EventEntry {
    pub timestamp: String,
    pub agent_id: String,
    pub event_type: String,
    pub message: String,
}

pub struct HookEntry {
    pub id: String,
    pub name: String,
    pub match_path: String,
    pub enabled: bool,
}

pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub entries: u64,
    pub size_bytes: u64,
}

pub struct HealthStatus {
    pub proxy: bool,
    pub cdn: bool,
    pub mcp: bool,
    pub events: bool,
    pub uptime_secs: u64,
    pub config_healthy: bool,
    pub config_error: Option<String>,
    pub alerts: Vec<String>,
}

// ── Dashboard: singleton, owns all state ─────────────────────────

pub struct Dashboard {
    // Tab
    tab: Tab,
    quit: bool,

    // Network ring buffers (fixed-capacity, zero-alloc)
    net_reqs: Ring<u64>,
    net_bytes_in: Ring<u64>,
    net_bytes_out: Ring<u64>,
    net_errors: Ring<u64>,
    net_latency: Ring<u64>,

    // Live counters (delta per tick)
    prev_reqs: u64,
    prev_bytes_in: u64,
    prev_bytes_out: u64,

    // Snapshot data (set by caller, borrowed during render)
    events: Vec<EventEntry>,
    hooks: Vec<HookEntry>,
    cache: CacheStats,
    health: HealthStatus,

    // ── v1.1: config health ──
    pub config_healthy: bool,
    pub config_error: Option<String>,
    pub alerts: Vec<String>,

    // Timing
    last_tick: Instant,
    last_refresh: Instant,
    uptime_start: Instant,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Dashboard {
    pub fn new() -> Self {
        Self {
            tab: Tab::Network,
            quit: false,
            net_reqs: Ring::new(),
            net_bytes_in: Ring::new(),
            net_bytes_out: Ring::new(),
            net_errors: Ring::new(),
            net_latency: Ring::new(),
            prev_reqs: 0,
            prev_bytes_in: 0,
            prev_bytes_out: 0,
            events: Vec::new(),
            hooks: Vec::new(),
            cache: CacheStats {
                hits: 0,
                misses: 0,
                entries: 0,
                size_bytes: 0,
            },
            health: HealthStatus {
                proxy: true,
                cdn: false,
                mcp: false,
                events: true,
                uptime_secs: 0,
                config_healthy: true,
                config_error: None,
                alerts: Vec::new(),
            },
            config_healthy: true,
            config_error: None,
            alerts: Vec::new(),
            last_tick: Instant::now(),
            last_refresh: Instant::now(),
            uptime_start: Instant::now(),
        }
    }

    // ── Data injection (called by caller with live data) ─────────

    pub fn push_net_sample(
        &mut self,
        total_reqs: u64,
        total_bytes_in: u64,
        total_bytes_out: u64,
        total_errors: u64,
        latency_us: u64,
    ) {
        let dr = total_reqs.saturating_sub(self.prev_reqs);
        let dbi = total_bytes_in.saturating_sub(self.prev_bytes_in);
        let dbo = total_bytes_out.saturating_sub(self.prev_bytes_out);
        self.net_reqs.push(dr);
        self.net_bytes_in.push(dbi);
        self.net_bytes_out.push(dbo);
        self.net_errors.push(total_errors);
        self.net_latency.push(latency_us);
        self.prev_reqs = total_reqs;
        self.prev_bytes_in = total_bytes_in;
        self.prev_bytes_out = total_bytes_out;
    }

    pub fn set_events(&mut self, events: Vec<EventEntry>) {
        self.events = events;
    }
    pub fn set_hooks(&mut self, hooks: Vec<HookEntry>) {
        self.hooks = hooks;
    }
    pub fn set_cache(&mut self, cache: CacheStats) {
        self.cache = cache;
    }
    pub fn set_health(&mut self, health: HealthStatus) {
        self.health = health;
    }

    // ── Non-interactive text output ──────────────────────────────

    pub fn render_text(&self) -> String {
        let mut o = String::with_capacity(1024);
        o.push_str("portail — unified proxy/gateway\n");
        o.push_str(&format!(
            "uptime: {}s\n\n",
            self.uptime_start.elapsed().as_secs()
        ));
        o.push_str("health:\n");
        o.push_str(&format!(
            "  proxy:   {}\n",
            if self.health.proxy { "✓" } else { "✗" }
        ));
        o.push_str(&format!(
            "  cdn:     {}\n",
            if self.health.cdn { "✓" } else { "✗" }
        ));
        o.push_str(&format!(
            "  mcp:     {}\n",
            if self.health.mcp { "✓" } else { "✗" }
        ));
        o.push_str(&format!(
            "  events:  {}\n",
            if self.health.events { "✓" } else { "✗" }
        ));
        o.push_str(&format!(
            "\ncache: {} entries, {} hits, {} misses\n",
            self.cache.entries, self.cache.hits, self.cache.misses
        ));
        o.push_str(&format!("hooks: {} registered\n", self.hooks.len()));
        o.push_str(&format!("events: {} recent\n", self.events.len()));
        o.push_str(&format!(
            "network: {} req/s (last), {} samples\n",
            self.net_reqs.last(),
            self.net_reqs.len
        ));
        o
    }

    // ── TUI entry point ──────────────────────────────────────────

    pub fn run_tui(&mut self) -> Result<()> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let tick = Duration::from_millis(TICK_MS);

        loop {
            // Push a zero sample every tick to keep the sparkline moving
            if self.last_tick.elapsed() >= tick {
                self.net_reqs.push(0);
                self.net_bytes_in.push(0);
                self.net_bytes_out.push(0);
                self.net_errors.push(0);
                self.net_latency.push(0);
                self.last_tick = Instant::now();
            }

            terminal.draw(|f| self.draw(f))?;

            if event::poll(tick)? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
                            KeyCode::Tab => self.next_tab(),
                            KeyCode::BackTab => self.prev_tab(),
                            KeyCode::Char('1') => self.tab = Tab::Network,
                            KeyCode::Char('2') => self.tab = Tab::Events,
                            KeyCode::Char('3') => self.tab = Tab::Hooks,
                            KeyCode::Char('4') => self.tab = Tab::Cache,
                            KeyCode::Char('5') => self.tab = Tab::Health,
                            KeyCode::Char('r') => self.last_refresh = Instant::now(),
                            KeyCode::Char('c') => {
                                self.alerts.clear();
                                self.config_error = None;
                            }
                            _ => {}
                        }
                    }
                }
            }

            if self.quit {
                break;
            }
        }

        disable_raw_mode()?;
        execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
        terminal.show_cursor()?;
        Ok(())
    }

    // ── Tab navigation ───────────────────────────────────────────

    fn next_tab(&mut self) {
        self.tab = Tab::ALL[(self.tab.index() + 1) % Tab::ALL.len()];
    }

    fn prev_tab(&mut self) {
        self.tab = Tab::ALL[(self.tab.index() + Tab::ALL.len() - 1) % Tab::ALL.len()];
    }

    // ── Frame draw ───────────────────────────────────────────────

    fn draw(&self, f: &mut Frame) {
        let root = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(8),  // logo
                Constraint::Length(3),  // tabs
                Constraint::Length(12), // network sparklines (always visible)
                Constraint::Min(0),     // tab content
                Constraint::Length(4),  // controls + status
            ])
            .split(f.area());

        self.draw_logo(f, root[0]);
        self.draw_tabs(f, root[1]);
        self.draw_network(f, root[2]);
        self.draw_content(f, root[3]);
        self.draw_controls(f, root[4]);
    }

    fn draw_logo(&self, f: &mut Frame, area: Rect) {
        f.render_widget(
            Paragraph::new(LOGO)
                .style(Style::default().fg(Color::Cyan))
                .alignment(ratatui::layout::Alignment::Center),
            area,
        );
    }

    fn draw_tabs(&self, f: &mut Frame, area: Rect) {
        let titles: Vec<Line> = Tab::ALL
            .iter()
            .map(|&t| {
                let style = if t == self.tab {
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(format!(" {} ", t.title()), style))
            })
            .collect();

        f.render_widget(
            Tabs::new(titles)
                .block(Block::default().borders(Borders::ALL).title(" Navigation "))
                .select(self.tab.index()),
            area,
        );
    }

    // ── Network sparklines (always visible) ──────────────────────

    fn draw_network(&self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(25),
            ])
            .split(area);

        self.draw_sparkline(
            f,
            cols[0],
            "Requests/s",
            self.net_reqs.as_slice(),
            Color::Green,
        );
        self.draw_sparkline(
            f,
            cols[1],
            "Bytes In/s",
            self.net_bytes_in.as_slice(),
            Color::Cyan,
        );
        self.draw_sparkline(
            f,
            cols[2],
            "Bytes Out/s",
            self.net_bytes_out.as_slice(),
            Color::Blue,
        );
        self.draw_sparkline(
            f,
            cols[3],
            "Latency μs",
            self.net_latency.as_slice(),
            Color::Yellow,
        );
    }

    fn draw_sparkline(&self, f: &mut Frame, area: Rect, title: &str, data: &[u64], color: Color) {
        let spark = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title(title))
            .data(data)
            .style(Style::default().fg(color));
        f.render_widget(spark, area);
    }

    // ── Tab content ──────────────────────────────────────────────

    fn draw_content(&self, f: &mut Frame, area: Rect) {
        match self.tab {
            Tab::Network => self.draw_net_detail(f, area),
            Tab::Events => self.draw_events(f, area),
            Tab::Hooks => self.draw_hooks(f, area),
            Tab::Cache => self.draw_cache(f, area),
            Tab::Health => self.draw_health(f, area),
        }
    }

    fn draw_net_detail(&self, f: &mut Frame, area: Rect) {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area);

        // Counters
        let reqs = self.net_reqs.last();
        let bi = self.net_bytes_in.last();
        let bo = self.net_bytes_out.last();
        let errs = self.net_errors.last();
        let lat = self.net_latency.last();

        let counters = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  req/s: {}  ", reqs),
                Style::default().fg(Color::Green),
            ),
            Span::styled(
                format!("in: {}  ", human_bytes(bi)),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                format!("out: {}  ", human_bytes(bo)),
                Style::default().fg(Color::Blue),
            ),
            Span::styled(format!("errs: {}  ", errs), Style::default().fg(Color::Red)),
            Span::styled(
                format!("lat: {}μs", lat),
                Style::default().fg(Color::Yellow),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Live Counters "),
        );
        f.render_widget(counters, rows[0]);

        // Error rate sparkline
        let err_spark = Sparkline::default()
            .block(Block::default().borders(Borders::ALL).title(" Error Rate "))
            .data(self.net_errors.as_slice())
            .style(Style::default().fg(Color::Red));
        f.render_widget(err_spark, rows[1]);
    }

    fn draw_events(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .events
            .iter()
            .rev()
            .map(|e| {
                let style = match e.event_type.as_str() {
                    "error" => Style::default().fg(Color::Red),
                    "warning" => Style::default().fg(Color::Yellow),
                    "info" => Style::default().fg(Color::Green),
                    _ => Style::default().fg(Color::DarkGray),
                };
                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", e.timestamp),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(format!("{} ", e.agent_id), Style::default().fg(Color::Cyan)),
                    Span::styled(format!("{} ", e.event_type), style),
                    Span::raw(&e.message),
                ]))
            })
            .collect();

        f.render_widget(
            List::new(items).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Agent Events "),
            ),
            area,
        );
    }

    fn draw_hooks(&self, f: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .hooks
            .iter()
            .map(|h| {
                let (sym, color) = if h.enabled {
                    ("✓", Color::Green)
                } else {
                    ("✗", Color::Red)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", sym), Style::default().fg(color)),
                    Span::styled(format!("[{}] ", h.id), Style::default().fg(Color::DarkGray)),
                    Span::styled(
                        &h.name,
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!(" → {}", h.match_path)),
                ]))
            })
            .collect();

        f.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Hooks ")),
            area,
        );
    }

    fn draw_cache(&self, f: &mut Frame, area: Rect) {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let total = self.cache.hits + self.cache.misses;
        let rate = if total > 0 {
            self.cache.hits as f64 / total as f64
        } else {
            0.0
        };

        f.render_widget(
            Gauge::default()
                .block(Block::default().borders(Borders::ALL).title(" Hit Rate "))
                .gauge_style(Style::default().fg(Color::Green))
                .ratio(rate)
                .label(format!("{:.1}%", rate * 100.0)),
            cols[0],
        );

        f.render_widget(
            Paragraph::new(vec![
                Line::from(format!("  entries: {}", self.cache.entries)),
                Line::from(format!("  size:    {}", human_bytes(self.cache.size_bytes))),
                Line::from(format!("  hits:    {}", self.cache.hits)),
                Line::from(format!("  misses:  {}", self.cache.misses)),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Cache Stats "),
            ),
            cols[1],
        );
    }

    fn draw_health(&self, f: &mut Frame, area: Rect) {
        let item = |name: &str, ok: bool| -> ListItem<'static> {
            let (sym, color) = if ok {
                ("✓", Color::Green)
            } else {
                ("✗", Color::Red)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("  {} ", sym), Style::default().fg(color)),
                Span::raw(name.to_string()),
            ]))
        };

        let items = vec![
            item("Proxy", self.health.proxy),
            item("CDN Cache", self.health.cdn),
            item("MCP Sidecar", self.health.mcp),
            item("Event Log", self.health.events),
            ListItem::new(Line::from(vec![
                Span::styled("  Uptime: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}s", self.uptime_start.elapsed().as_secs())),
            ])),
        ];

        f.render_widget(
            List::new(items).block(Block::default().borders(Borders::ALL).title(" Health ")),
            area,
        );
    }

    fn draw_controls(&self, f: &mut Frame, area: Rect) {
        let elapsed = self.last_refresh.elapsed().as_secs_f64();

        // Config health dot
        let health_span = if self.config_healthy {
            Span::styled(
                " ● healthy ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(
                " ● broken ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )
        };

        // Alerts
        let mut spans = vec![health_span];
        if let Some(ref err) = self.config_error {
            spans.push(Span::styled(
                format!(" | config: {}", err),
                Style::default().fg(Color::Red),
            ));
        }
        for alert in &self.alerts {
            spans.push(Span::styled(
                format!(" | {}", alert),
                Style::default().fg(Color::Yellow),
            ));
        }

        let status_line = Line::from(spans);
        f.render_widget(
            Paragraph::new(status_line),
            Rect {
                y: area.y,
                x: area.x,
                width: area.width,
                height: 1,
            },
        );

        let line = Line::from(vec![
            Span::styled(
                " q",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" quit  "),
            Span::styled(
                "Tab",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" next  "),
            Span::styled(
                "1-5",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" jump  "),
            Span::styled(
                "r",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" refresh  "),
            Span::styled(
                "c",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" clear alerts  "),
            Span::raw(format!("{:.1}s ago", elapsed)),
        ]);
        f.render_widget(
            Paragraph::new(line)
                .block(Block::default().borders(Borders::ALL).title(" Controls "))
                .style(Style::default().fg(Color::DarkGray)),
            Rect {
                y: area.y + 1,
                x: area.x,
                width: area.width,
                height: area.height.saturating_sub(1),
            },
        );
    }
}

// ── Helpers (no alloc for small values) ──────────────────────────

fn human_bytes(b: u64) -> String {
    if b < 1024 {
        return format!("{}B", b);
    }
    if b < 1024 * 1024 {
        return format!("{:.1}KB", b as f64 / 1024.0);
    }
    if b < 1024 * 1024 * 1024 {
        return format!("{:.1}MB", b as f64 / (1024.0 * 1024.0));
    }
    format!("{:.1}GB", b as f64 / (1024.0 * 1024.0 * 1024.0))
}
