use crate::app::AppState;
use chrono::{Datelike, Local, Timelike};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

const DAY_NAMES: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];

pub fn render_week_view(f: &mut Frame, area: Rect, state: &AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // ヘッダー（曜日）
            Constraint::Length(2), // 終日イベント行
            Constraint::Min(0),    // 時間スロット
        ])
        .split(area);

    render_header(f, chunks[0], state);
    render_all_day_row(f, chunks[1], state);
    render_time_slots(f, chunks[2], state);
}

fn render_header(f: &mut Frame, area: Rect, state: &AppState) {
    let today = Local::now().date_naive();
    let week_dates = state.week_dates();

    let mut constraints = vec![Constraint::Length(6)];
    constraints.extend(vec![Constraint::Min(1); 7]);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    f.render_widget(Paragraph::new("  JST"), cols[0]);

    for (i, date) in week_dates.iter().enumerate() {
        let is_today = *date == today;
        let is_selected = *date == state.selected_date;
        let label = format!("{} {}", DAY_NAMES[i], date.day());
        let style = if is_today {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        f.render_widget(Paragraph::new(Span::styled(label, style)), cols[i + 1]);
    }
}

fn render_all_day_row(f: &mut Frame, area: Rect, state: &AppState) {
    let week_dates = state.week_dates();
    let mut constraints = vec![Constraint::Length(6)];
    constraints.extend(vec![Constraint::Min(1); 7]);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    f.render_widget(
        Paragraph::new("終日").style(Style::default().fg(Color::DarkGray)),
        cols[0],
    );

    for (i, date) in week_dates.iter().enumerate() {
        let all_day_events: Vec<&str> = state
            .events_for_date(date)
            .into_iter()
            .filter(|e| e.is_all_day)
            .map(|e| e.title.as_str())
            .collect();

        let text = all_day_events.join(", ");
        let style = Style::default().fg(Color::Cyan);
        f.render_widget(
            Paragraph::new(Span::styled(
                if text.len() > cols[i + 1].width as usize {
                    text.chars()
                        .take(cols[i + 1].width as usize - 1)
                        .collect::<String>()
                        + "…"
                } else {
                    text
                },
                style,
            )),
            cols[i + 1],
        );
    }
}

fn render_time_slots(f: &mut Frame, area: Rect, state: &AppState) {
    let now = Local::now();
    let today = now.date_naive();
    let current_hour = now.hour();

    let visible_hours = area.height as usize;
    let start_hour = state.scroll_offset as usize;

    let mut constraints = vec![Constraint::Length(6)];
    constraints.extend(vec![Constraint::Min(1); 7]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let week_dates = state.week_dates();

    let time_labels: Vec<Line> = (start_hour..start_hour + visible_hours)
        .map(|h| {
            let style = if h == current_hour as usize {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            Line::from(Span::styled(format!("{:02}:00", h % 24), style))
        })
        .collect();
    f.render_widget(Paragraph::new(time_labels), cols[0]);

    for (col_idx, date) in week_dates.iter().enumerate() {
        let timed_events: Vec<_> = state
            .events_for_date(date)
            .into_iter()
            .filter(|e| !e.is_all_day)
            .collect();

        let slot_lines: Vec<Line> = (start_hour..start_hour + visible_hours)
            .map(|h| {
                if *date == today && h == current_hour as usize {
                    return Line::from(Span::styled(
                        "─".repeat(cols[col_idx + 1].width as usize),
                        Style::default().fg(Color::Red),
                    ));
                }

                let event = timed_events.iter().find(|e| {
                    if let Some(dt) = e.datetime_start {
                        let local_dt = dt.with_timezone(&chrono::Local);
                        local_dt.hour() as usize == h
                    } else {
                        false
                    }
                });

                if let Some(ev) = event {
                    let end_str = ev
                        .datetime_end
                        .map(|dt| {
                            let local = dt.with_timezone(&chrono::Local);
                            format!("–{:02}:{:02}", local.hour(), local.minute())
                        })
                        .unwrap_or_default();
                    let label = format!("{}{}", &ev.title, end_str);
                    let truncated = if label.len() > cols[col_idx + 1].width as usize {
                        label
                            .chars()
                            .take(cols[col_idx + 1].width as usize - 1)
                            .collect::<String>()
                            + "…"
                    } else {
                        label
                    };
                    Line::from(Span::styled(
                        truncated,
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from("")
                }
            })
            .collect();

        f.render_widget(Paragraph::new(slot_lines), cols[col_idx + 1]);
    }
}
