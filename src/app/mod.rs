use crate::api::models::NotionEvent;
use crate::config::DatabaseConfig;
use chrono::{Datelike, Local, NaiveDate, Timelike};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub enum AppMode {
    Normal,
    Form,
    Confirm(ConfirmAction),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeleteEvent(String), // page_id
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormMode {
    Create,
    Edit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FormField {
    DbSelect,
    Title,
    Date,
    IsAllDay,
    StartTime,
    EndTime,
}

#[derive(Debug, Clone)]
pub struct EventForm {
    pub mode: FormMode,
    pub editing_event_id: Option<String>,
    pub title: String,
    pub date: String,       // "YYYY-MM-DD"
    pub is_all_day: bool,
    pub start_time: String, // "HH:MM"
    pub end_time: String,   // "HH:MM"
    pub focused_field: FormField,
    pub db_index: usize,
}

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
    pub scroll_offset: u16, // 時間スロットのスクロール位置（15分単位）
    pub cursor_hour: u32,
    pub mode: AppMode,
    pub form: Option<EventForm>,
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
            scroll_offset: 28, // デフォルト07:00から表示（7 * 4 = 28）
            cursor_hour: 9,
            mode: AppMode::Normal,
            form: None,
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
        if self.selected_date >= self.current_week_start + chrono::Duration::weeks(1) {
            self.current_week_start += chrono::Duration::weeks(1);
        }
    }

    pub fn select_prev_day(&mut self) {
        self.selected_date -= chrono::Duration::days(1);
        if self.selected_date < self.current_week_start {
            self.current_week_start -= chrono::Duration::weeks(1);
        }
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
        if self.scroll_offset < 88 {
            self.scroll_offset += 1;
        }
    }

    pub fn cursor_up(&mut self) {
        if self.cursor_hour > 0 {
            self.cursor_hour -= 1;
        }
        // カーソルが画面上端より上なら scroll_offset を追従
        let cursor_slot = self.cursor_hour * 4;
        if (cursor_slot as u16) < self.scroll_offset {
            self.scroll_offset = cursor_slot as u16;
        }
    }

    pub fn cursor_down(&mut self) {
        if self.cursor_hour < 23 {
            self.cursor_hour += 1;
        }
        // カーソルが画面下端より下なら scroll_offset を追従（20 スロット ≈ 5 時間と仮定）
        let cursor_bottom = (self.cursor_hour * 4 + 3) as u16;
        let visible: u16 = 20;
        if cursor_bottom >= self.scroll_offset + visible {
            self.scroll_offset = cursor_bottom.saturating_sub(visible - 1);
        }
        // 上限 88（22:00）
        if self.scroll_offset > 88 {
            self.scroll_offset = 88;
        }
    }

    pub fn toggle_panel(&mut self) {
        self.active_panel = match self.active_panel {
            ActivePanel::Sidebar => ActivePanel::Calendar,
            ActivePanel::Calendar => ActivePanel::Sidebar,
        };
    }

    pub fn open_create_form(&mut self) {
        let date_str = self.selected_date.format("%Y-%m-%d").to_string();
        let start_str = format!("{:02}:00", self.cursor_hour);
        let end_hour = (self.cursor_hour + 1).min(23);
        let end_str = format!("{:02}:00", end_hour);
        self.form = Some(EventForm {
            mode: FormMode::Create,
            editing_event_id: None,
            title: String::new(),
            date: date_str,
            is_all_day: false,
            start_time: start_str,
            end_time: end_str,
            focused_field: FormField::Title,
            db_index: 0,
        });
        self.mode = AppMode::Form;
    }

    pub fn open_edit_form(
        &mut self,
        event_id: &str,
        title: &str,
        date: &str,
        is_all_day: bool,
        start_time: &str,
        end_time: &str,
        database_id: &str,
    ) {
        let db_index = self
            .databases
            .iter()
            .position(|db| db.id == database_id)
            .unwrap_or(0);
        self.form = Some(EventForm {
            mode: FormMode::Edit,
            editing_event_id: Some(event_id.to_string()),
            title: title.to_string(),
            date: date.to_string(),
            is_all_day,
            start_time: start_time.to_string(),
            end_time: end_time.to_string(),
            focused_field: FormField::Title,
            db_index,
        });
        self.mode = AppMode::Form;
    }

    pub fn close_form(&mut self) {
        self.form = None;
        self.mode = AppMode::Normal;
    }

    pub fn event_at_cursor(&self) -> Option<&crate::api::models::NotionEvent> {
        let events = self.events_for_date(&self.selected_date);
        events.into_iter().find(|e| {
            if let Some(dt) = e.datetime_start {
                let local = dt.with_timezone(&chrono::Local);
                local.hour() as u32 == self.cursor_hour
            } else {
                false
            }
        })
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
    fn test_scroll_offset_initial_is_28() {
        let state = AppState::new(vec![]);
        assert_eq!(state.scroll_offset, 28); // 07:00 = 7 * 4
    }

    #[test]
    fn test_scroll_down_increments_by_one_slot() {
        let mut state = AppState::new(vec![]);
        let initial = state.scroll_offset;
        state.scroll_down();
        assert_eq!(state.scroll_offset, initial + 1);
    }

    #[test]
    fn test_scroll_down_caps_at_88() {
        let mut state = AppState::new(vec![]);
        state.scroll_offset = 88;
        state.scroll_down();
        assert_eq!(state.scroll_offset, 88); // 22:00 を超えない
    }

    #[test]
    fn test_scroll_up_decrements_by_one_slot() {
        let mut state = AppState::new(vec![]);
        state.scroll_offset = 10;
        state.scroll_up();
        assert_eq!(state.scroll_offset, 9);
    }

    #[test]
    fn test_scroll_up_does_not_underflow() {
        let mut state = AppState::new(vec![]);
        state.scroll_offset = 0;
        state.scroll_up();
        assert_eq!(state.scroll_offset, 0);
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

    #[test]
    fn test_select_next_day_follows_week() {
        let mut state = AppState::new(vec![]);
        let week_end = state.current_week_start + chrono::Duration::days(6);
        state.selected_date = week_end;
        let initial_week = state.current_week_start;

        state.select_next_day();

        assert_eq!(
            state.current_week_start,
            initial_week + chrono::Duration::weeks(1)
        );
    }

    #[test]
    fn test_select_prev_day_follows_week() {
        let mut state = AppState::new(vec![]);
        state.selected_date = state.current_week_start;
        let initial_week = state.current_week_start;

        state.select_prev_day();

        assert_eq!(
            state.current_week_start,
            initial_week - chrono::Duration::weeks(1)
        );
    }

    #[test]
    fn test_select_next_day_within_week_does_not_change_week() {
        let mut state = AppState::new(vec![]);
        state.selected_date = state.current_week_start + chrono::Duration::days(2);
        let initial_week = state.current_week_start;

        state.select_next_day();

        assert_eq!(state.current_week_start, initial_week);
    }

    #[test]
    fn test_default_mode_is_normal() {
        let state = AppState::new(vec![]);
        assert!(matches!(state.mode, AppMode::Normal));
    }

    #[test]
    fn test_default_cursor_hour_is_9() {
        let state = AppState::new(vec![]);
        assert_eq!(state.cursor_hour, 9);
    }

    #[test]
    fn test_cursor_up_clamps_at_0() {
        let mut state = AppState::new(vec![]);
        state.cursor_hour = 0;
        state.cursor_up();
        assert_eq!(state.cursor_hour, 0);
    }

    #[test]
    fn test_cursor_down_clamps_at_23() {
        let mut state = AppState::new(vec![]);
        state.cursor_hour = 23;
        state.cursor_down();
        assert_eq!(state.cursor_hour, 23);
    }

    #[test]
    fn test_cursor_down_increments() {
        let mut state = AppState::new(vec![]);
        state.cursor_hour = 10;
        state.cursor_down();
        assert_eq!(state.cursor_hour, 11);
    }

    #[test]
    fn test_cursor_up_decrements() {
        let mut state = AppState::new(vec![]);
        state.cursor_hour = 10;
        state.cursor_up();
        assert_eq!(state.cursor_hour, 9);
    }

    #[test]
    fn test_open_create_form_sets_mode_and_form() {
        let mut state = AppState::new(vec![]);
        state.open_create_form();
        assert!(matches!(state.mode, AppMode::Form));
        assert!(state.form.is_some());
    }

    #[test]
    fn test_close_form_resets_to_normal() {
        let mut state = AppState::new(vec![]);
        state.open_create_form();
        state.close_form();
        assert!(matches!(state.mode, AppMode::Normal));
        assert!(state.form.is_none());
    }

    #[test]
    fn test_open_create_form_presets_selected_date() {
        let mut state = AppState::new(vec![]);
        state.selected_date = chrono::NaiveDate::from_ymd_opt(2026, 3, 10).unwrap();
        state.cursor_hour = 14;
        state.open_create_form();
        let form = state.form.unwrap();
        assert_eq!(form.date, "2026-03-10");
        assert_eq!(form.start_time, "14:00");
        assert_eq!(form.end_time, "15:00");
    }

    #[test]
    fn test_event_at_cursor_returns_event() {
        use chrono::{Local, TimeZone};
        let mut state = AppState::new(vec![]);
        let today = state.selected_date;
        state.cursor_hour = 10;
        // 10:00 のイベントを挿入
        let dt_start = Local
            .from_local_datetime(&today.and_hms_opt(10, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&chrono::Utc);
        let event = crate::api::models::NotionEvent {
            id: "e1".to_string(),
            title: "会議".to_string(),
            date_start: None,
            datetime_start: Some(dt_start),
            datetime_end: None,
            is_all_day: false,
            database_id: "db".to_string(),
            color: None,
        };
        state.events.insert(today, vec![event]);
        assert!(state.event_at_cursor().is_some());
    }

    #[test]
    fn test_event_at_cursor_returns_none_when_no_event() {
        let mut state = AppState::new(vec![]);
        state.cursor_hour = 10;
        assert!(state.event_at_cursor().is_none());
    }
}
