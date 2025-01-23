use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use meap_core::{
    agent::{Agent, AgentStatus},
    protocol::Message,
};
use std::{
    io,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame, Terminal,
};

struct App {
    messages: Vec<Message>,
    agents: Vec<Agent>,
    selected_tab: usize,
    scroll: u16,
}

impl App {
    fn new() -> App {
        App {
            messages: Vec::new(),
            agents: Vec::new(),
            selected_tab: 0,
            scroll: 0,
        }
    }
}

fn main() -> Result<(), io::Error> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run it
    let app = App::new();
    let res = run_app(&mut terminal, app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) -> io::Result<()> {
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(250);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Tab => {
                        app.selected_tab = (app.selected_tab + 1) % 2;
                    }
                    KeyCode::Up => {
                        app.scroll = app.scroll.saturating_sub(1);
                    }
                    KeyCode::Down => {
                        app.scroll = app.scroll.saturating_add(1);
                    }
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            // Update app state
            last_tick = Instant::now();
        }
    }
}

fn ui<B: Backend>(f: &mut Frame<B>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(f.size());

    // Title
    let title = Paragraph::new("MEAP Debug Console")
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Main content
    let content = match app.selected_tab {
        0 => render_messages(app),
        1 => render_agents(app),
        _ => vec![],
    };

    let content_list = List::new(content)
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(Color::White));
    f.render_widget(content_list, chunks[1]);

    // Footer
    let footer = Paragraph::new(vec![Spans::from(vec![
        Span::raw("Press "),
        Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to switch views, "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(" to quit"),
    ])])
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

fn render_messages(app: &App) -> Vec<ListItem> {
    app.messages
        .iter()
        .map(|msg| {
            ListItem::new(vec![
                Spans::from(vec![
                    Span::styled(
                        format!("[{}] ", msg.message_type),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::raw(format!("{} -> {}", msg.from, msg.to)),
                ]),
                Spans::from(vec![Span::raw(format!(
                    "  {}",
                    msg.content.to_string()
                ))]),
            ])
        })
        .collect()
}

fn render_agents(app: &App) -> Vec<ListItem> {
    app.agents
        .iter()
        .map(|agent| {
            ListItem::new(vec![Spans::from(vec![
                Span::styled(
                    format!("[{}] ", agent.id()),
                    Style::default().fg(Color::Green),
                ),
                Span::raw(format!("Capabilities: {:?}", agent.capabilities())),
            ])])
        })
        .collect()
} 