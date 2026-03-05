use crate::app::AppState;
use chrono::{Datelike, Local, NaiveDate};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

pub fn render_sidebar(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // ミニ月カレンダー
            Constraint::Min(0),     // DBリスト
        ])
        .split(area);

    render_mini_calendar(f, chunks[0], state);
    render_db_list(f, chunks[1], state);
}

fn render_mini_calendar(f: &mut Frame, area: Rect, state: &AppState) {
    let today = Local::now().date_naive();
    let selected = state.selected_date;
    let year = selected.year();
    let month = selected.month();

    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let days_from_monday = first_day.weekday().num_days_from_monday();

    let mut lines = vec![
        Line::from(Span::styled(
            format!("  {} {}月", year, month),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from("月 火 水 木 金 土 日"),
    ];

    let days_in_month = days_in_month(year, month);
    let mut day = 1u32;
    let mut week_spans = vec![Span::raw("   ".repeat(days_from_monday as usize))];

    while day <= days_in_month {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        let weekday = date.weekday().num_days_from_monday();

        let style = if date == today {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if date == selected {
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        week_spans.push(Span::styled(format!("{:2}", day), style));

        if weekday == 6 || day == days_in_month {
            lines.push(Line::from(week_spans.clone()));
            week_spans = vec![];
        } else {
            week_spans.push(Span::raw(" "));
        }

        day += 1;
    }

    let paragraph = Paragraph::new(lines).block(Block::default().borders(Borders::BOTTOM));
    f.render_widget(paragraph, area);
}

fn render_db_list(f: &mut Frame, area: Rect, state: &AppState) {
    let items: Vec<ListItem> = state
        .databases
        .iter()
        .map(|db| {
            let color = crate::ui::color_from_str(&db.color);
            ListItem::new(Line::from(vec![
                Span::styled("■ ", Style::default().fg(color)),
                Span::raw(&db.name),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().title("Notion").borders(Borders::NONE));
    f.render_widget(list, area);
}

fn days_in_month(year: i32, month: u32) -> u32 {
    if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)
    }
    .unwrap()
    .signed_duration_since(NaiveDate::from_ymd_opt(year, month, 1).unwrap())
    .num_days() as u32
}
