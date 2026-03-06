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
        let date_events = state.events_for_date(date);
        let all_day_events: Vec<&str> = date_events
            .into_iter()
            .filter(|e| e.is_all_day)
            .map(|e| e.title.as_str())
            .collect();

        let text = all_day_events.join(", ");
        let event_color = state
            .events_for_date(date)
            .into_iter()
            .filter(|e| e.is_all_day)
            .next()
            .and_then(|e| e.color.as_deref())
            .map(crate::ui::color_from_str)
            .unwrap_or(Color::Cyan);
        let style = Style::default().fg(event_color);
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
    let current_hour = now.hour() as usize;
    let current_minute = now.minute() as usize;
    let current_slot = current_hour * 4 + current_minute / 15;

    let visible_slots = area.height as usize;
    let start_slot = state.scroll_offset as usize;
    let cursor_slot_start = state.cursor_hour as usize * 4;
    let cursor_slot_end = cursor_slot_start + 4;

    let mut constraints = vec![Constraint::Length(6)];
    constraints.extend(vec![Constraint::Min(1); 7]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let week_dates = state.week_dates();

    // 時間ラベル列: :00のスロットのみ時刻表示、それ以外は空白
    let time_labels: Vec<Line> = (start_slot..start_slot + visible_slots)
        .map(|s| {
            if s % 4 == 0 {
                let h = s / 4;
                let style = if h == current_hour {
                    Style::default().fg(Color::Red)
                } else if s >= cursor_slot_start && s < cursor_slot_end {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(format!("{:02}:00", h % 24), style))
            } else {
                Line::from("")
            }
        })
        .collect();
    f.render_widget(Paragraph::new(time_labels), cols[0]);

    for (col_idx, date) in week_dates.iter().enumerate() {
        let timed_events: Vec<_> = state
            .events_for_date(date)
            .into_iter()
            .filter(|e| !e.is_all_day)
            .collect();

        let slot_lines: Vec<Line> = (start_slot..start_slot + visible_slots)
            .map(|s| {
                let is_cursor_row =
                    s >= cursor_slot_start && s < cursor_slot_end && *date == state.selected_date;
                // 現在時刻インジケーター
                if *date == today && s == current_slot {
                    return Line::from(Span::styled(
                        "─".repeat(cols[col_idx + 1].width as usize),
                        Style::default().fg(Color::Red),
                    ));
                }

                let slot_h = s / 4;
                let slot_m = (s % 4) * 15;
                let slot_total_min = slot_h * 60 + slot_m; // スロット開始（分）
                let slot_end_min = slot_total_min + 15; // スロット終了（分）

                // このスロットに重なるイベントを探す
                let active_event = timed_events.iter().find(|e| {
                    if let Some(dt_start) = e.datetime_start {
                        let local_start = dt_start.with_timezone(&chrono::Local);
                        let start_total_min =
                            local_start.hour() as usize * 60 + local_start.minute() as usize;

                        // イベント終了時刻（なければ開始+1時間とみなす）
                        let end_total_min = if let Some(dt_end) = e.datetime_end {
                            let local_end = dt_end.with_timezone(&chrono::Local);
                            local_end.hour() as usize * 60 + local_end.minute() as usize
                        } else {
                            start_total_min + 60
                        };

                        // スロットに重なる: start < slot_end && end > slot_start
                        start_total_min < slot_end_min && end_total_min > slot_total_min
                    } else {
                        false
                    }
                });

                if let Some(ev) = active_event {
                    let event_color = ev
                        .color
                        .as_deref()
                        .map(crate::ui::color_from_str)
                        .unwrap_or(Color::Green);
                    let event_style_str = state
                        .databases
                        .iter()
                        .find(|db| db.id == ev.database_id)
                        .map(|db| db.event_style.as_str())
                        .unwrap_or("block");

                    // このスロットが開始スロットかどうか判定
                    let is_start_slot = if let Some(dt_start) = ev.datetime_start {
                        let local_start = dt_start.with_timezone(&chrono::Local);
                        let start_total_min =
                            local_start.hour() as usize * 60 + local_start.minute() as usize;
                        // 開始時刻がこのスロット内 [slot_total_min, slot_end_min)
                        start_total_min >= slot_total_min && start_total_min < slot_end_min
                    } else {
                        false
                    };

                    if is_start_slot {
                        // 開始スロット: タイトルを表示
                        let end_str = ev
                            .datetime_end
                            .map(|dt| {
                                let local = dt.with_timezone(&chrono::Local);
                                format!("–{:02}:{:02}", local.hour(), local.minute())
                            })
                            .unwrap_or_default();
                        let label = format!("{}{}", &ev.title, end_str);
                        let col_width = cols[col_idx + 1].width as usize;
                        let truncated = if label.len() > col_width {
                            label
                                .chars()
                                .take(col_width.saturating_sub(1))
                                .collect::<String>()
                                + "…"
                        } else {
                            label
                        };

                        match event_style_str {
                            "text" => Line::from(Span::styled(
                                truncated,
                                Style::default().fg(event_color).add_modifier(Modifier::BOLD),
                            )),
                            "bar" => Line::from(vec![
                                Span::styled("▌", Style::default().fg(event_color)),
                                Span::styled(
                                    truncated,
                                    Style::default().add_modifier(Modifier::BOLD),
                                ),
                            ]),
                            _ => Line::from(Span::styled(
                                truncated,
                                Style::default()
                                    .bg(event_color)
                                    .fg(Color::Black)
                                    .add_modifier(Modifier::BOLD),
                            )),
                        }
                    } else {
                        // 継続スロット: スタイルに応じたマーカーのみ
                        match event_style_str {
                            "bar" => {
                                Line::from(Span::styled("▌", Style::default().fg(event_color)))
                            }
                            "block" => Line::from(Span::styled(
                                " ".repeat(cols[col_idx + 1].width as usize),
                                Style::default().bg(event_color),
                            )),
                            _ => Line::from(""), // text: 継続行は空白
                        }
                    }
                } else if is_cursor_row {
                    Line::from(Span::styled(
                        " ".repeat(cols[col_idx + 1].width as usize),
                        Style::default().bg(Color::DarkGray),
                    ))
                } else {
                    Line::from("")
                }
            })
            .collect();

        f.render_widget(Paragraph::new(slot_lines), cols[col_idx + 1]);
    }
}
