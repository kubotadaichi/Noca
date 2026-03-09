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

fn build_cursor_cell_text(width: usize, label: Option<&str>) -> String {
    if width == 0 {
        return String::new();
    }
    let base = format!(">{}", label.unwrap_or_default());
    let len = base.chars().count();
    if len >= width {
        base.chars().take(width).collect()
    } else {
        format!("{base:<width$}")
    }
}

/// スロット（15分）に重なるイベントをすべて返す
fn events_in_slot<'a>(
    timed_events: &[&'a crate::api::models::NotionEvent],
    slot_total_min: usize,
) -> Vec<&'a crate::api::models::NotionEvent> {
    let slot_end_min = slot_total_min + 15;
    timed_events
        .iter()
        .filter(|e| {
            if let Some(dt_start) = e.datetime_start {
                let local_start = dt_start.with_timezone(&chrono::Local);
                let start_min = local_start.hour() as usize * 60 + local_start.minute() as usize;
                let end_min = if let Some(dt_end) = e.datetime_end {
                    let local_end = dt_end.with_timezone(&chrono::Local);
                    local_end.hour() as usize * 60 + local_end.minute() as usize
                } else {
                    start_min + 60
                };
                start_min < slot_end_min && end_min > slot_total_min
            } else {
                false
            }
        })
        .copied()
        .collect()
}

/// イベント1件分のスロット表示テキストを構築する
fn build_event_span(
    ev: &crate::api::models::NotionEvent,
    slot_total_min: usize,
    col_width: usize,
    is_cursor_row: bool,
    cursor_style: ratatui::style::Style,
    databases: &[crate::config::DatabaseConfig],
) -> ratatui::text::Span<'static> {
    if col_width == 0 {
        return Span::raw("");
    }
    let event_color = ev
        .color
        .as_deref()
        .map(crate::ui::color_from_str)
        .unwrap_or(Color::Green);
    let event_style_str = databases
        .iter()
        .find(|db| db.id == ev.database_id)
        .map(|db| db.event_style.as_str())
        .unwrap_or("block");

    let slot_end_min = slot_total_min + 15;
    let is_start_slot = if let Some(dt_start) = ev.datetime_start {
        let local_start = dt_start.with_timezone(&chrono::Local);
        let start_min = local_start.hour() as usize * 60 + local_start.minute() as usize;
        start_min >= slot_total_min && start_min < slot_end_min
    } else {
        false
    };

    if is_start_slot {
        let end_str = ev
            .datetime_end
            .map(|dt| {
                let local = dt.with_timezone(&chrono::Local);
                format!("–{:02}:{:02}", local.hour(), local.minute())
            })
            .unwrap_or_default();
        let label = format!("{}{}", &ev.title, end_str);
        let truncated: String = if label.chars().count() > col_width {
            label
                .chars()
                .take(col_width.saturating_sub(1))
                .collect::<String>()
                + "…"
        } else {
            format!("{:<width$}", label, width = col_width)
        };

        if is_cursor_row {
            return Span::styled(build_cursor_cell_text(col_width, Some(&truncated)), cursor_style);
        }

        match event_style_str {
            "text" => Span::styled(
                truncated,
                Style::default()
                    .fg(event_color)
                    .add_modifier(Modifier::BOLD),
            ),
            "bar" => {
                let body: String = truncated.chars().take(col_width.saturating_sub(1)).collect();
                Span::styled(
                    format!("▌{}", body),
                    Style::default().fg(event_color).add_modifier(Modifier::BOLD),
                )
            }
            _ => Span::styled(
                truncated,
                Style::default()
                    .bg(event_color)
                    .fg(Color::Black)
                    .add_modifier(Modifier::BOLD),
            ),
        }
    } else {
        // 継続スロット
        if is_cursor_row {
            return Span::styled(build_cursor_cell_text(col_width, None), cursor_style);
        }
        match event_style_str {
            "bar" => Span::styled(
                format!("{:<width$}", "▌", width = col_width),
                Style::default().fg(event_color),
            ),
            "block" => Span::styled(" ".repeat(col_width), Style::default().bg(event_color)),
            _ => Span::raw(" ".repeat(col_width)),
        }
    }
}

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
    let cursor_style = Style::default()
        .bg(Color::Yellow)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);
    let time_labels: Vec<Line> = (start_slot..start_slot + visible_slots)
        .map(|s| {
            if s >= cursor_slot_start && s < cursor_slot_end {
                let label = if s % 4 == 0 {
                    format!("{:02}:00", (s / 4) % 24)
                } else {
                    String::new()
                };
                Line::from(Span::styled(
                    build_cursor_cell_text(cols[0].width as usize, Some(&label)),
                    cursor_style,
                ))
            } else if s % 4 == 0 {
                let h = s / 4;
                let style = if h == current_hour {
                    Style::default().fg(Color::Red)
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
                let col_width = cols[col_idx + 1].width as usize;
                // 現在時刻インジケーター
                if *date == today && s == current_slot {
                    return Line::from(Span::styled(
                        "─".repeat(col_width),
                        Style::default().fg(Color::Red),
                    ));
                }

                let slot_h = s / 4;
                let slot_m = (s % 4) * 15;
                let slot_total_min = slot_h * 60 + slot_m;

                let slot_events = events_in_slot(&timed_events, slot_total_min);

                match slot_events.len() {
                    0 => {
                        if is_cursor_row {
                            Line::from(Span::styled(
                                build_cursor_cell_text(col_width, None),
                                cursor_style,
                            ))
                        } else {
                            Line::from("")
                        }
                    }
                    1 => {
                        let ev = slot_events[0];
                        let span = build_event_span(
                            ev,
                            slot_total_min,
                            col_width,
                            is_cursor_row,
                            cursor_style,
                            &state.databases,
                        );
                        Line::from(span)
                    }
                    n => {
                        // 2件以上: 左半分にイベント0、右半分にイベント1（または +N）
                        let left_w = col_width / 2;
                        let right_w = col_width - left_w;

                        let (left_cursor, right_cursor) = if is_cursor_row {
                            (state.overlap_focus == 0, state.overlap_focus == 1)
                        } else {
                            (false, false)
                        };

                        let left_span = build_event_span(
                            slot_events[0],
                            slot_total_min,
                            left_w,
                            left_cursor,
                            cursor_style,
                            &state.databases,
                        );

                        let right_span = if n == 2 {
                            build_event_span(
                                slot_events[1],
                                slot_total_min,
                                right_w,
                                right_cursor,
                                cursor_style,
                                &state.databases,
                            )
                        } else if right_w == 0 {
                            Span::raw("")
                        } else {
                            let overflow = n - 1;
                            let label = format!("+{}", overflow);
                            let padded = format!("{:<width$}", label, width = right_w);
                            if right_cursor {
                                Span::styled(padded, cursor_style)
                            } else {
                                Span::styled(padded, Style::default().fg(Color::DarkGray))
                            }
                        };

                        Line::from(vec![left_span, right_span])
                    }
                }
            })
            .collect();

        f.render_widget(Paragraph::new(slot_lines), cols[col_idx + 1]);
    }
}

#[cfg(test)]
mod tests {
    use super::build_cursor_cell_text;

    #[test]
    fn test_build_cursor_cell_text_has_marker() {
        let text = build_cursor_cell_text(8, Some("Task"));
        assert!(text.starts_with(">"));
    }

    #[test]
    fn test_build_cursor_cell_text_truncates_to_width() {
        let text = build_cursor_cell_text(4, Some("ABCDE"));
        assert_eq!(text.chars().count(), 4);
    }
}
