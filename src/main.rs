mod api;
mod app;
mod config;
mod ui;

use anyhow::Result;
use app::{AppMessage, AppState};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut state, &client, &cfg.databases).await;

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
    let mut pending_d = false; // "dd" の 1 キー目フラグ

    let (tx, mut rx) = mpsc::unbounded_channel::<AppMessage>();
    // 初期ロード
    spawn_fetch(state, &tx, client, databases);

    loop {
        while let Ok(msg) = rx.try_recv() {
            apply_message(state, msg, &tx, client, databases);
        }

        terminal.draw(|f| {
            let has_form = state.form.is_some();
            let root_constraints: Vec<Constraint> = if has_form {
                vec![
                    Constraint::Min(0),
                    Constraint::Length(5), // フォームパネル
                    Constraint::Length(1), // ステータスバー
                ]
            } else {
                vec![Constraint::Min(0), Constraint::Length(1)]
            };

            let root_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(root_constraints)
                .split(f.area());

            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(22), Constraint::Min(0)])
                .split(root_chunks[0]);

            ui::sidebar::render_sidebar(f, main_chunks[0], state);
            ui::week_view::render_week_view(f, main_chunks[1], state);

            if has_form {
                ui::form::render_form_panel(f, root_chunks[1], state);
                render_status_bar(f, root_chunks[2], state);
            } else {
                render_status_bar(f, root_chunks[1], state);
            }
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(KeyEvent { code, .. }) = event::read()? {
                // モードを clone して borrow checker を回避
                let current_mode = state.mode.clone();
                match current_mode {
                    app::AppMode::Normal => {
                        match code {
                            KeyCode::Char('q') => break,
                            KeyCode::Char('H') => {
                                pending_d = false;
                                state.prev_week();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('L') => {
                                pending_d = false;
                                state.next_week();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('j') => {
                                pending_d = false;
                                state.cursor_down();
                            }
                            KeyCode::Char('k') => {
                                pending_d = false;
                                state.cursor_up();
                            }
                            KeyCode::Char('h') => {
                                pending_d = false;
                                state.select_prev_day();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('l') => {
                                pending_d = false;
                                state.select_next_day();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('t') => {
                                pending_d = false;
                                state.go_to_today();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Tab => {
                                pending_d = false;
                                state.toggle_panel();
                            }
                            KeyCode::Char('n') => {
                                pending_d = false;
                                state.open_create_form();
                            }
                            KeyCode::Char('e') => {
                                pending_d = false;
                                if let Some(event) = state.event_at_cursor() {
                                    let (date_str, start_str, end_str) =
                                        event_to_form_strings(event);
                                    let id = event.id.clone();
                                    let title = event.title.clone();
                                    let is_all_day = event.is_all_day;
                                    let database_id = event.database_id.clone();
                                    state.open_edit_form(
                                        &id,
                                        &title,
                                        &date_str,
                                        is_all_day,
                                        &start_str,
                                        &end_str,
                                        &database_id,
                                    );
                                } else {
                                    state.status_message =
                                        Some("カーソル位置にイベントがありません".to_string());
                                }
                            }
                            KeyCode::Char('d') => {
                                if pending_d {
                                    pending_d = false;
                                    if let Some(event) = state.event_at_cursor() {
                                        let page_id = event.id.clone();
                                        let title = event.title.clone();
                                        state.mode = app::AppMode::Confirm(
                                            app::ConfirmAction::DeleteEvent(page_id),
                                        );
                                        state.status_message = Some(format!(
                                            "「{}」を削除しますか？ [y/N]",
                                            title
                                        ));
                                    } else {
                                        state.status_message = Some(
                                            "カーソル位置にイベントがありません".to_string(),
                                        );
                                    }
                                } else {
                                    pending_d = true;
                                }
                            }
                            KeyCode::Left => {
                                pending_d = false;
                                state.overlap_focus_left();
                            }
                            KeyCode::Right => {
                                pending_d = false;
                                state.overlap_focus_right();
                            }
                            _ => {
                                pending_d = false;
                            }
                        }
                    }
                    app::AppMode::Form => {
                        let db_count = state.databases.len();
                        match code {
                            KeyCode::Esc => state.close_form(),
                            KeyCode::Enter => {
                                handle_form_submit(state, &tx, client);
                            }
                            _ => {
                                if let Some(form) = state.form.as_mut() {
                                    match code {
                                        KeyCode::Tab => form.next_field(),
                                        KeyCode::BackTab => form.prev_field(),
                                        KeyCode::Char(' ') => form.toggle_all_day(),
                                        KeyCode::Char(c) => form.input_char(c),
                                        KeyCode::Backspace => form.delete_char(),
                                        KeyCode::Left => form.db_prev(db_count),
                                        KeyCode::Right => form.db_next(db_count),
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    app::AppMode::Confirm(_) => match code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            handle_delete_confirm(state, &tx, client);
                        }
                        _ => {
                            state.mode = app::AppMode::Normal;
                            state.status_message = None;
                        }
                    },
                }
            }
        }
    }
    Ok(())
}

fn render_status_bar(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let error = state.status_message.as_deref();
    let text = if matches!(state.mode, app::AppMode::Confirm(_)) {
        // Confirm モードはステータスバーに確認メッセージを表示（status_message に格納済み）
        error.unwrap_or("").to_string()
    } else {
        ui::status_bar_text(state.pending_requests > 0, error)
    };
    let style = if error.is_some() && !matches!(state.mode, app::AppMode::Confirm(_)) {
        Style::default().fg(Color::Red)
    } else if matches!(state.mode, app::AppMode::Confirm(_)) {
        Style::default().fg(Color::Yellow)
    } else if state.pending_requests > 0 {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}

fn event_to_form_strings(
    event: &api::models::NotionEvent,
) -> (String, String, String) {
    if event.is_all_day {
        let date = event
            .date_start
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        (date, "09:00".to_string(), "10:00".to_string())
    } else {
        let start_info = event.datetime_start.map(|dt| {
            let local = dt.with_timezone(&chrono::Local);
            (
                local.format("%Y-%m-%d").to_string(),
                local.format("%H:%M").to_string(),
            )
        });
        let end_str = event
            .datetime_end
            .map(|dt| dt.with_timezone(&chrono::Local).format("%H:%M").to_string())
            .unwrap_or_else(|| "10:00".to_string());
        match start_info {
            Some((date, start_time)) => (date, start_time, end_str),
            None => ("".to_string(), "09:00".to_string(), "10:00".to_string()),
        }
    }
}

fn handle_form_submit(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
) {
    let form = match state.form.clone() {
        Some(f) => f,
        None => return,
    };

    if let Some(err) = app::form_logic::validate_form(&form) {
        state.status_message = Some(err);
        return;
    }

    let db = match state.databases.get(form.db_index) {
        Some(d) => d.clone(),
        None => {
            state.status_message = Some("DBが選択されていません".to_string());
            return;
        }
    };

    let title_prop = db.title_property.clone().unwrap_or_else(|| "Name".to_string());
    let date_prop = db.date_property.clone().unwrap_or_else(|| "Date".to_string());
    let (date_start, date_end) = app::form_logic::form_to_date_strings(&form);

    // 即座にフォームを閉じてナビゲーションへ戻す
    state.close_form();
    state.begin_mutation();

    let tx = tx.clone();
    let client = client.clone();
    let mode = form.mode.clone();
    let title = form.title.clone();
    let editing_id = form.editing_event_id.clone();
    let db_id = db.id.clone();
    let select = db.create_profile.select.clone();

    tokio::spawn(async move {
        let result: anyhow::Result<()> = match mode {
            app::FormMode::Create => client
                .create_page(
                    &db_id,
                    &title,
                    &date_start,
                    date_end.as_deref(),
                    &title_prop,
                    &date_prop,
                    &select,
                )
                .await
                .map(|_| ()),
            app::FormMode::Edit => match editing_id {
                Some(id) => client
                    .update_page(
                        &id,
                        &title,
                        &date_start,
                        date_end.as_deref(),
                        &title_prop,
                        &date_prop,
                    )
                    .await
                    .map(|_| ()),
                None => Ok(()),
            },
        };
        let error = result.err().map(|e| format!("✗ {}", e));
        let _ = tx.send(AppMessage::MutationResult { error });
    });
}

fn handle_delete_confirm(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
) {
    let page_id = match &state.mode {
        app::AppMode::Confirm(app::ConfirmAction::DeleteEvent(id)) => id.clone(),
        _ => return,
    };

    state.mode = app::AppMode::Normal;
    state.status_message = None;
    state.begin_mutation();

    let tx = tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let error = client
            .archive_page(&page_id)
            .await
            .err()
            .map(|e| format!("✗ {}", e));
        let _ = tx.send(AppMessage::MutationResult { error });
    });
}

fn spawn_fetch(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
    databases: &[config::DatabaseConfig],
) {
    let req = state.plan_fetch();
    let tx = tx.clone();
    let client = client.clone();
    let databases = databases.to_vec();
    tokio::spawn(async move {
        let mut fetched: HashMap<chrono::NaiveDate, Vec<api::models::NotionEvent>> = HashMap::new();
        let mut error: Option<String> = None;
        let mut had_success = false;

        for db in &databases {
            match client.query_database(&db.id, &req.start, &req.end).await {
                Ok(pages) => {
                    had_success = true;
                    for page in &pages {
                        if let Some(mut event) = api::parse_event_with_keys(
                            page,
                            &db.id,
                            db.title_property.as_deref(),
                            db.date_property.as_deref(),
                        ) {
                            event.color = Some(db.color.clone());
                            let date = event.date_start.or_else(|| {
                                event
                                    .datetime_start
                                    .map(|dt| dt.with_timezone(&chrono::Local).date_naive())
                            });
                            if let Some(d) = date {
                                fetched.entry(d).or_default().push(event);
                            }
                        }
                    }
                }
                Err(e) => {
                    error = Some(format!("API Error: {}", e));
                }
            }
        }

        // 1件でも成功していれば成功分を反映（エラーは無視）
        let final_error = if had_success { None } else { error };
        let _ = tx.send(AppMessage::FetchResult {
            generation: req.generation,
            range: req.range,
            events: fetched,
            error: final_error,
        });
    });
}

fn apply_message(
    state: &mut AppState,
    msg: AppMessage,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
    databases: &[config::DatabaseConfig],
) {
    match msg {
        AppMessage::FetchResult {
            generation,
            range,
            events,
            error,
        } => {
            state.apply_fetch_result(generation, range, events, error);
        }
        AppMessage::MutationResult { error } => {
            state.finish_mutation(error.clone());
            if error.is_none() {
                // 変更が反映された最新データを取得
                spawn_fetch(state, tx, client, databases);
            }
        }
    }
}
