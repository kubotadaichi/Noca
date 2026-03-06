use crate::app::{AppState, EventForm, FormField, FormMode};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

pub fn render_form_panel(f: &mut Frame, area: Rect, state: &AppState) {
    let Some(form) = &state.form else { return };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // DB 選択行
            Constraint::Length(1), // タイトル行
            Constraint::Length(1), // 日付・終日・時刻行
            Constraint::Length(1), // ヒント
        ])
        .split(inner);

    let db_names: Vec<&str> = state.databases.iter().map(|d| d.name.as_str()).collect();
    render_db_row(f, chunks[0], form, &db_names);
    render_title_row(f, chunks[1], form);
    render_datetime_row(f, chunks[2], form);
    render_hint_row(f, chunks[3]);
}

fn focused_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

fn render_db_row(f: &mut Frame, area: Rect, form: &EventForm, db_names: &[&str]) {
    let mode_label = match form.mode {
        FormMode::Create => "[新規]",
        FormMode::Edit => "[編集]",
    };
    let db_name = db_names.get(form.db_index).copied().unwrap_or("(DB なし)");
    let focused = form.focused_field == FormField::DbSelect;
    let line = Line::from(vec![
        Span::styled(format!("{} DB: ", mode_label), Style::default().fg(Color::Cyan)),
        Span::styled(format!(" {} ", db_name), focused_style(focused)),
        Span::styled("  ← → で切替", Style::default().fg(Color::DarkGray)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_title_row(f: &mut Frame, area: Rect, form: &EventForm) {
    let focused = form.focused_field == FormField::Title;
    let display = if form.title.is_empty() && !focused {
        "(タイトルを入力)".to_string()
    } else {
        form.title.clone()
    };
    let line = Line::from(vec![
        Span::raw("タイトル: "),
        Span::styled(format!(" {} ", display), focused_style(focused)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_datetime_row(f: &mut Frame, area: Rect, form: &EventForm) {
    let date_focused = form.focused_field == FormField::Date;
    let allday_focused = form.focused_field == FormField::IsAllDay;
    let start_focused = form.focused_field == FormField::StartTime;
    let end_focused = form.focused_field == FormField::EndTime;

    let allday_check = if form.is_all_day { "[x]" } else { "[ ]" };
    let time_style = if form.is_all_day {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let mut spans = vec![
        Span::raw("日付: "),
        Span::styled(format!(" {} ", form.date), focused_style(date_focused)),
        Span::raw("  終日: "),
        Span::styled(format!(" {} ", allday_check), focused_style(allday_focused)),
    ];

    if !form.is_all_day {
        spans.push(Span::raw("  開始: "));
        spans.push(Span::styled(
            format!(" {} ", form.start_time),
            if start_focused {
                focused_style(true)
            } else {
                time_style
            },
        ));
        spans.push(Span::raw("  終了: "));
        spans.push(Span::styled(
            format!(" {} ", form.end_time),
            if end_focused {
                focused_style(true)
            } else {
                time_style
            },
        ));
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_hint_row(f: &mut Frame, area: Rect) {
    let line = Line::from(Span::styled(
        "Tab: 移動  Space: 終日切替  Enter: 確定  Esc: キャンセル",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(Paragraph::new(line), area);
}
