# Noca UX Polish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Improve Noca's visual and UX quality by adding config-driven property name mapping, DB-color event rendering, navigation consistency, and a live status bar.

**Architecture:** Extend `DatabaseConfig` with optional fields, thread them through the API client and renderer, fix `AppState` day-selection to auto-follow weeks, and replace the static help bar with a context-aware status bar.

**Tech Stack:** Rust, ratatui, crossterm, serde/toml, chrono, anyhow

---

### Task 1: Config に property name と event_style フィールドを追加

**Files:**
- Modify: `src/config/mod.rs`

**Step 1: テストを書く**

`src/config/mod.rs` の `#[cfg(test)]` ブロックに追記:

```rust
#[test]
fn test_optional_property_names() {
    let toml_str = r#"
[auth]
integration_token = "secret"

[[databases]]
id = "aaaa"
name = "Work"
color = "green"
date_property = "開催日"
title_property = "タスク名"
event_style = "bar"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let db = &config.databases[0];
    assert_eq!(db.date_property, Some("開催日".to_string()));
    assert_eq!(db.title_property, Some("タスク名".to_string()));
    assert_eq!(db.event_style, "bar");
}

#[test]
fn test_optional_property_names_defaults() {
    let toml_str = r#"
[auth]
integration_token = "secret"

[[databases]]
id = "aaaa"
name = "Work"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let db = &config.databases[0];
    assert_eq!(db.date_property, None);
    assert_eq!(db.title_property, None);
    assert_eq!(db.event_style, "block");
}
```

**Step 2: テスト失敗確認**

```bash
cargo test config
```
Expected: コンパイルエラー（フィールド未定義）

**Step 3: `DatabaseConfig` にフィールドを追加**

`src/config/mod.rs` の `DatabaseConfig` を以下に置き換える:

```rust
fn default_event_style() -> String {
    "block".to_string()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
    pub date_property: Option<String>,
    pub title_property: Option<String>,
    #[serde(default = "default_event_style")]
    pub event_style: String,
}
```

**Step 4: テスト実行**

```bash
cargo test config
```
Expected: 全テスト pass（既存2件 + 新規2件）

**Step 5: Commit**

```bash
git add src/config/mod.rs
git commit -m "feat: add date_property, title_property, event_style to DatabaseConfig"
```

---

### Task 2: API クライアントに config プロパティ名を反映

**Files:**
- Modify: `src/api/mod.rs`

**Step 1: テストを書く**

`src/api/mod.rs` の `#[cfg(test)]` ブロックに追記:

```rust
#[test]
fn test_parse_event_with_custom_title_property() {
    let page = make_page(json!({
        "タスク名": { "title": [{ "plain_text": "カスタムタイトル" }] },
        "Date": { "date": { "start": "2026-03-05" } }
    }));
    // title_keys に "タスク名" を渡す
    let event = parse_event_with_keys(&page, "db-1", Some("タスク名"), Some("Date"));
    assert!(event.is_some());
    assert_eq!(event.unwrap().title, "カスタムタイトル");
}

#[test]
fn test_parse_event_with_custom_date_property() {
    let page = make_page(json!({
        "Name": { "title": [{ "plain_text": "イベント" }] },
        "開催日": { "date": { "start": "2026-03-05" } }
    }));
    let event = parse_event_with_keys(&page, "db-1", Some("Name"), Some("開催日"));
    assert!(event.is_some());
    assert!(event.unwrap().is_all_day);
}

#[test]
fn test_parse_event_with_keys_none_falls_back() {
    // None を渡した場合は従来のフォールバックが動く
    let page = make_page(json!({
        "Name": { "title": [{ "plain_text": "テスト" }] },
        "Date": { "date": { "start": "2026-03-05" } }
    }));
    let event = parse_event_with_keys(&page, "db-1", None, None);
    assert!(event.is_some());
}
```

**Step 2: テスト失敗確認**

```bash
cargo test api
```
Expected: コンパイルエラー（`parse_event_with_keys` 未定義）

**Step 3: `parse_event_with_keys` を実装し、`parse_event` をラッパーにする**

`src/api/mod.rs` の `parse_event` を以下に置き換える:

```rust
/// PageObject から NotionEvent に変換する（プロパティ名指定版）
pub fn parse_event_with_keys(
    page: &PageObject,
    database_id: &str,
    title_property: Option<&str>,
    date_property: Option<&str>,
) -> Option<NotionEvent> {
    let props = &page.properties;
    let title = extract_title_with_key(props, title_property)?;
    let (date_start, datetime_start, datetime_end, is_all_day) =
        extract_date_with_key(props, date_property)?;
    Some(NotionEvent {
        id: page.id.clone(),
        title,
        date_start,
        datetime_start,
        datetime_end,
        is_all_day,
        database_id: database_id.to_string(),
        color: None,
    })
}

/// 後方互換ラッパー
pub fn parse_event(page: &PageObject, database_id: &str) -> Option<NotionEvent> {
    parse_event_with_keys(page, database_id, None, None)
}
```

`extract_title` を `extract_title_with_key` に置き換える:

```rust
fn extract_title_with_key(props: &serde_json::Value, key: Option<&str>) -> Option<String> {
    let candidates: Vec<&str> = if let Some(k) = key {
        vec![k]
    } else {
        vec!["名前", "Name", "title", "Title"]
    };
    for key in &candidates {
        if let Some(title_prop) = props.get(*key) {
            if let Some(arr) = title_prop["title"].as_array() {
                let text: String = arr
                    .iter()
                    .filter_map(|t| t["plain_text"].as_str())
                    .collect();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}
```

`extract_date` を `extract_date_with_key` に置き換える:

```rust
fn extract_date_with_key(
    props: &serde_json::Value,
    key: Option<&str>,
) -> Option<(
    Option<chrono::NaiveDate>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    bool,
)> {
    let candidates: Vec<&str> = if let Some(k) = key {
        vec![k]
    } else {
        vec!["日付", "Date", "date"]
    };
    for key in &candidates {
        if let Some(date_prop) = props.get(*key) {
            if let Some(start_str) = date_prop["date"]["start"].as_str() {
                if start_str.len() == 10 {
                    let date = start_str.parse::<chrono::NaiveDate>().ok()?;
                    return Some((Some(date), None, None, true));
                } else {
                    let dt = chrono::DateTime::parse_from_rfc3339(start_str)
                        .ok()?
                        .with_timezone(&chrono::Utc);
                    let end_dt = date_prop["date"]["end"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.with_timezone(&chrono::Utc));
                    return Some((None, Some(dt), end_dt, false));
                }
            }
        }
    }
    None
}
```

既存の `extract_title` と `extract_date` の呼び出し箇所を `extract_title_with_key(props, None)` / `extract_date_with_key(props, None)` に更新（`parse_event` ラッパー経由なので自動的に対応済み）。

**Step 4: `main.rs` の `fetch_events` で `parse_event_with_keys` を使う**

`src/main.rs` の `fetch_events` 内の以下を変更:

```rust
// 変更前
if let Some(mut event) = api::parse_event(page, &db.id) {

// 変更後
if let Some(mut event) = api::parse_event_with_keys(
    page,
    &db.id,
    db.title_property.as_deref(),
    db.date_property.as_deref(),
) {
```

**Step 5: テスト実行**

```bash
cargo test
```
Expected: 全テスト pass

**Step 6: Commit**

```bash
git add src/api/mod.rs src/main.rs
git commit -m "feat: support custom date/title property names from config"
```

---

### Task 3: H/L ナビゲーションの週追従

**Files:**
- Modify: `src/app/mod.rs`

**Step 1: テストを書く**

`src/app/mod.rs` の `#[cfg(test)]` ブロックに追記:

```rust
#[test]
fn test_select_next_day_follows_week() {
    let mut state = AppState::new(vec![]);
    // 週末（日曜）まで進める
    let week_end = state.current_week_start + chrono::Duration::days(6);
    state.selected_date = week_end;
    let initial_week = state.current_week_start;

    // 次の日（月曜）に進む → 週が1週間進むはず
    state.select_next_day();

    assert_eq!(state.current_week_start, initial_week + chrono::Duration::weeks(1));
}

#[test]
fn test_select_prev_day_follows_week() {
    let mut state = AppState::new(vec![]);
    // 週頭（月曜）にいる
    state.selected_date = state.current_week_start;
    let initial_week = state.current_week_start;

    // 前の日（日曜）に戻る → 週が1週間戻るはず
    state.select_prev_day();

    assert_eq!(state.current_week_start, initial_week - chrono::Duration::weeks(1));
}

#[test]
fn test_select_next_day_within_week_does_not_change_week() {
    let mut state = AppState::new(vec![]);
    state.selected_date = state.current_week_start + chrono::Duration::days(2); // 水曜
    let initial_week = state.current_week_start;

    state.select_next_day();

    assert_eq!(state.current_week_start, initial_week);
}
```

**Step 2: テスト失敗確認**

```bash
cargo test app
```
Expected: `test_select_next_day_follows_week` と `test_select_prev_day_follows_week` が FAIL

**Step 3: `select_next_day` / `select_prev_day` を修正**

`src/app/mod.rs` の2メソッドを以下に置き換える:

```rust
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
```

**Step 4: テスト実行**

```bash
cargo test app
```
Expected: 全テスト pass

**Step 5: `main.rs` で H/L 時に fetch_events を発火**

`src/main.rs` のキーハンドラを以下に変更:

```rust
KeyCode::Char('H') => {
    let week_before = state.current_week_start;
    state.select_prev_day();
    if state.current_week_start != week_before {
        fetch_events(client, state, databases).await;
    }
}
KeyCode::Char('L') => {
    let week_before = state.current_week_start;
    state.select_next_day();
    if state.current_week_start != week_before {
        fetch_events(client, state, databases).await;
    }
}
```

**Step 6: ビルド確認**

```bash
cargo build
```
Expected: Finished

**Step 7: Commit**

```bash
git add src/app/mod.rs src/main.rs
git commit -m "fix: H/L day selection auto-follows week boundary"
```

---

### Task 4: イベント描画に DB カラーと event_style を反映

**Files:**
- Modify: `src/ui/week_view.rs`
- Modify: `src/ui/sidebar.rs`（`color_from_str` を `ui/mod.rs` に移動）
- Modify: `src/ui/mod.rs`

**Step 1: `color_from_str` を `ui/mod.rs` に移動して共有**

`src/ui/mod.rs` に追加:

```rust
use ratatui::style::Color;

pub mod sidebar;
pub mod week_view;

pub fn help_text() -> &'static str {
    "[h/l]週移動  [j/k]スクロール  [H/L]日選択  [t]今日  [Tab]切替  [q]終了"
}

pub fn color_from_str(s: &str) -> Color {
    match s {
        "red" => Color::Red,
        "green" => Color::Green,
        "yellow" => Color::Yellow,
        "blue" => Color::Blue,
        "magenta" => Color::Magenta,
        "cyan" => Color::Cyan,
        _ => Color::White,
    }
}
```

`src/ui/sidebar.rs` の `color_from_str` 関数を削除し、呼び出し箇所を `crate::ui::color_from_str` に変更:

```rust
// 変更前
let color = color_from_str(&db.color);
// 変更後
let color = crate::ui::color_from_str(&db.color);
```

**Step 2: `render_time_slots` の描画スタイルを event_style 対応に変更**

`src/ui/week_view.rs` の `render_time_slots` 内、イベントが存在する場合のブランチを以下に置き換える:

```rust
if let Some(ev) = event {
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
        label.chars().take(col_width.saturating_sub(1)).collect::<String>() + "…"
    } else {
        label
    };

    let event_color = ev
        .color
        .as_deref()
        .map(crate::ui::color_from_str)
        .unwrap_or(Color::Green);

    // DatabaseConfig の event_style を取得
    let event_style_str = state
        .databases
        .iter()
        .find(|db| db.id == ev.database_id)
        .map(|db| db.event_style.as_str())
        .unwrap_or("block");

    let styled_line = match event_style_str {
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
        // "block" がデフォルト
        _ => Line::from(Span::styled(
            truncated,
            Style::default()
                .bg(event_color)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )),
    };
    styled_line
```

また `render_all_day_row` でも DB カラーを反映:

```rust
// 変更前
let style = Style::default().fg(Color::Cyan);

// 変更後（全日イベントは最初のDB色を使う。複数DBが混在する場合は最初のもの）
let event_color = state
    .events_for_date(date)
    .into_iter()
    .filter(|e| e.is_all_day)
    .next()
    .and_then(|e| e.color.as_deref())
    .map(crate::ui::color_from_str)
    .unwrap_or(Color::Cyan);
let style = Style::default().fg(event_color);
```

**Step 3: ビルド確認**

```bash
cargo build
```
Expected: Finished（警告は無視可）

**Step 4: Commit**

```bash
git add src/ui/mod.rs src/ui/week_view.rs src/ui/sidebar.rs
git commit -m "feat: render events with DB color and configurable event_style"
```

---

### Task 5: ステータスバーにローディング・エラー状態を表示

**Files:**
- Modify: `src/main.rs`
- Modify: `src/ui/mod.rs`

**Step 1: `help_text` テストを `status_bar` 用に更新**

`src/ui/mod.rs` の既存テストを維持しつつ、新しい関数のテストを追加:

```rust
#[test]
fn test_status_bar_loading() {
    let text = status_bar_text(true, None);
    assert!(text.contains("読み込み中"));
}

#[test]
fn test_status_bar_error() {
    let text = status_bar_text(false, Some("API Error: 401"));
    assert!(text.contains("API Error: 401"));
}

#[test]
fn test_status_bar_normal() {
    let text = status_bar_text(false, None);
    assert!(text.contains("[h/l]"));
}
```

**Step 2: テスト失敗確認**

```bash
cargo test ui
```
Expected: コンパイルエラー（`status_bar_text` 未定義）

**Step 3: `status_bar_text` を実装**

`src/ui/mod.rs` に追加:

```rust
pub fn status_bar_text<'a>(loading: bool, error: Option<&'a str>) -> String {
    if let Some(msg) = error {
        format!("✗ {}", msg)
    } else if loading {
        "読み込み中...".to_string()
    } else {
        help_text().to_string()
    }
}
```

**Step 4: テスト実行**

```bash
cargo test ui
```
Expected: 全テスト pass

**Step 5: `main.rs` の `render_help_bar` を `render_status_bar` に置き換え**

`src/main.rs` の `render_help_bar` 関数を以下に置き換える:

```rust
fn render_status_bar(f: &mut ratatui::Frame, area: Rect, state: &AppState) {
    let error = state.status_message.as_deref();
    let text = ui::status_bar_text(state.loading, error);
    let style = if error.is_some() {
        Style::default().fg(Color::Red)
    } else if state.loading {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(text).style(style), area);
}
```

`terminal.draw` 内の呼び出しを更新:

```rust
// 変更前
render_help_bar(f, root_chunks[1]);
// 変更後
render_status_bar(f, root_chunks[1], state);
```

**Step 6: ビルド確認 + 全テスト実行**

```bash
cargo test && cargo build
```
Expected: 全テスト pass、Finished

**Step 7: Commit**

```bash
git add src/ui/mod.rs src/main.rs
git commit -m "feat: show loading/error state in status bar"
```

---

## 完了後の確認

```bash
cargo run
```

- イベントが DB カラーで背景色表示される（デフォルト `block` スタイル）
- `H/L` で週をまたぐと自動的に表示週が切り替わり fetch が走る
- 起動時・週移動時に「読み込み中...」が黄色で表示される
- API エラー時にエラーメッセージが赤で表示される

`~/.config/noca/config.toml` で動作確認:

```toml
[[databases]]
id = "xxx"
name = "仕事"
color = "green"
date_property = "開催日"     # カスタムプロパティ名
event_style = "bar"          # バースタイルで表示
```
