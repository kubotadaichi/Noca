# 15-Minute Time Slot Granularity Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 週ビューの時間スロットを「1行=1時間」から「1行=15分」に変更し、イベントの持続時間を視覚的に表現する。

**Architecture:** `scroll_offset` の単位を15分スロット（0〜95）に変更し、`render_time_slots` で各スロットをイベントのstart〜endに対して照合する。開始スロットにタイトルを、継続スロットにはスタイルに応じたマーカー（`▌`や背景色）を表示する。

**Tech Stack:** Rust, ratatui, chrono

---

### Task 1: scroll_offset の単位を15分スロットに変更

**Files:**
- Modify: `src/app/mod.rs`

**Step 1: 既存テストを確認して失敗することを確認**

```bash
cd /Users/kubotadaichi/dev/github/Noca
cargo test 2>&1 | head -40
```

Expected: テスト全件 PASS（現状確認）

**Step 2: scroll_offset の初期値・上限・増減を変更**

`src/app/mod.rs` の以下の箇所を変更：

```rust
// AppState::new 内
scroll_offset: 28, // デフォルト07:00から表示（7 * 4 = 28スロット）

// scroll_up
pub fn scroll_up(&mut self) {
    self.scroll_offset = self.scroll_offset.saturating_sub(1);
}

// scroll_down
pub fn scroll_down(&mut self) {
    if self.scroll_offset < 88 { // 22:00 = 22 * 4 = 88
        self.scroll_offset += 1;
    }
}
```

**Step 3: scroll_offset に関するテストを追加**

`src/app/mod.rs` のテストモジュールに追加：

```rust
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
```

**Step 4: テスト実行**

```bash
cargo test app:: 2>&1
```

Expected: 全テスト PASS

**Step 5: コミット**

```bash
git add src/app/mod.rs
git commit -m "feat: change scroll_offset unit to 15-min slots"
```

---

### Task 2: 時間ラベルを15分粒度に更新

**Files:**
- Modify: `src/ui/week_view.rs:105-133`（`render_time_slots` の時間ラベル部分）

**Step 1: `render_time_slots` の変数名と計算を変更**

`render_time_slots` 関数の冒頭部分を以下に変更：

```rust
fn render_time_slots(f: &mut Frame, area: Rect, state: &AppState) {
    let now = Local::now();
    let today = now.date_naive();
    let current_hour = now.hour() as usize;
    let current_minute = now.minute() as usize;
    let current_slot = current_hour * 4 + current_minute / 15;

    let visible_slots = area.height as usize;
    let start_slot = state.scroll_offset as usize;

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
            let h = s / 4;
            if s % 4 == 0 {
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
```

**Step 2: ビルドエラーがないか確認**

```bash
cargo build 2>&1
```

Expected: コンパイル成功（まだイベント描画は変更していないのでエラーなし）

**Step 3: コミット**

```bash
git add src/ui/week_view.rs
git commit -m "feat: update time labels to 15-min slot granularity"
```

---

### Task 3: イベント描画を15分スロット対応に変更

**Files:**
- Modify: `src/ui/week_view.rs:135-211`（イベント描画ループ部分）

**Step 1: イベントループを15分スロット対応に書き換え**

`render_time_slots` のイベント描画ループ（`for (col_idx, date) in ...` 以降）を以下に置き換える：

```rust
    for (col_idx, date) in week_dates.iter().enumerate() {
        let timed_events: Vec<_> = state
            .events_for_date(date)
            .into_iter()
            .filter(|e| !e.is_all_day)
            .collect();

        let slot_lines: Vec<Line> = (start_slot..start_slot + visible_slots)
            .map(|s| {
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
                let slot_end_min = slot_total_min + 15;    // スロット終了（分）

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
                            label.chars().take(col_width.saturating_sub(1)).collect::<String>()
                                + "…"
                        } else {
                            label
                        };

                        match event_style_str {
                            "text" => Line::from(Span::styled(
                                truncated,
                                Style::default()
                                    .fg(event_color)
                                    .add_modifier(Modifier::BOLD),
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
                            "bar" => Line::from(Span::styled(
                                "▌",
                                Style::default().fg(event_color),
                            )),
                            "block" => Line::from(Span::styled(
                                " ".repeat(cols[col_idx + 1].width as usize),
                                Style::default().bg(event_color),
                            )),
                            _ => Line::from(""), // text: 継続行は空白
                        }
                    }
                } else {
                    Line::from("")
                }
            })
            .collect();

        f.render_widget(Paragraph::new(slot_lines), cols[col_idx + 1]);
    }
```

**Step 2: ビルド確認**

```bash
cargo build 2>&1
```

Expected: コンパイル成功

**Step 3: テスト実行**

```bash
cargo test 2>&1
```

Expected: 全テスト PASS

**Step 4: 動作確認（手動）**

```bash
cargo run 2>&1
```

確認項目：
- 時間ラベルが `08:00` / 空行 / 空行 / 空行 / `09:00` ... のパターンで表示される
- `j`/`k` で15分単位でスクロールする
- 時間付きイベントが正しいスロットに表示される
- 1時間以上のイベントが複数スロットにまたがって表示される（`bar` は `▌` が続く、`block` は背景色が続く）
- 異なるイベント間に空白スロットが生まれる

**Step 5: コミット**

```bash
git add src/ui/week_view.rs
git commit -m "feat: render events across 15-min slots with continuation markers"
```

---

## 完了後の確認

```bash
cargo test 2>&1
```

全テスト PASS を確認。
