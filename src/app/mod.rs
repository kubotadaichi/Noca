use crate::api::models::NotionEvent;
use crate::config::DatabaseConfig;
use chrono::{Datelike, Local, NaiveDate};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum ActivePanel {
    Sidebar,
    Calendar,
}

#[derive(Debug)]
pub struct AppState {
    pub current_week_start: NaiveDate,
    pub selected_date: NaiveDate,
    pub events: HashMap<NaiveDate, Vec<NotionEvent>>,
    pub databases: Vec<DatabaseConfig>,
    pub active_panel: ActivePanel,
    pub loading: bool,
    pub status_message: Option<String>,
    pub scroll_offset: u16, // 時間スロットのスクロール位置（時間単位）
}

impl AppState {
    pub fn new(databases: Vec<DatabaseConfig>) -> Self {
        let today = Local::now().date_naive();
        let week_start = week_start_of(today);
        Self {
            current_week_start: week_start,
            selected_date: today,
            events: HashMap::new(),
            databases,
            active_panel: ActivePanel::Calendar,
            loading: false,
            status_message: None,
            scroll_offset: 7, // デフォルト07:00から表示
        }
    }

    pub fn next_week(&mut self) {
        self.current_week_start += chrono::Duration::weeks(1);
    }

    pub fn prev_week(&mut self) {
        self.current_week_start -= chrono::Duration::weeks(1);
    }

    pub fn select_next_day(&mut self) {
        self.selected_date += chrono::Duration::days(1);
    }

    pub fn select_prev_day(&mut self) {
        self.selected_date -= chrono::Duration::days(1);
    }

    pub fn go_to_today(&mut self) {
        let today = Local::now().date_naive();
        self.selected_date = today;
        self.current_week_start = week_start_of(today);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        if self.scroll_offset < 22 {
            self.scroll_offset += 1;
        }
    }

    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Sidebar => ActivePanel::Calendar,
            ActivePanel::Calendar => ActivePanel::Sidebar,
        };
    }

    pub fn week_dates(&self) -> Vec<NaiveDate> {
        (0..7)
            .map(|i| self.current_week_start + chrono::Duration::days(i))
            .collect()
    }

    pub fn events_for_date(&self, date: &NaiveDate) -> Vec<&NotionEvent> {
        self.events
            .get(date)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn replace_events(&mut self, events: HashMap<NaiveDate, Vec<NotionEvent>>) {
        let mut deduped = HashMap::new();
        for (date, date_events) in events {
            let mut seen_ids = HashSet::new();
            let mut unique_events = Vec::new();
            for event in date_events {
                if seen_ids.insert(event.id.clone()) {
                    unique_events.push(event);
                }
            }
            if !unique_events.is_empty() {
                deduped.insert(date, unique_events);
            }
        }
        self.events = deduped;
    }
}

pub fn week_start_of(date: NaiveDate) -> NaiveDate {
    let days_from_monday = date.weekday().num_days_from_monday();
    date - chrono::Duration::days(days_from_monday as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::models::NotionEvent;

    fn make_event(id: &str, title: &str, date: NaiveDate) -> NotionEvent {
        NotionEvent {
            id: id.to_string(),
            title: title.to_string(),
            date_start: Some(date),
            datetime_start: None,
            datetime_end: None,
            is_all_day: true,
            database_id: "db".to_string(),
            color: None,
        }
    }

    #[test]
    fn test_week_start_of_thursday() {
        let thursday = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap(); // 木曜
        let monday = week_start_of(thursday);
        assert_eq!(monday, NaiveDate::from_ymd_opt(2026, 3, 2).unwrap());
    }

    #[test]
    fn test_week_start_of_monday() {
        let monday = NaiveDate::from_ymd_opt(2026, 3, 2).unwrap();
        assert_eq!(week_start_of(monday), monday);
    }

    #[test]
    fn test_next_prev_week() {
        let mut state = AppState::new(vec![]);
        let initial = state.current_week_start;
        state.next_week();
        assert_eq!(state.current_week_start, initial + chrono::Duration::weeks(1));
        state.prev_week();
        assert_eq!(state.current_week_start, initial);
    }

    #[test]
    fn test_week_dates_returns_7_days() {
        let state = AppState::new(vec![]);
        let dates = state.week_dates();
        assert_eq!(dates.len(), 7);
    }

    #[test]
    fn test_toggle_panel() {
        let mut state = AppState::new(vec![]);
        assert_eq!(state.active_panel, ActivePanel::Calendar);
        state.toggle_panel();
        assert_eq!(state.active_panel, ActivePanel::Sidebar);
        state.toggle_panel();
        assert_eq!(state.active_panel, ActivePanel::Calendar);
    }

    #[test]
    fn test_replace_events_deduplicates_and_replaces_old_data() {
        let mut state = AppState::new(vec![]);
        let day = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();

        state
            .events
            .insert(day, vec![make_event("old", "Old event", day)]);

        let mut incoming = HashMap::new();
        incoming.insert(
            day,
            vec![
                make_event("e1", "Event 1", day),
                make_event("e1", "Event 1 duplicate", day),
                make_event("e2", "Event 2", day),
            ],
        );

        state.replace_events(incoming);

        let day_events = state.events.get(&day).unwrap();
        assert_eq!(day_events.len(), 2);
        assert!(day_events.iter().any(|e| e.id == "e1"));
        assert!(day_events.iter().any(|e| e.id == "e2"));
        assert!(!day_events.iter().any(|e| e.id == "old"));
    }
}
