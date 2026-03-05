mod api;
mod app;
mod config;
mod ui;

use anyhow::Result;
use app::AppState;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    Terminal,
};
use std::io;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // 設定読み込み
    let cfg = match config::load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("設定エラー: {}", e);
            eprintln!("~/.config/noca/config.toml を作成してください。");
            eprintln!("\n例:\n[auth]\nintegration_token = \"secret_xxx\"\n\n[[databases]]\nid = \"your-db-id\"\nname = \"My Calendar\"\ncolor = \"green\"");
            std::process::exit(1);
        }
    };

    let client = api::NotionClient::new(cfg.auth.integration_token.clone());
    let mut state = AppState::new(cfg.databases.clone());

    // 初回イベント取得
    fetch_events(&client, &mut state, &cfg.databases).await;

    // TUI初期化
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut state, &client, &cfg.databases).await;

    // TUI終了処理
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run_app(
    terminal: &mut ratatui::Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    client: &api::NotionClient,
    databases: &[config::DatabaseConfig],
) -> Result<()> {
    loop {
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(22), Constraint::Min(0)])
                .split(f.area());

            ui::sidebar::render_sidebar(f, chunks[0], state);
            ui::week_view::render_week_view(f, chunks[1], state);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('h') => {
                        state.prev_week();
                        fetch_events(client, state, databases).await;
                    }
                    KeyCode::Char('l') => {
                        state.next_week();
                        fetch_events(client, state, databases).await;
                    }
                    KeyCode::Char('j') => state.scroll_down(),
                    KeyCode::Char('k') => state.scroll_up(),
                    KeyCode::Char('H') => state.select_prev_day(),
                    KeyCode::Char('L') => state.select_next_day(),
                    KeyCode::Char('t') => {
                        state.go_to_today();
                        fetch_events(client, state, databases).await;
                    }
                    KeyCode::Tab => state.toggle_panel(),
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn fetch_events(
    client: &api::NotionClient,
    state: &mut AppState,
    databases: &[config::DatabaseConfig],
) {
    let week_start = state.current_week_start;
    let start_str = week_start.format("%Y-%m-%d").to_string();
    let end_str = (week_start + chrono::Duration::weeks(3))
        .format("%Y-%m-%d")
        .to_string();

    state.loading = true;

    for db in databases {
        match client.query_database(&db.id, &start_str, &end_str).await {
            Ok(pages) => {
                for page in &pages {
                    if let Some(event) = api::parse_event(page, &db.id) {
                        let date = event.date_start.or_else(|| {
                            event
                                .datetime_start
                                .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
                        });
                        if let Some(d) = date {
                            state.events.entry(d).or_default().push(event);
                        }
                    }
                }
            }
            Err(e) => {
                state.status_message = Some(format!("API Error: {}", e));
            }
        }
    }

    state.loading = false;
}
