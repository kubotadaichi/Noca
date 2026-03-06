use super::{EventForm, FormField};

/// フォームのバリデーション。エラーメッセージを返す（None = OK）
pub fn validate_form(form: &EventForm) -> Option<String> {
    if form.title.trim().is_empty() {
        return Some("タイトルを入力してください".to_string());
    }
    if chrono::NaiveDate::parse_from_str(&form.date, "%Y-%m-%d").is_err() {
        return Some("日付は YYYY-MM-DD 形式で入力してください".to_string());
    }
    if !form.is_all_day {
        let start = parse_hhmm(&form.start_time);
        let end = parse_hhmm(&form.end_time);
        match (start, end) {
            (Some(s), Some(e)) if s >= e => {
                return Some("終了時刻は開始時刻より後にしてください".to_string());
            }
            (None, _) => {
                return Some("開始時刻は HH:MM 形式で入力してください".to_string());
            }
            (_, None) => {
                return Some("終了時刻は HH:MM 形式で入力してください".to_string());
            }
            _ => {}
        }
    }
    None
}

fn parse_hhmm(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let h: u32 = parts[0].parse().ok()?;
    let m: u32 = parts[1].parse().ok()?;
    if h > 23 || m > 59 {
        return None;
    }
    Some(h * 60 + m)
}

/// フォームを Notion API 用の日付文字列に変換する
/// 返り値: (date_start, date_end)
pub fn form_to_date_strings(form: &EventForm) -> (String, Option<String>) {
    if form.is_all_day {
        (form.date.clone(), None)
    } else {
        let offset = *chrono::Local::now().offset();
        let offset_str = format_offset(offset.local_minus_utc());
        let start = format!("{}T{}:00{}", form.date, form.start_time, offset_str);
        let end = format!("{}T{}:00{}", form.date, form.end_time, offset_str);
        (start, Some(end))
    }
}

fn format_offset(seconds: i32) -> String {
    let sign = if seconds >= 0 { "+" } else { "-" };
    let abs = seconds.unsigned_abs();
    let h = abs / 3600;
    let m = (abs % 3600) / 60;
    format!("{}{:02}:{:02}", sign, h, m)
}

impl EventForm {
    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            FormField::DbSelect => FormField::Title,
            FormField::Title => FormField::Date,
            FormField::Date => FormField::IsAllDay,
            FormField::IsAllDay => {
                if self.is_all_day {
                    FormField::DbSelect
                } else {
                    FormField::StartTime
                }
            }
            FormField::StartTime => FormField::EndTime,
            FormField::EndTime => FormField::DbSelect,
        };
    }

    pub fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            FormField::DbSelect => FormField::EndTime,
            FormField::Title => FormField::DbSelect,
            FormField::Date => FormField::Title,
            FormField::IsAllDay => FormField::Date,
            FormField::StartTime => FormField::IsAllDay,
            FormField::EndTime => FormField::StartTime,
        };
    }

    pub fn input_char(&mut self, c: char) {
        match self.focused_field {
            FormField::Title => self.title.push(c),
            FormField::Date => {
                if self.date.len() >= 10 {
                    self.date.clear();
                }
                if self.date.len() < 10 {
                    self.date.push(c);
                }
            }
            FormField::StartTime => {
                if self.start_time.len() >= 5 {
                    self.start_time.clear();
                }
                if self.start_time.len() < 5 {
                    self.start_time.push(c);
                }
            }
            FormField::EndTime => {
                if self.end_time.len() >= 5 {
                    self.end_time.clear();
                }
                if self.end_time.len() < 5 {
                    self.end_time.push(c);
                }
            }
            _ => {}
        }
    }

    pub fn delete_char(&mut self) {
        match self.focused_field {
            FormField::Title => {
                self.title.pop();
            }
            FormField::Date => {
                self.date.pop();
            }
            FormField::StartTime => {
                self.start_time.pop();
            }
            FormField::EndTime => {
                self.end_time.pop();
            }
            _ => {}
        }
    }

    pub fn toggle_all_day(&mut self) {
        if self.focused_field == FormField::IsAllDay {
            self.is_all_day = !self.is_all_day;
        }
    }

    pub fn db_next(&mut self, db_count: usize) {
        if self.focused_field == FormField::DbSelect && db_count > 0 {
            self.db_index = (self.db_index + 1) % db_count;
        }
    }

    pub fn db_prev(&mut self, db_count: usize) {
        if self.focused_field == FormField::DbSelect && db_count > 0 {
            self.db_index = (self.db_index + db_count - 1) % db_count;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{EventForm, FormField, FormMode};

    fn make_form() -> EventForm {
        EventForm {
            mode: FormMode::Create,
            editing_event_id: None,
            title: "Test".to_string(),
            date: "2026-03-06".to_string(),
            is_all_day: false,
            start_time: "10:00".to_string(),
            end_time: "11:00".to_string(),
            focused_field: FormField::Title,
            db_index: 0,
        }
    }

    #[test]
    fn test_validate_empty_title() {
        let mut form = make_form();
        form.title = "".to_string();
        assert!(validate_form(&form).unwrap().contains("タイトル"));
    }

    #[test]
    fn test_validate_whitespace_title() {
        let mut form = make_form();
        form.title = "   ".to_string();
        assert!(validate_form(&form).is_some());
    }

    #[test]
    fn test_validate_bad_date() {
        let mut form = make_form();
        form.date = "2026/03/06".to_string();
        assert!(validate_form(&form).unwrap().contains("YYYY-MM-DD"));
    }

    #[test]
    fn test_validate_start_ge_end() {
        let mut form = make_form();
        form.start_time = "11:00".to_string();
        form.end_time = "10:00".to_string();
        assert!(validate_form(&form).unwrap().contains("終了時刻"));
    }

    #[test]
    fn test_validate_start_eq_end() {
        let mut form = make_form();
        form.start_time = "10:00".to_string();
        form.end_time = "10:00".to_string();
        assert!(validate_form(&form).is_some());
    }

    #[test]
    fn test_validate_ok() {
        let form = make_form();
        assert!(validate_form(&form).is_none());
    }

    #[test]
    fn test_validate_all_day_skips_time_check() {
        let mut form = make_form();
        form.is_all_day = true;
        form.start_time = "23:00".to_string();
        form.end_time = "01:00".to_string();
        assert!(validate_form(&form).is_none());
    }

    #[test]
    fn test_next_field_title_to_date() {
        let mut form = make_form();
        form.focused_field = FormField::Title;
        form.next_field();
        assert_eq!(form.focused_field, FormField::Date);
    }

    #[test]
    fn test_next_field_allday_on_skips_time() {
        let mut form = make_form();
        form.is_all_day = true;
        form.focused_field = FormField::IsAllDay;
        form.next_field();
        assert_eq!(form.focused_field, FormField::DbSelect);
    }

    #[test]
    fn test_next_field_allday_off_goes_to_start() {
        let mut form = make_form();
        form.is_all_day = false;
        form.focused_field = FormField::IsAllDay;
        form.next_field();
        assert_eq!(form.focused_field, FormField::StartTime);
    }

    #[test]
    fn test_input_char_appends_to_title() {
        let mut form = make_form();
        form.focused_field = FormField::Title;
        form.title = "".to_string();
        form.input_char('A');
        form.input_char('B');
        assert_eq!(form.title, "AB");
    }

    #[test]
    fn test_input_char_on_prefilled_start_time_replaces_from_empty() {
        let mut form = make_form();
        form.focused_field = FormField::StartTime;
        form.start_time = "09:00".to_string();
        form.input_char('1');
        assert_eq!(form.start_time, "1");
    }

    #[test]
    fn test_delete_char_removes_from_title() {
        let mut form = make_form();
        form.focused_field = FormField::Title;
        form.title = "AB".to_string();
        form.delete_char();
        assert_eq!(form.title, "A");
    }

    #[test]
    fn test_toggle_all_day() {
        let mut form = make_form();
        form.focused_field = FormField::IsAllDay;
        assert!(!form.is_all_day);
        form.toggle_all_day();
        assert!(form.is_all_day);
        form.toggle_all_day();
        assert!(!form.is_all_day);
    }

    #[test]
    fn test_toggle_all_day_only_when_focused() {
        let mut form = make_form();
        form.focused_field = FormField::Title; // IsAllDay にフォーカスなし
        form.toggle_all_day();
        assert!(!form.is_all_day); // 変化しない
    }

    #[test]
    fn test_db_next_wraps() {
        let mut form = make_form();
        form.focused_field = FormField::DbSelect;
        form.db_index = 1;
        form.db_next(2);
        assert_eq!(form.db_index, 0);
    }

    #[test]
    fn test_db_prev_wraps() {
        let mut form = make_form();
        form.focused_field = FormField::DbSelect;
        form.db_index = 0;
        form.db_prev(2);
        assert_eq!(form.db_index, 1);
    }

    #[test]
    fn test_form_to_date_strings_all_day() {
        let mut form = make_form();
        form.is_all_day = true;
        let (start, end) = form_to_date_strings(&form);
        assert_eq!(start, "2026-03-06");
        assert!(end.is_none());
    }

    #[test]
    fn test_form_to_date_strings_timed() {
        let form = make_form();
        let (start, end) = form_to_date_strings(&form);
        assert!(start.starts_with("2026-03-06T10:00:00"));
        assert!(end.is_some());
        assert!(end.unwrap().starts_with("2026-03-06T11:00:00"));
    }
}
