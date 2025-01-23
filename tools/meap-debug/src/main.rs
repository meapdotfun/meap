//! MEAP Debug Tool
//! Interactive TUI for monitoring and debugging MEAP agents

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use meap_core::{
    agent::AgentStatus,
    connection::ConnectionPool,
    protocol::Message,
    error::Result,
};
use std::{
    io,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;
use tui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};

/// Application state
struct App {
    connection_pool: Arc<ConnectionPool>,
    messages: Arc<RwLock<Vec<Message>>>,
    selected_agent: Option<String>,
    show_help: bool,
}

impl App {
    fn new(connection_pool: Arc<ConnectionPool>) -> Self {
        Self {
            connection_pool,
            messages: Arc::new(RwLock::new(Vec::new())),
            selected_agent: None,
            show_help: false,
        }
    }

    async fn add_message(&self, message: Message) {
        let mut messages = self.messages.write().await;
        messages.push(message);
        if messages.len() > 100 {
            messages.remove(0);
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app state
    let config = meap_core::connection::ConnectionConfig {
        max_reconnects: 3,
        reconnect_delay: Duration::from_secs(1),
        buffer_size: 32,
    };
    let connection_pool = Arc::new(ConnectionPool::new(config));
    let app = Arc::new(App::new(connection_pool));

    // Start UI update loop
    let app_clone = app.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(100));
        loop {
            interval.tick().await;
            if let Err(e) = ui_loop(&mut terminal, &app_clone).await {
                eprintln!("UI error: {}", e);
                break;
            }
        }
    });

    // Start message monitoring
    monitor_messages(app.clone()).await?;

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

async fn ui_loop<B: tui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &App,
) -> io::Result<()> {
    terminal.draw(|f| {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),  // Help
                Constraint::Length(10), // Agents
                Constraint::Min(0),     // Messages
            ])
            .split(f.size());

        // Help section
        let help = if app.show_help {
            vec![
                Spans::from("q: Quit"),
                Spans::from("h: Toggle help"),
                Spans::from("↑/↓: Select agent"),
                Spans::from("Enter: View agent details"),
            ]
        } else {
            vec![Spans::from("Press 'h' for help")]
        };
        let help_text = Paragraph::new(help)
            .block(Block::default().borders(Borders::ALL).title("Help"));
        f.render_widget(help_text, chunks[0]);

        // Agents list
        let agents = app.connection_pool.connections().blocking_read();
        let agent_items: Vec<ListItem> = agents
            .iter()
            .map(|(id, conn)| {
                let status = if conn.is_alive() { "Active" } else { "Inactive" };
                ListItem::new(Spans::from(vec![
                    Span::raw(id),
                    Span::raw(" - "),
                    Span::styled(
                        status,
                        Style::default().fg(if conn.is_alive() {
                            Color::Green
                        } else {
                            Color::Red
                        }),
                    ),
                ]))
            })
            .collect();

        let agents_list = List::new(agent_items)
            .block(Block::default().borders(Borders::ALL).title("Agents"))
            .highlight_style(Style::default().add_modifier(Modifier::BOLD));
        f.render_widget(agents_list, chunks[1]);

        // Messages
        let messages = app.messages.blocking_read();
        let message_items: Vec<ListItem> = messages
            .iter()
            .map(|msg| {
                ListItem::new(vec![Spans::from(vec![
                    Span::styled(
                        format!("{} -> {}: ", msg.from, msg.to),
                        Style::default().fg(Color::Blue),
                    ),
                    Span::raw(format!("{:?}", msg.content)),
                ])])
            })
            .collect();

        let messages_list = List::new(message_items)
            .block(Block::default().borders(Borders::ALL).title("Messages"));
        f.render_widget(messages_list, chunks[2]);
    })?;

    if event::poll(Duration::from_millis(100))? {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Char('q') => return Err(io::Error::new(io::ErrorKind::Other, "quit")),
                KeyCode::Char('h') => app.show_help = !app.show_help,
                _ => {}
            }
        }
    }

    Ok(())
}

async fn monitor_messages(app: Arc<App>) -> Result<()> {
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        interval.tick().await;
        // TODO: Implement message monitoring
    }
    Ok(())
} 