# 重複イベント横並び表示 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 同じ時間帯に複数のイベントがある場合、列を最大2分割して横並びに表示する。

**Architecture:** AppStateに`overlap_focus`フィールドと`events_overlapping_hour`メソッドを追加し、`event_at_cursor()`をフォーカスに対応させる。レンダリング側では各スロットで重複イベントを収集し、Spanの左右分割で2列表示する。Layoutの変更は不要。

**Tech Stack:** Rust, ratatui (Span/Line), chrono

---

### Task 1: AppState に overlap_focus フィールドと events_overlapping_hour を追加

**Files:**
- Modify: `src/app/mod.rs`

**Step 1: 失敗するテストを書く**

`src/app/mod.rs` の `#[cfg(test)]` ブロック末尾に追加：

```rust
#[test]
fn test_events_overlapping_hour_returns_all_overlapping() {
    use chrono::{Local, TimeZone};
    let mut state = AppState::new(vec![]);
    let today = state.selected_date;

    let make_timed_event = |id: &str, start_h: u32, end_h: u32| {
        let dt_start = Local
            .from_local_datetime(&today.and_hms_opt(start_h, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&chrono::Utc);
        let dt_end = Local
            .from_local_datetime(&today.and_hms_opt(end_h, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&chrono::Utc);
        crate::api::models::NotionEvent {
            id: id.to_string(),
            title: id.to_string(),
            date_start: None,
            datetime_start: Some(dt_start),
            datetime_end: Some(dt_end),
            is_all_day: false,
            database_id: "db".to_string(),
            color: None,
        }
    };

    // 10:00-11:00 と 10:30-11:30 の2件
    state.events.insert(
        today,
        vec![
            make_timed_event("a", 10, 11),
            make_timed_event("b", 10, 11),
        ],
    );

    let result = state.events_overlapping_hour(today, 10);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_events_overlapping_hour_excludes_non_overlapping() {
    use chrono::{Local, TimeZone};
    let mut state = AppState::new(vec![]);
    let today = state.selected_date;

    let dt_start = Local
        .from_local_datetime(&today.and_hms_opt(11, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&chrono::Utc);
    let dt_end = Local
        .from_local_datetime(&today.and_hms_opt(12, 0, 0).unwrap())
        .unwrap()
        .with_timezone(&chrono::Utc);
    let event = crate::api::models::NotionEvent {
        id: "a".to_string(),
        title: "a".to_string(),
        date_start: None,
        datetime_start: Some(dt_start),
        datetime_end: Some(dt_end),
        is_all_day: false,
        database_id: "db".to_string(),
        color: None,
    };
    state.events.insert(today, vec![event]);

    // cursor_hour=10 のとき 11:00 スタートのイベントは含まれない
    let result = state.events_overlapping_hour(today, 10);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_overlap_focus_resets_on_cursor_move() {
    let mut state = AppState::new(vec![]);
    state.overlap_focus = 1;
    state.cursor_down();
    assert_eq!(state.overlap_focus, 0);
    state.overlap_focus = 1;
    state.cursor_up();
    assert_eq!(state.overlap_focus, 0);
}

#[test]
fn test_event_at_cursor_respects_overlap_focus() {
    use chrono::{Local, TimeZone};
    let mut state = AppState::new(vec![]);
    let today = state.selected_date;
    state.cursor_hour = 10;

    let make_event = |id: &str| {
        let dt = Local
            .from_local_datetime(&today.and_hms_opt(10, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&chrono::Utc);
        let dt_end = Local
            .from_local_datetime(&today.and_hms_opt(11, 0, 0).unwrap())
            .unwrap()
            .with_timezone(&chrono::Utc);
        crate::api::models::NotionEvent {
            id: id.to_string(),
            title: id.to_string(),
            date_start: None,
            datetime_start: Some(dt),
            datetime_end: Some(dt_end),
            is_all_day: false,
            database_id: "db".to_string(),
            color: None,
        }
    };

    state.events.insert(today, vec![make_event("first"), make_event("second")]);

    state.overlap_focus = 0;
    assert_eq!(state.event_at_cursor().unwrap().id, "first");

    state.overlap_focus = 1;
    assert_eq!(state.event_at_cursor().unwrap().id, "second");
}
```

**Step 2: テストが失敗することを確認**

```bash
rustup run stable cargo test test_events_overlapping_hour 2>&1 | tail -5
rustup run stable cargo test test_overlap_focus 2>&1 | tail -5
rustup run stable cargo test test_event_at_cursor_respects 2>&1 | tail -5
```

Expected: `error[E0599]: no method named 'events_overlapping_hour'` など

**Step 3: AppState に overlap_focus フィールドを追加**

`src/app/mod.rs` の `AppState` 構造体に追加：

```rust
pub struct AppState {
    // ... 既存フィールド ...
    pub overlap_focus: u8,  // 重複イベントのフォーカス列インデックス（0 or 1）
}
```

`AppState::new` の `Self { ... }` ブロックに追加：

```rust
overlap_focus: 0,
```

**Step 4: events_overlapping_hour メソッドを追加**

`impl AppState` ブロックに追加（`event_at_cursor` の直前あたり）：

```rust
/// 指定日・指定時刻（hour）に重なる時間付きイベントを全件返す
/// 重複判定: event_start < (hour+1)*60 && event_end > hour*60
pub fn events_overlapping_hour(&self, date: NaiveDate, hour: u32) -> Vec<&NotionEvent> {
    let hour_start_min = hour as usize * 60;
    let hour_end_min = hour_start_min + 60;

    self.events_for_date(&date)
        .into_iter()
        .filter(|e| {
            if e.is_all_day {
                return false;
            }
            if let Some(dt_start) = e.datetime_start {
                let local_start = dt_start.with_timezone(&chrono::Local);
                let start_min =
                    local_start.hour() as usize * 60 + local_start.minute() as usize;
                let end_min = if let Some(dt_end) = e.datetime_end {
                    let local_end = dt_end.with_timezone(&chrono::Local);
                    local_end.hour() as usize * 60 + local_end.minute() as usize
                } else {
                    start_min + 60
                };
                start_min < hour_end_min && end_min > hour_start_min
            } else {
                false
            }
        })
        .collect()
}
```

**Step 5: event_at_cursor を overlap_focus 対応に更新**

既存の `event_at_cursor` を以下に置き換え：

```rust
pub fn event_at_cursor(&self) -> Option<&crate::api::models::NotionEvent> {
    let overlapping = self.events_overlapping_hour(self.selected_date, self.cursor_hour);
    overlapping.into_iter().nth(self.overlap_focus as usize)
}
```

**Step 6: cursor_up / cursor_down で overlap_focus をリセット**

`cursor_up` の先頭に追加：

```rust
self.overlap_focus = 0;
```

`cursor_down` の先頭に追加：

```rust
self.overlap_focus = 0;
```

**Step 7: テストが通ることを確認**

```bash
rustup run stable cargo test 2>&1 | tail -10
```

Expected: `test result: ok. N passed`（全テスト通過）

**Step 8: コミット**

```bash
git add src/app/mod.rs
git commit -m "feat: add overlap_focus and events_overlapping_hour to AppState"
```

---

### Task 2: Normal モードの ←/→ キーで overlap_focus を切り替え

**Files:**
- Modify: `src/main.rs`

**Step 1: overlap_focus_left / overlap_focus_right メソッドを AppState に追加**

`src/app/mod.rs` の `impl AppState` に追加：

```rust
pub fn overlap_focus_left(&mut self) {
    self.overlap_focus = 0;
}

pub fn overlap_focus_right(&mut self) {
    self.overlap_focus = 1;
}
```

**Step 2: main.rs の Normal モードに ←/→ ハンドラを追加**

`src/main.rs` の `app::AppMode::Normal` の `match code` ブロック内、`_ => { pending_d = false; }` の直前に追加：

```rust
KeyCode::Left => {
    pending_d = false;
    state.overlap_focus_left();
}
KeyCode::Right => {
    pending_d = false;
    state.overlap_focus_right();
}
```

**Step 3: ビルドが通ることを確認**

```bash
rustup run stable cargo build 2>&1 | tail -5
```

Expected: `Finished`

**Step 4: コミット**

```bash
git add src/main.rs src/app/mod.rs
git commit -m "feat: add overlap_focus left/right key handling in normal mode"
```

---

### Task 3: week_view でスロットごとに複数イベントを横並び表示

**Files:**
- Modify: `src/ui/week_view.rs`

**Step 1: スロット内の複数イベント収集ロジックを理解する**

`render_time_slots` 内の既存ロジック（`active_event` を `find` で取得している箇所）を `collect` に変更する。

変更対象: `src/ui/week_view.rs:198-217` あたりの `let active_event = timed_events.iter().find(...)` ブロック。

**Step 2: スロット重複イベントを収集する関数を追加**

ファイル先頭付近（`build_cursor_cell_text` の下）に追加：

```rust
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
                let start_min =
                    local_start.hour() as usize * 60 + local_start.minute() as usize;
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
```

**Step 3: スロット1件分のSpanを構築するヘルパーを追加**

```rust
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
        let truncated: String = if label.len() > col_width {
            label
                .chars()
                .take(col_width.saturating_sub(1))
                .collect::<String>()
                + "…"
        } else {
            format!("{:<width$}", label, width = col_width)
        };

        if is_cursor_row {
            return Span::styled(
                build_cursor_cell_text(col_width, Some(&truncated)),
                cursor_style,
            );
        }

        match event_style_str {
            "text" => Span::styled(
                truncated,
                Style::default()
                    .fg(event_color)
                    .add_modifier(Modifier::BOLD),
            ),
            "bar" => Span::styled(
                format!("▌{}", &truncated[..truncated.len().saturating_sub(1)]),
                Style::default().fg(event_color).add_modifier(Modifier::BOLD),
            ),
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
            return Span::styled(
                build_cursor_cell_text(col_width, None),
                cursor_style,
            );
        }
        match event_style_str {
            "bar" => Span::styled(
                format!("{:<width$}", "▌", width = col_width),
                Style::default().fg(event_color),
            ),
            "block" => Span::styled(
                " ".repeat(col_width),
                Style::default().bg(event_color),
            ),
            _ => Span::raw(" ".repeat(col_width)),
        }
    }
}
```

**Step 4: render_time_slots のスロット処理を書き換え**

`slot_lines` を構築している `map(|s| { ... })` のクロージャ本体を以下に置き換え（`let active_event = ...` から末尾の `Line::from("")` まで）：

```rust
let slot_h = s / 4;
let slot_m = (s % 4) * 15;
let slot_total_min = slot_h * 60 + slot_m;

// このスロットに重なる全イベントを取得
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

        // フォーカス判定（selected_date のみ overlap_focus を反映）
        let (left_cursor, right_cursor) = if is_cursor_row {
            (
                state.overlap_focus == 0,
                state.overlap_focus == 1,
            )
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
        } else {
            // 3件以上: "+N" を表示
            let overflow = n - 1;
            let label = format!("+{}", overflow);
            let padded = format!("{:<width$}", label, width = right_w);
            if right_cursor {
                Span::styled(padded, cursor_style)
            } else {
                Span::styled(
                    padded,
                    Style::default().fg(Color::DarkGray),
                )
            }
        };

        Line::from(vec![left_span, right_span])
    }
}
```

また、クロージャの先頭にある `slot_h` / `slot_m` / `slot_total_min` / `slot_end_min` の既存定義を削除する（重複するため）。

**Step 5: ビルドが通ることを確認**

```bash
rustup run stable cargo build 2>&1 | tail -10
```

Expected: `Finished`（警告は許容）

**Step 6: テストが全通過することを確認**

```bash
rustup run stable cargo test 2>&1 | tail -5
```

Expected: `test result: ok. N passed`

**Step 7: コミット**

```bash
git add src/ui/week_view.rs
git commit -m "feat: render overlapping events side-by-side in week view (max 2 columns)"
```
