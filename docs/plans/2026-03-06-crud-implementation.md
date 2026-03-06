# Noca CRUD Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Notion カレンダー TUI に週ビューからキーボードのみでイベントを作成・編集・削除できる CRUD 機能を実装する。

**Architecture:** AppMode（Normal/Form/Confirm）で状態管理。下部フォームパネルで入力、`dd`+確認でアーカイブ削除。API に create_page/update_page/archive_page を追加し、成功後は fetch_events で再取得する。スクロールは既存の 15 分スロット単位を維持し、cursor_hour（時間単位）を追加してイベント選択に使う。

**Tech Stack:** Rust, ratatui, crossterm, reqwest, tokio, serde_json, chrono, anyhow

---

### Task 1: AppMode・EventForm・cursor_hour を AppState に追加

**Files:**
- Modify: `src/app/mod.rs`

**Step 1: テストを書く**

`src/app/mod.rs` の `#[cfg(test)]` に追加:

```rust
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
        .from_local_datetime(
            &today.and_hms_opt(10, 0, 0).unwrap(),
        )
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
```

**Step 2: テスト失敗確認**

```bash
cargo test app
```
Expected: エラー（AppMode, cursor_hour 等が未定義）

**Step 3: 実装**

`src/app/mod.rs` の既存 `use` 文の後、`#[derive(Debug, Clone, PartialEq)] pub enum ActivePanel` の前に以下を追加:

```rust
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
    pub date: String,        // "YYYY-MM-DD"
    pub is_all_day: bool,
    pub start_time: String,  // "HH:MM"
    pub end_time: String,    // "HH:MM"
    pub focused_field: FormField,
    pub db_index: usize,
}
```

`AppState` 構造体に以下のフィールドを追加:

```rust
pub cursor_hour: u32,
pub mode: AppMode,
pub form: Option<EventForm>,
```

`AppState::new()` の `Self { ... }` に追加:

```rust
cursor_hour: 9,
mode: AppMode::Normal,
form: None,
```

`impl AppState` に以下のメソッドを追加:

```rust
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
```

**Step 4: テスト実行**

```bash
cargo test app
```
Expected: 全テスト pass

**Step 5: ビルド確認**

```bash
cargo build
```

**Step 6: Commit**

```bash
git add src/app/mod.rs
git commit -m "feat: add AppMode, EventForm, cursor_hour to AppState"
```

---

### Task 2: form_logic モジュール（バリデーション・フォーム操作）

**Files:**
- Create: `src/app/form_logic.rs`
- Modify: `src/app/mod.rs`

**Step 1: テストから書く**

`src/app/form_logic.rs` を作成（テストを最初に書き、実装は後）:

```rust
use super::{EventForm, FormField, FormMode};

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
        use chrono::FixedOffset;
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
                if self.date.len() < 10 {
                    self.date.push(c);
                }
            }
            FormField::StartTime => {
                if self.start_time.len() < 5 {
                    self.start_time.push(c);
                }
            }
            FormField::EndTime => {
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
```

**Step 2: テスト失敗確認**

```bash
cargo test app::form_logic
```
Expected: エラー（モジュール未登録）

**Step 3: app/mod.rs に mod 宣言を追加**

`src/app/mod.rs` の先頭 `use` 文の後に追加:

```rust
pub mod form_logic;
```

**Step 4: テスト実行**

```bash
cargo test app
```
Expected: 全テスト pass

**Step 5: Commit**

```bash
git add src/app/form_logic.rs src/app/mod.rs
git commit -m "feat: add EventForm validation and input logic"
```

---

### Task 3: API - create_page / update_page / archive_page

**Files:**
- Modify: `src/api/mod.rs`

**Step 1: テストを書く**

`src/api/mod.rs` の `#[cfg(test)]` に追加:

```rust
#[test]
fn test_build_create_body_all_day() {
    let body = build_create_body("db-id", "Meeting", "2026-03-06", None, "Name", "Date");
    assert_eq!(body["parent"]["database_id"], "db-id");
    assert_eq!(
        body["properties"]["Name"]["title"][0]["text"]["content"],
        "Meeting"
    );
    assert_eq!(body["properties"]["Date"]["date"]["start"], "2026-03-06");
    assert!(body["properties"]["Date"]["date"]["end"].is_null());
}

#[test]
fn test_build_create_body_timed() {
    let body = build_create_body(
        "db-id",
        "Meeting",
        "2026-03-06T10:00:00+09:00",
        Some("2026-03-06T11:00:00+09:00"),
        "Name",
        "Date",
    );
    assert_eq!(
        body["properties"]["Date"]["date"]["start"],
        "2026-03-06T10:00:00+09:00"
    );
    assert_eq!(
        body["properties"]["Date"]["date"]["end"],
        "2026-03-06T11:00:00+09:00"
    );
}

#[test]
fn test_build_update_body() {
    let body = build_update_body("Updated", "2026-03-07", None, "Name", "Date");
    assert_eq!(
        body["properties"]["Name"]["title"][0]["text"]["content"],
        "Updated"
    );
    assert_eq!(body["properties"]["Date"]["date"]["start"], "2026-03-07");
    // update_body に parent はない
    assert!(body.get("parent").is_none());
}

#[test]
fn test_build_create_body_uses_custom_props() {
    let body = build_create_body("db", "Task", "2026-03-06", None, "タスク名", "開催日");
    assert!(body["properties"]["タスク名"]["title"].is_array());
    assert!(body["properties"]["開催日"]["date"]["start"].is_string());
}
```

**Step 2: テスト失敗確認**

```bash
cargo test api
```
Expected: エラー（build_create_body 未定義）

**Step 3: 実装**

`src/api/mod.rs` に以下を追加（既存の `fn build_query_body` の後）:

```rust
fn build_create_body(
    database_id: &str,
    title: &str,
    date_start: &str,
    date_end: Option<&str>,
    title_prop: &str,
    date_prop: &str,
) -> serde_json::Value {
    let date_value = if let Some(end) = date_end {
        json!({ "start": date_start, "end": end })
    } else {
        json!({ "start": date_start })
    };
    json!({
        "parent": { "database_id": database_id },
        "properties": {
            title_prop: {
                "title": [{ "text": { "content": title } }]
            },
            date_prop: {
                "date": date_value
            }
        }
    })
}

fn build_update_body(
    title: &str,
    date_start: &str,
    date_end: Option<&str>,
    title_prop: &str,
    date_prop: &str,
) -> serde_json::Value {
    let date_value = if let Some(end) = date_end {
        json!({ "start": date_start, "end": end })
    } else {
        json!({ "start": date_start })
    };
    json!({
        "properties": {
            title_prop: {
                "title": [{ "text": { "content": title } }]
            },
            date_prop: {
                "date": date_value
            }
        }
    })
}
```

`impl NotionClient` に以下を追加:

```rust
pub async fn create_page(
    &self,
    database_id: &str,
    title: &str,
    date_start: &str,
    date_end: Option<&str>,
    title_prop: &str,
    date_prop: &str,
) -> Result<String> {
    let url = format!("{}/pages", NOTION_API_BASE);
    let body = build_create_body(database_id, title, date_start, date_end, title_prop, date_prop);
    let response = self
        .client
        .post(&url)
        .header("Authorization", format!("Bearer {}", self.token))
        .header("Notion-Version", NOTION_VERSION)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Notion APIへの接続に失敗しました")?;

    let status = response.status();
    let raw = response.text().await.context("レスポンス読み取り失敗")?;
    if !status.is_success() {
        let err: serde_json::Value = serde_json::from_str(&raw).unwrap_or(json!({}));
        let msg = err["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow!("ページ作成に失敗しました ({}): {}", status, msg));
    }
    let page: serde_json::Value =
        serde_json::from_str(&raw).context("レスポンスのパース失敗")?;
    let id = page["id"].as_str().context("ページIDが取得できませんでした")?;
    Ok(id.to_string())
}

pub async fn update_page(
    &self,
    page_id: &str,
    title: &str,
    date_start: &str,
    date_end: Option<&str>,
    title_prop: &str,
    date_prop: &str,
) -> Result<()> {
    let url = format!("{}/pages/{}", NOTION_API_BASE, page_id);
    let body = build_update_body(title, date_start, date_end, title_prop, date_prop);
    let response = self
        .client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", self.token))
        .header("Notion-Version", NOTION_VERSION)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Notion APIへの接続に失敗しました")?;

    let status = response.status();
    if !status.is_success() {
        let raw = response.text().await.unwrap_or_default();
        let err: serde_json::Value = serde_json::from_str(&raw).unwrap_or(json!({}));
        let msg = err["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow!("ページ更新に失敗しました ({}): {}", status, msg));
    }
    Ok(())
}

pub async fn archive_page(&self, page_id: &str) -> Result<()> {
    let url = format!("{}/pages/{}", NOTION_API_BASE, page_id);
    let body = json!({ "archived": true });
    let response = self
        .client
        .patch(&url)
        .header("Authorization", format!("Bearer {}", self.token))
        .header("Notion-Version", NOTION_VERSION)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .context("Notion APIへの接続に失敗しました")?;

    let status = response.status();
    if !status.is_success() {
        let raw = response.text().await.unwrap_or_default();
        let err: serde_json::Value = serde_json::from_str(&raw).unwrap_or(json!({}));
        let msg = err["message"].as_str().unwrap_or("unknown error");
        return Err(anyhow!("ページ削除に失敗しました ({}): {}", status, msg));
    }
    Ok(())
}
```

**Step 4: テスト実行**

```bash
cargo test api
```
Expected: 全テスト pass

**Step 5: ビルド確認**

```bash
cargo build
```

**Step 6: Commit**

```bash
git add src/api/mod.rs
git commit -m "feat: add create/update/archive Notion API methods"
```

---

### Task 4: ui/form.rs フォームパネルレンダリング

**Files:**
- Create: `src/ui/form.rs`
- Modify: `src/ui/mod.rs`

**Step 1: 実装**

`src/ui/form.rs` を作成:

```rust
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
            if start_focused { focused_style(true) } else { time_style },
        ));
        spans.push(Span::raw("  終了: "));
        spans.push(Span::styled(
            format!(" {} ", form.end_time),
            if end_focused { focused_style(true) } else { time_style },
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
```

**Step 2: ui/mod.rs に追加**

`src/ui/mod.rs` に追加:

```rust
pub mod form;
pub mod sidebar;
pub mod week_view;
```

**Step 3: ビルド確認**

```bash
cargo build
```
Expected: Finished（警告は無視可）

**Step 4: Commit**

```bash
git add src/ui/form.rs src/ui/mod.rs
git commit -m "feat: add form panel UI widget"
```

---

### Task 5: week_view.rs カーソル描画

**Files:**
- Modify: `src/ui/week_view.rs`

**Step 1: render_time_slots にカーソルハイライトを追加**

`src/ui/week_view.rs` の `render_time_slots` 関数内を以下のように変更する。

変更 1: `visible_slots` 定義の直後（`let start_slot = ...` の後）に追加:

```rust
let cursor_slot_start = state.cursor_hour as usize * 4;
let cursor_slot_end = cursor_slot_start + 4;
```

変更 2: 時間ラベル列のスタイル判定（現在 `if s % 4 == 0` のブロック内）を以下に置き換え:

```rust
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
```

変更 3: 各日カラムのスロット描画で、カーソル行に背景色を追加する。
`let slot_lines: Vec<Line> = (start_slot..start_slot + visible_slots).map(|s| {` の直後に:

```rust
let is_cursor_row = s >= cursor_slot_start
    && s < cursor_slot_end
    && *date == state.selected_date;
```

そして `} else { Line::from("") }` の部分（イベントがないスロットの else 節）を:

```rust
} else if is_cursor_row {
    Line::from(Span::styled(
        " ".repeat(cols[col_idx + 1].width as usize),
        Style::default().bg(Color::DarkGray),
    ))
} else {
    Line::from("")
}
```

**Step 2: ビルド確認**

```bash
cargo build
```
Expected: Finished

**Step 3: Commit**

```bash
git add src/ui/week_view.rs
git commit -m "feat: add cursor row highlighting to week view"
```

---

### Task 6: main.rs キー統合

**Files:**
- Modify: `src/main.rs`

**Step 1: 実装**

`src/main.rs` 全体を以下に置き換える:

```rust
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
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::Paragraph,
    Terminal,
};
use std::collections::HashMap;
use std::io;
use std::time::Duration;

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

    fetch_events(&client, &mut state, &cfg.databases).await;

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

    loop {
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
                            KeyCode::Char('h') => {
                                pending_d = false;
                                state.prev_week();
                                fetch_events(client, state, databases).await;
                            }
                            KeyCode::Char('l') => {
                                pending_d = false;
                                state.next_week();
                                fetch_events(client, state, databases).await;
                            }
                            KeyCode::Char('j') => {
                                pending_d = false;
                                state.cursor_down();
                            }
                            KeyCode::Char('k') => {
                                pending_d = false;
                                state.cursor_up();
                            }
                            KeyCode::Char('H') => {
                                pending_d = false;
                                let week_before = state.current_week_start;
                                state.select_prev_day();
                                if state.current_week_start != week_before {
                                    fetch_events(client, state, databases).await;
                                }
                            }
                            KeyCode::Char('L') => {
                                pending_d = false;
                                let week_before = state.current_week_start;
                                state.select_next_day();
                                if state.current_week_start != week_before {
                                    fetch_events(client, state, databases).await;
                                }
                            }
                            KeyCode::Char('t') => {
                                pending_d = false;
                                state.go_to_today();
                                fetch_events(client, state, databases).await;
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
                                handle_form_submit(client, state, databases).await;
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
                            handle_delete_confirm(client, state, databases).await;
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
        ui::status_bar_text(state.loading, error)
    };
    let style = if error.is_some() && !matches!(state.mode, app::AppMode::Confirm(_)) {
        Style::default().fg(Color::Red)
    } else if matches!(state.mode, app::AppMode::Confirm(_)) {
        Style::default().fg(Color::Yellow)
    } else if state.loading {
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

async fn handle_form_submit(
    client: &api::NotionClient,
    state: &mut AppState,
    databases: &[config::DatabaseConfig],
) {
    let form = match state.form.clone() {
        Some(f) => f,
        None => return,
    };

    if let Some(err) = app::form_logic::validate_form(&form) {
        state.status_message = Some(err);
        return;
    }

    let db = match databases.get(form.db_index) {
        Some(d) => d,
        None => {
            state.status_message = Some("DBが選択されていません".to_string());
            return;
        }
    };

    let title_prop = db.title_property.as_deref().unwrap_or("Name");
    let date_prop = db.date_property.as_deref().unwrap_or("Date");
    let (date_start, date_end) = app::form_logic::form_to_date_strings(&form);

    match form.mode {
        app::FormMode::Create => {
            match client
                .create_page(
                    &db.id,
                    &form.title,
                    &date_start,
                    date_end.as_deref(),
                    title_prop,
                    date_prop,
                )
                .await
            {
                Ok(_) => {
                    state.close_form();
                    fetch_events(client, state, databases).await;
                }
                Err(e) => {
                    state.status_message = Some(format!("✗ {}", e));
                }
            }
        }
        app::FormMode::Edit => {
            let page_id = match &form.editing_event_id {
                Some(id) => id.clone(),
                None => return,
            };
            match client
                .update_page(
                    &page_id,
                    &form.title,
                    &date_start,
                    date_end.as_deref(),
                    title_prop,
                    date_prop,
                )
                .await
            {
                Ok(_) => {
                    state.close_form();
                    fetch_events(client, state, databases).await;
                }
                Err(e) => {
                    state.status_message = Some(format!("✗ {}", e));
                }
            }
        }
    }
}

async fn handle_delete_confirm(
    client: &api::NotionClient,
    state: &mut AppState,
    databases: &[config::DatabaseConfig],
) {
    let page_id = match &state.mode {
        app::AppMode::Confirm(app::ConfirmAction::DeleteEvent(id)) => id.clone(),
        _ => return,
    };

    state.mode = app::AppMode::Normal;
    state.status_message = None;

    match client.archive_page(&page_id).await {
        Ok(_) => {
            fetch_events(client, state, databases).await;
        }
        Err(e) => {
            state.status_message = Some(format!("✗ {}", e));
        }
    }
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
    let mut fetched_events: HashMap<chrono::NaiveDate, Vec<api::models::NotionEvent>> =
        HashMap::new();
    let mut had_success = false;

    for db in databases {
        match client.query_database(&db.id, &start_str, &end_str).await {
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
                            fetched_events.entry(d).or_default().push(event);
                        }
                    }
                }
            }
            Err(e) => {
                state.status_message = Some(format!("API Error: {}", e));
            }
        }
    }

    if had_success {
        state.replace_events(fetched_events);
        state.status_message = None;
    }

    state.loading = false;
}
```

**Step 2: ビルド確認**

```bash
cargo build
```
Expected: Finished（警告は無視可）

**Step 3: 全テスト実行**

```bash
cargo test
```
Expected: 全テスト pass

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat: integrate CRUD keybindings and form submit/delete logic"
```

---

## キーバインドまとめ（CRUD 追加後）

| キー | モード | 動作 |
|------|--------|------|
| `j` / `k` | Normal | 時間カーソル上下移動（自動スクロール） |
| `n` | Normal | 新規作成フォームを開く（選択日時プリフィル） |
| `e` | Normal | カーソル位置のイベントを編集フォームで開く |
| `dd` | Normal | カーソル位置のイベントを削除（確認あり） |
| `Tab` / `Shift+Tab` | Form | フィールド移動 |
| `Space` | Form (IsAllDay フォーカス) | 終日トグル |
| `← / →` | Form (DbSelect フォーカス) | DB 切替 |
| `Enter` | Form | バリデーション → 確定・送信 |
| `Esc` | Form | キャンセル |
| `y` | Confirm | 削除実行 |
| その他 | Confirm | キャンセル |
