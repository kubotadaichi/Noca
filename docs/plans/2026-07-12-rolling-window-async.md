# ローリング7日ウィンドウ + 全通信の非同期化 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 週表示を月曜固定から「1日ずつスライドできるローリング7日ウィンドウ」に変え、全 Notion API 通信をバックグラウンド化して通信中も UI 操作を継続できるようにする。

**Architecture:** 表示ウィンドウ左端 `view_start` を状態に持ち、選択日移動時に1日だけ追従させる。ネットワーク処理は `tokio::sync::mpsc` チャネルでメインループから分離し、`tokio::spawn` で実行、結果を `AppMessage` として `try_recv()` で取り込む。stale 対策に世代番号、無駄な再取得抑制に取得済み範囲判定を使う。

**Tech Stack:** Rust, ratatui + crossterm, tokio (mpsc + spawn), reqwest, chrono, anyhow

## Global Constraints

- cargo は必ず `rustup run stable cargo` で実行する。
- 既存のユニットテスト（68件）を壊さない。仕様変更で意味が変わるテストは新仕様に合わせて更新する。
- `NotionClient` は既に `#[derive(Clone)]`（内部 reqwest `Client` は Arc 共有）。spawn には clone で渡し、`Arc` ラッパは追加しない。
- `DatabaseConfig` は `Clone`。spawn へは `databases.to_vec()` で渡す。
- ローディング表示は `pending_requests > 0` を `ui::status_bar_text(bool, ...)` に渡す（シグネチャは変えない）。

---

### Task 1: `view_start` へのリネームと1日ローリング

月曜固定の `current_week_start` を「表示ウィンドウ左端」を表す `view_start` にリネームし、選択日移動時にウィンドウが1日だけ追従するようにする。`go_to_today` は月曜始まりの今日の週へスナップする。

**Files:**
- Modify: `src/app/mod.rs`（フィールド定義・`new`・`next_week`/`prev_week`/`select_next_day`/`select_prev_day`/`go_to_today`/`week_dates`・既存テスト）

**Interfaces:**
- Consumes: なし
- Produces:
  - `AppState.view_start: NaiveDate`（旧 `current_week_start`）
  - `AppState::select_next_day(&mut self)` / `select_prev_day(&mut self)`: 選択日を1日動かし、ウィンドウ外なら `view_start` を1日だけ追従
  - `AppState::next_week(&mut self)` / `prev_week(&mut self)`: `view_start` を7日スライド
  - `AppState::go_to_today(&mut self)`: `selected_date = today`, `view_start = week_start_of(today)`

- [ ] **Step 1: 既存テストを新仕様に更新（失敗を確認するため先に書き換え）**

`src/app/mod.rs` のテストで `current_week_start` を参照している箇所を `view_start` に置換し、日単位追従を検証する形へ更新する。該当テストを次のように差し替える。

```rust
    #[test]
    fn test_next_prev_week() {
        let mut state = AppState::new(vec![]);
        let initial = state.view_start;
        state.next_week();
        assert_eq!(state.view_start, initial + chrono::Duration::weeks(1));
        state.prev_week();
        assert_eq!(state.view_start, initial);
    }

    #[test]
    fn test_select_next_day_rolls_window_by_one_day() {
        let mut state = AppState::new(vec![]);
        // 選択日をウィンドウ右端に置く
        state.selected_date = state.view_start + chrono::Duration::days(6);
        let initial_view = state.view_start;

        state.select_next_day();

        // ウィンドウは7日ではなく1日だけ進む
        assert_eq!(state.view_start, initial_view + chrono::Duration::days(1));
        assert_eq!(state.selected_date, initial_view + chrono::Duration::days(7));
    }

    #[test]
    fn test_select_prev_day_rolls_window_by_one_day() {
        let mut state = AppState::new(vec![]);
        // 選択日をウィンドウ左端に置く
        state.selected_date = state.view_start;
        let initial_view = state.view_start;

        state.select_prev_day();

        assert_eq!(state.view_start, initial_view - chrono::Duration::days(1));
        assert_eq!(state.selected_date, initial_view - chrono::Duration::days(1));
    }

    #[test]
    fn test_select_next_day_within_window_does_not_move_view() {
        let mut state = AppState::new(vec![]);
        state.selected_date = state.view_start + chrono::Duration::days(2);
        let initial_view = state.view_start;

        state.select_next_day();

        assert_eq!(state.view_start, initial_view);
    }

    #[test]
    fn test_go_to_today_snaps_view_to_monday_week() {
        let mut state = AppState::new(vec![]);
        state.selected_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        state.view_start = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();

        state.go_to_today();

        let today = chrono::Local::now().date_naive();
        assert_eq!(state.selected_date, today);
        assert_eq!(state.view_start, week_start_of(today));
    }
```

また `test_week_start_of_*`、`test_week_dates_returns_7_days`、`test_scroll_*`、`test_toggle_panel`、`test_replace_events_*`、`test_cursor_*`、`test_event*`、`test_overlap_*` は変更不要（`current_week_start` を参照していない）。旧 `test_select_next_day_follows_week` / `test_select_prev_day_follows_week` / `test_select_next_day_within_week_does_not_change_week` は上記3テストで置き換えるため削除する。

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `rustup run stable cargo test`
Expected: FAIL（`view_start` が存在しない旨のコンパイルエラー、または新テストの assert 不一致）

- [ ] **Step 3: フィールドとメソッドを実装**

`src/app/mod.rs` の構造体フィールド（57行目付近）:

```rust
    pub view_start: NaiveDate,
```
（`pub current_week_start: NaiveDate,` を置換）

`AppState::new`（76行目付近）の初期化:

```rust
            view_start: week_start,
```
（`current_week_start: week_start,` を置換）

`next_week`/`prev_week`:

```rust
    pub fn next_week(&mut self) {
        self.view_start += chrono::Duration::weeks(1);
    }

    pub fn prev_week(&mut self) {
        self.view_start -= chrono::Duration::weeks(1);
    }
```

`select_next_day`/`select_prev_day`（1日追従に変更）:

```rust
    pub fn select_next_day(&mut self) {
        self.selected_date += chrono::Duration::days(1);
        if self.selected_date > self.view_start + chrono::Duration::days(6) {
            self.view_start += chrono::Duration::days(1);
        }
    }

    pub fn select_prev_day(&mut self) {
        self.selected_date -= chrono::Duration::days(1);
        if self.selected_date < self.view_start {
            self.view_start -= chrono::Duration::days(1);
        }
    }
```

`go_to_today`:

```rust
    pub fn go_to_today(&mut self) {
        let today = Local::now().date_naive();
        self.selected_date = today;
        self.view_start = week_start_of(today);
    }
```

`week_dates`（262行目付近）:

```rust
            .map(|i| self.view_start + chrono::Duration::days(i))
```
（`self.current_week_start` を `self.view_start` に置換）

- [ ] **Step 4: テストを実行して成功を確認**

Run: `rustup run stable cargo test`
Expected: `app` モジュールのテストは PASS。`main.rs` はまだ `current_week_start`/`loading` を参照するため**ビルドは失敗し得る**が、それは Task 3/4 で解消する。この時点では `cargo test --lib` 相当で app のロジックが通ることを確認する。まず `rustup run stable cargo build 2>&1 | head` で残る参照エラーが `main.rs` の `current_week_start`/`state.loading` のみであることを確認する。

- [ ] **Step 5: コミット**

まだ `main.rs` がビルドを通らない場合は Task 4 完了後にまとめてコミットする。app 単体が完結しているので、`main.rs` を先に暫定追随（`current_week_start`→`view_start` の機械置換のみ）してビルドを通してからコミットしてもよい。ここでは app の変更のみステージしてコミットする:

```bash
git add src/app/mod.rs
git commit -m "feat: rolling 7-day window (view_start + 1-day follow)"
```

---

### Task 2: 取得済み範囲の追跡と `needs_fetch()`

プリフェッチ済み範囲を状態に持ち、ローリング時に端へ近づいたときだけ再取得を要求する判定を追加する。

**Files:**
- Modify: `src/app/mod.rs`（`AppState` フィールド追加・`new`・`needs_fetch` メソッド・テスト）

**Interfaces:**
- Consumes: `AppState.view_start`（Task 1）
- Produces:
  - `AppState.fetched_start: Option<NaiveDate>` / `fetched_end: Option<NaiveDate>`
  - `AppState::needs_fetch(&self) -> bool`

- [ ] **Step 1: 失敗するテストを書く**

`src/app/mod.rs` のテストモジュールに追加:

```rust
    #[test]
    fn test_needs_fetch_true_when_never_fetched() {
        let state = AppState::new(vec![]);
        assert!(state.needs_fetch());
    }

    #[test]
    fn test_needs_fetch_false_within_range() {
        let mut state = AppState::new(vec![]);
        // ウィンドウ左端の 1 週間前 〜 2 週間後を取得済みにする
        state.fetched_start = Some(state.view_start - chrono::Duration::weeks(1));
        state.fetched_end = Some(state.view_start + chrono::Duration::weeks(2));
        assert!(!state.needs_fetch());
    }

    #[test]
    fn test_needs_fetch_true_near_left_edge() {
        let mut state = AppState::new(vec![]);
        state.fetched_start = Some(state.view_start - chrono::Duration::days(2));
        state.fetched_end = Some(state.view_start + chrono::Duration::weeks(2));
        // 左端まで残り 2 日（マージン 3 日未満）→ 再取得が必要
        assert!(state.needs_fetch());
    }

    #[test]
    fn test_needs_fetch_true_near_right_edge() {
        let mut state = AppState::new(vec![]);
        state.fetched_start = Some(state.view_start - chrono::Duration::weeks(1));
        // 表示右端 view_start+6、取得右端はその 2 日先のみ（マージン 3 日未満）
        state.fetched_end = Some(state.view_start + chrono::Duration::days(8));
        assert!(state.needs_fetch());
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `rustup run stable cargo test needs_fetch`
Expected: FAIL（`fetched_start` / `needs_fetch` が未定義）

- [ ] **Step 3: フィールドとメソッドを実装**

`AppState` フィールドに追加:

```rust
    pub fetched_start: Option<NaiveDate>,
    pub fetched_end: Option<NaiveDate>,
```

`AppState::new` の初期化に追加:

```rust
            fetched_start: None,
            fetched_end: None,
```

メソッド（`impl AppState` 内、`week_dates` の近く）:

```rust
    /// 表示ウィンドウが取得済み範囲の端マージン内に近づいた、
    /// または未取得なら再取得が必要
    pub fn needs_fetch(&self) -> bool {
        const MARGIN: i64 = 3;
        match (self.fetched_start, self.fetched_end) {
            (Some(fs), Some(fe)) => {
                self.view_start < fs + chrono::Duration::days(MARGIN)
                    || self.view_start + chrono::Duration::days(6)
                        > fe - chrono::Duration::days(MARGIN)
            }
            _ => true,
        }
    }
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `rustup run stable cargo test needs_fetch`
Expected: PASS（4件）

- [ ] **Step 5: コミット**

```bash
git add src/app/mod.rs
git commit -m "feat: track fetched range and add needs_fetch()"
```

---

### Task 3: 非同期メッセージ型と状態遷移メソッド

バックグラウンド処理とやり取りする `AppMessage` 型、fetch のリクエスト生成 (`plan_fetch`)・結果適用 (`apply_fetch_result`)、mutation のカウンタ操作 (`begin_mutation`/`finish_mutation`) を純ロジックとして実装しテストする。`loading: bool` を `pending_requests: usize` に置き換える。

**Files:**
- Modify: `src/app/mod.rs`（`loading` 削除・`pending_requests`/`fetch_generation` 追加・`AppMessage`/`FetchRequest` 型・メソッド・テスト）

**Interfaces:**
- Consumes: `AppState.view_start`（Task 1）, `AppState.fetched_start/end`（Task 2）, `AppState::replace_events`（既存）
- Produces:
  - `AppState.pending_requests: usize`（旧 `loading: bool`）
  - `AppState.fetch_generation: u64`
  - `pub struct FetchRequest { pub generation: u64, pub start: String, pub end: String, pub range: (NaiveDate, NaiveDate) }`
  - `pub enum AppMessage { FetchResult { generation: u64, range: (NaiveDate, NaiveDate), events: HashMap<NaiveDate, Vec<NotionEvent>>, error: Option<String> }, MutationResult { error: Option<String> } }`
  - `AppState::plan_fetch(&mut self) -> FetchRequest`
  - `AppState::apply_fetch_result(&mut self, generation: u64, range: (NaiveDate, NaiveDate), events: HashMap<NaiveDate, Vec<NotionEvent>>, error: Option<String>)`
  - `AppState::begin_mutation(&mut self)` / `finish_mutation(&mut self, error: Option<String>)`

- [ ] **Step 1: 失敗するテストを書く**

`src/app/mod.rs` のテストモジュールに追加:

```rust
    #[test]
    fn test_plan_fetch_increments_generation_and_pending() {
        let mut state = AppState::new(vec![]);
        assert_eq!(state.fetch_generation, 0);
        assert_eq!(state.pending_requests, 0);

        let req = state.plan_fetch();

        assert_eq!(req.generation, 1);
        assert_eq!(state.fetch_generation, 1);
        assert_eq!(state.pending_requests, 1);
        // 範囲は view_start の 1 週間前 〜 2 週間後
        assert_eq!(req.range.0, state.view_start - chrono::Duration::weeks(1));
        assert_eq!(req.range.1, state.view_start + chrono::Duration::weeks(2));
    }

    #[test]
    fn test_apply_fetch_result_latest_generation_applies() {
        let mut state = AppState::new(vec![]);
        let req = state.plan_fetch();
        let day = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let mut events = HashMap::new();
        events.insert(day, vec![make_event("e1", "Event 1", day)]);

        state.apply_fetch_result(req.generation, req.range, events, None);

        assert_eq!(state.pending_requests, 0);
        assert!(state.events.contains_key(&day));
        assert_eq!(state.fetched_start, Some(req.range.0));
        assert_eq!(state.fetched_end, Some(req.range.1));
    }

    #[test]
    fn test_apply_fetch_result_stale_generation_dropped() {
        let mut state = AppState::new(vec![]);
        let stale = state.plan_fetch(); // generation 1
        let _newer = state.plan_fetch(); // generation 2（最新）
        let day = NaiveDate::from_ymd_opt(2026, 3, 5).unwrap();
        let mut events = HashMap::new();
        events.insert(day, vec![make_event("stale", "old", day)]);

        state.apply_fetch_result(stale.generation, stale.range, events, None);

        // 古い世代なのでイベントは反映されない
        assert!(!state.events.contains_key(&day));
        // ただし pending は減る（2 → 1）
        assert_eq!(state.pending_requests, 1);
    }

    #[test]
    fn test_apply_fetch_result_error_sets_status() {
        let mut state = AppState::new(vec![]);
        let req = state.plan_fetch();

        state.apply_fetch_result(req.generation, req.range, HashMap::new(), Some("API Error: boom".to_string()));

        assert_eq!(state.pending_requests, 0);
        assert_eq!(state.status_message.as_deref(), Some("API Error: boom"));
    }

    #[test]
    fn test_mutation_counter() {
        let mut state = AppState::new(vec![]);
        state.begin_mutation();
        assert_eq!(state.pending_requests, 1);
        state.finish_mutation(None);
        assert_eq!(state.pending_requests, 0);
        assert!(state.status_message.is_none());

        state.begin_mutation();
        state.finish_mutation(Some("✗ boom".to_string()));
        assert_eq!(state.pending_requests, 0);
        assert_eq!(state.status_message.as_deref(), Some("✗ boom"));
    }
```

- [ ] **Step 2: テストを実行して失敗を確認**

Run: `rustup run stable cargo test`
Expected: FAIL（`plan_fetch` / `AppMessage` / `pending_requests` 未定義）

- [ ] **Step 3: 型・フィールド・メソッドを実装**

`src/app/mod.rs` 冒頭付近（`use` の下、`AppMode` の近く）に型を追加:

```rust
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub generation: u64,
    pub start: String, // "YYYY-MM-DD"
    pub end: String,   // "YYYY-MM-DD"
    pub range: (NaiveDate, NaiveDate),
}

#[derive(Debug)]
pub enum AppMessage {
    FetchResult {
        generation: u64,
        range: (NaiveDate, NaiveDate),
        events: HashMap<NaiveDate, Vec<NotionEvent>>,
        error: Option<String>,
    },
    MutationResult {
        error: Option<String>,
    },
}
```

`AppState` フィールド: `pub loading: bool,` を削除し、次を追加:

```rust
    pub pending_requests: usize,
    pub fetch_generation: u64,
```

`AppState::new`: `loading: false,` を削除し、次を追加:

```rust
            pending_requests: 0,
            fetch_generation: 0,
```

メソッド（`impl AppState` 内）:

```rust
    /// fetch リクエストを生成。世代番号を進め、pending をインクリメントする。
    pub fn plan_fetch(&mut self) -> FetchRequest {
        self.fetch_generation += 1;
        self.pending_requests += 1;
        let start = self.view_start - chrono::Duration::weeks(1);
        let end = self.view_start + chrono::Duration::weeks(2);
        FetchRequest {
            generation: self.fetch_generation,
            start: start.format("%Y-%m-%d").to_string(),
            end: end.format("%Y-%m-%d").to_string(),
            range: (start, end),
        }
    }

    /// fetch 結果を適用。最新世代のみ反映し、古い世代は破棄する。
    pub fn apply_fetch_result(
        &mut self,
        generation: u64,
        range: (NaiveDate, NaiveDate),
        events: HashMap<NaiveDate, Vec<NotionEvent>>,
        error: Option<String>,
    ) {
        self.pending_requests = self.pending_requests.saturating_sub(1);
        if generation != self.fetch_generation {
            return; // stale: 破棄
        }
        match error {
            Some(e) => self.status_message = Some(e),
            None => {
                self.replace_events(events);
                self.fetched_start = Some(range.0);
                self.fetched_end = Some(range.1);
                self.status_message = None;
            }
        }
    }

    pub fn begin_mutation(&mut self) {
        self.pending_requests += 1;
    }

    pub fn finish_mutation(&mut self, error: Option<String>) {
        self.pending_requests = self.pending_requests.saturating_sub(1);
        if let Some(e) = error {
            self.status_message = Some(e);
        }
    }
```

- [ ] **Step 4: テストを実行して成功を確認**

Run: `rustup run stable cargo test`
Expected: `app` のテストは PASS（`main.rs` のビルドエラーは Task 4 で解消）。`rustup run stable cargo build 2>&1 | grep -c current_week_start` などで残エラーが `main.rs` に限られることを確認する。

- [ ] **Step 5: コミット**

```bash
git add src/app/mod.rs
git commit -m "feat: add AppMessage + async state transitions (plan/apply fetch, mutation counter)"
```

---

### Task 4: main.rs — mpsc 導入と fetch 非同期化

`run_app` に mpsc チャネルを導入し、`fetch_events(...).await` をバックグラウンド spawn (`spawn_fetch`) に置き換え、結果を `try_recv()` で取り込む。ローディング表示を `pending_requests > 0` に切り替える。

**Files:**
- Modify: `src/main.rs`（`use`・`main`・`run_app`・`render_status_bar`・`fetch_events` 削除・`spawn_fetch`/`apply_message` 追加）

**Interfaces:**
- Consumes: `AppState::plan_fetch`/`apply_fetch_result`/`needs_fetch`/`finish_mutation`（Task 2/3）, `AppMessage`（Task 3）, `NotionClient`（`Clone`）, `api::parse_event_with_keys`（既存）
- Produces:
  - `fn spawn_fetch(state: &mut AppState, tx: &tokio::sync::mpsc::UnboundedSender<app::AppMessage>, client: &api::NotionClient, databases: &[config::DatabaseConfig])`
  - `fn apply_message(state: &mut AppState, msg: app::AppMessage, tx: &tokio::sync::mpsc::UnboundedSender<app::AppMessage>, client: &api::NotionClient, databases: &[config::DatabaseConfig])`

- [ ] **Step 1: `use` とインポートを追加**

`src/main.rs` の `use` 群に追加:

```rust
use tokio::sync::mpsc;
```

`use app::AppState;` の行を次に変更（`AppMessage` を使うため）:

```rust
use app::{AppMessage, AppState};
```

- [ ] **Step 2: `main` の初期 fetch を削除**

`main` 関数内の以下の行を**削除**する（初期取得は `run_app` 側で spawn する）:

```rust
    fetch_events(&client, &mut state, &cfg.databases).await;
```

- [ ] **Step 3: `spawn_fetch` と `apply_message` を追加**

`src/main.rs` の末尾（旧 `fetch_events` を置き換える位置）に追加。**旧 `async fn fetch_events(...)` は削除する。**

```rust
fn spawn_fetch(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
    databases: &[config::DatabaseConfig],
) {
    let req = state.plan_fetch();
    let tx = tx.clone();
    let client = client.clone();
    let databases = databases.to_vec();
    tokio::spawn(async move {
        let mut fetched: HashMap<chrono::NaiveDate, Vec<api::models::NotionEvent>> = HashMap::new();
        let mut error: Option<String> = None;
        let mut had_success = false;

        for db in &databases {
            match client.query_database(&db.id, &req.start, &req.end).await {
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
                                fetched.entry(d).or_default().push(event);
                            }
                        }
                    }
                }
                Err(e) => {
                    error = Some(format!("API Error: {}", e));
                }
            }
        }

        // 1件でも成功していれば成功分を反映（エラーは無視）
        let final_error = if had_success { None } else { error };
        let _ = tx.send(AppMessage::FetchResult {
            generation: req.generation,
            range: req.range,
            events: fetched,
            error: final_error,
        });
    });
}

fn apply_message(
    state: &mut AppState,
    msg: AppMessage,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
    databases: &[config::DatabaseConfig],
) {
    match msg {
        AppMessage::FetchResult {
            generation,
            range,
            events,
            error,
        } => {
            state.apply_fetch_result(generation, range, events, error);
        }
        AppMessage::MutationResult { error } => {
            state.finish_mutation(error.clone());
            if error.is_none() {
                // 変更が反映された最新データを取得
                spawn_fetch(state, tx, client, databases);
            }
        }
    }
}
```

- [ ] **Step 4: `run_app` にチャネルと受信・初期 fetch を組み込む**

`run_app` 冒頭（`let mut pending_d = false;` の下）に追加:

```rust
    let (tx, mut rx) = mpsc::unbounded_channel::<AppMessage>();
    // 初期ロード
    spawn_fetch(state, &tx, client, databases);
```

`loop {` の直後（`terminal.draw` の前）にバックグラウンド結果の取り込みを追加:

```rust
        while let Ok(msg) = rx.try_recv() {
            apply_message(state, msg, &tx, client, databases);
        }
```

- [ ] **Step 5: ナビゲーション系キーハンドラの `.await` を除去**

`run_app` の Normal モード各アームを次のように置き換える。`fetch_events(...).await` を削除し、必要時のみ `spawn_fetch` する。

```rust
                            KeyCode::Char('H') => {
                                pending_d = false;
                                state.prev_week();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('L') => {
                                pending_d = false;
                                state.next_week();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
```

```rust
                            KeyCode::Char('h') => {
                                pending_d = false;
                                state.select_prev_day();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
                            KeyCode::Char('l') => {
                                pending_d = false;
                                state.select_next_day();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
```

```rust
                            KeyCode::Char('t') => {
                                pending_d = false;
                                state.go_to_today();
                                if state.needs_fetch() {
                                    spawn_fetch(state, &tx, client, databases);
                                }
                            }
```

（`j`/`k`/`Tab`/`n`/`e`/`d`/`Left`/`Right`/`q` のアームは変更なし。旧 `h`/`l` にあった `week_before` 比較ロジックは不要になるので削除する。）

- [ ] **Step 6: ローディング表示を `pending_requests` に切り替え**

`render_status_bar`（256行目付近）の `state.loading` 参照を置換:

```rust
        ui::status_bar_text(state.pending_requests > 0, error)
```

同関数内のスタイル分岐（262行目付近）:

```rust
    } else if state.pending_requests > 0 {
```

- [ ] **Step 7: ビルドとテストを実行**

Run: `rustup run stable cargo build`
Expected: `handle_form_submit` / `handle_delete_confirm` がまだ旧シグネチャ（`.await`）で残るため、それらの呼び出し箇所は Task 5 まで一時的にコンパイルエラーになり得る。ビルドを通すため、Task 5 と本タスクは連続して実装し、Task 5 完了後に一括ビルドする。ここでは `cargo build 2>&1` の残エラーが `handle_form_submit`/`handle_delete_confirm` 関連のみであることを確認する。

- [ ] **Step 8: コミット（Task 5 完了後にまとめて可）**

Task 5 と密結合のため、Task 5 の Step でまとめてコミットする。

---

### Task 5: フォーム送信・削除の非同期化

`handle_form_submit` / `handle_delete_confirm` を非 async 化し、バリデーション後に即フォーム/確認を閉じてバックグラウンド spawn する。呼び出し側の `.await` を除去する。

**Files:**
- Modify: `src/main.rs`（`handle_form_submit`・`handle_delete_confirm`・`run_app` の Form/Confirm アーム）

**Interfaces:**
- Consumes: `AppState::begin_mutation`（Task 3）, `AppMessage::MutationResult`（Task 3）, `client.create_page`/`update_page`/`archive_page`（既存）, `app::form_logic::validate_form`/`form_to_date_strings`（既存）
- Produces:
  - `fn handle_form_submit(state: &mut AppState, tx: &mpsc::UnboundedSender<AppMessage>, client: &api::NotionClient)`（非 async）
  - `fn handle_delete_confirm(state: &mut AppState, tx: &mpsc::UnboundedSender<AppMessage>, client: &api::NotionClient)`（非 async）

- [ ] **Step 1: `handle_form_submit` を非同期 spawn 版に置き換え**

`src/main.rs` の `async fn handle_form_submit(...)` 全体を次に置き換える。引数から `databases` を外し（送信後の fetch は `MutationResult` 経由で行う）、`db` は `form.db_index` を `state.databases` から引く。

```rust
fn handle_form_submit(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
) {
    let form = match state.form.clone() {
        Some(f) => f,
        None => return,
    };

    if let Some(err) = app::form_logic::validate_form(&form) {
        state.status_message = Some(err);
        return;
    }

    let db = match state.databases.get(form.db_index) {
        Some(d) => d.clone(),
        None => {
            state.status_message = Some("DBが選択されていません".to_string());
            return;
        }
    };

    let title_prop = db.title_property.clone().unwrap_or_else(|| "Name".to_string());
    let date_prop = db.date_property.clone().unwrap_or_else(|| "Date".to_string());
    let (date_start, date_end) = app::form_logic::form_to_date_strings(&form);

    // 即座にフォームを閉じてナビゲーションへ戻す
    state.close_form();
    state.begin_mutation();

    let tx = tx.clone();
    let client = client.clone();
    let mode = form.mode.clone();
    let title = form.title.clone();
    let editing_id = form.editing_event_id.clone();
    let db_id = db.id.clone();
    let select = db.create_profile.select.clone();

    tokio::spawn(async move {
        let result: anyhow::Result<()> = match mode {
            app::FormMode::Create => client
                .create_page(
                    &db_id,
                    &title,
                    &date_start,
                    date_end.as_deref(),
                    &title_prop,
                    &date_prop,
                    &select,
                )
                .await
                .map(|_| ()),
            app::FormMode::Edit => match editing_id {
                Some(id) => client
                    .update_page(
                        &id,
                        &title,
                        &date_start,
                        date_end.as_deref(),
                        &title_prop,
                        &date_prop,
                    )
                    .await
                    .map(|_| ()),
                None => Ok(()),
            },
        };
        let error = result.err().map(|e| format!("✗ {}", e));
        let _ = tx.send(AppMessage::MutationResult { error });
    });
}
```

- [ ] **Step 2: `handle_delete_confirm` を非同期 spawn 版に置き換え**

`async fn handle_delete_confirm(...)` 全体を次に置き換える。

```rust
fn handle_delete_confirm(
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
    client: &api::NotionClient,
) {
    let page_id = match &state.mode {
        app::AppMode::Confirm(app::ConfirmAction::DeleteEvent(id)) => id.clone(),
        _ => return,
    };

    state.mode = app::AppMode::Normal;
    state.status_message = None;
    state.begin_mutation();

    let tx = tx.clone();
    let client = client.clone();
    tokio::spawn(async move {
        let error = client
            .archive_page(&page_id)
            .await
            .err()
            .map(|e| format!("✗ {}", e));
        let _ = tx.send(AppMessage::MutationResult { error });
    });
}
```

- [ ] **Step 3: 呼び出し側の `.await` を除去**

`run_app` の Form モードアーム（215行目付近）:

```rust
                            KeyCode::Enter => {
                                handle_form_submit(state, &tx, client);
                            }
```

Confirm モードアーム（235行目付近）:

```rust
                    app::AppMode::Confirm(_) => match code {
                        KeyCode::Char('y') | KeyCode::Char('Y') => {
                            handle_delete_confirm(state, &tx, client);
                        }
                        _ => {
                            state.mode = app::AppMode::Normal;
                            state.status_message = None;
                        }
                    },
```

- [ ] **Step 4: ビルドとテストを実行**

Run: `rustup run stable cargo build`
Expected: SUCCESS（警告のみ許容）

Run: `rustup run stable cargo test`
Expected: 全テスト PASS

- [ ] **Step 5: 手動動作確認**

Run: `rustup run stable cargo run`
確認項目:
- `h`/`l` で表示ウィンドウが1日ずつスライドし、選択日が常に画面内に収まる。
- `H`/`L` で週送り、`t` で今日の週（月曜始まり）へ戻る。
- `n` でイベント作成 → Enter 直後にフォームが閉じてナビゲーション可能、少し後に一覧へ反映。作成中も `h`/`l`/`j`/`k` が効く。
- `dd`→`y` で削除しても UI が固まらない。
- 通信中はステータスバーがローディング表示、完了で消える。

（設定ファイル未整備で API を叩けない環境ではビルド＋`cargo test` までを完了条件とし、手動確認は環境のある所で実施する。）

- [ ] **Step 6: コミット**

```bash
git add src/main.rs
git commit -m "feat: run all Notion API calls in background (non-blocking UI)"
```

---

### Task 6: ドキュメント更新

挙動変更を `CLAUDE.md` と `MEMORY.md` に反映する。

**Files:**
- Modify: `CLAUDE.md`（キーバインド表・AppState 説明）
- Modify: `/Users/kubotadaichi/.claude/projects/-Users-kubotadaichi-dev-github-Noca/memory/MEMORY.md`

**Interfaces:**
- Consumes: なし
- Produces: なし

- [ ] **Step 1: `CLAUDE.md` を更新**

- キーバインド表の `h`/`l` を「前日/次日選択（1日ローリングスライド）」、`H`/`L` を「前週/次週移動」に修正。
- 「主要な設計ポイント > AppState」の `current_week_start` を `view_start`（表示ウィンドウ左端、月曜固定廃止）に修正。
- 「通信は全てバックグラウンド（mpsc + tokio::spawn）、`pending_requests` でローディング管理、fetch は `fetch_generation` で stale 破棄、`needs_fetch()` でプリフェッチ判定」を追記。

- [ ] **Step 2: `MEMORY.md` を更新**

上記と同内容を `AppState` セクション・キーバインドセクションに反映。

- [ ] **Step 3: コミット**

```bash
git add CLAUDE.md
git commit -m "docs: update keybindings and AppState notes for rolling window + async"
```

（`MEMORY.md` は git 管理外のためコミット対象は `CLAUDE.md` のみ）

---

## Self-Review

**Spec coverage:**
- パート1 ローリングウィンドウ → Task 1（`view_start` リネーム・1日追従・`go_to_today`）
- プリフェッチ範囲・`needs_fetch` → Task 2
- `AppMessage`・`pending_requests`・世代・plan/apply → Task 3
- mpsc 導入・fetch 非同期・ローディング表示 → Task 4
- 送信/削除の非同期化・フォーム即クローズ → Task 5
- ドキュメント反映 → Task 6
- 非対象（楽観的更新・キャンセル）→ 計画に含めず（設計の YAGNI に合致）

**Type consistency:**
- `view_start`（Task 1）を Task 2/3/4 で一貫使用。
- `plan_fetch -> FetchRequest`、`apply_fetch_result(generation, range, events, error)`、`AppMessage::FetchResult { generation, range, events, error }` のフィールド名・型が Task 3 定義と Task 4 使用で一致。
- `spawn_fetch`/`apply_message`/`handle_form_submit`/`handle_delete_confirm` の引数順（`state, tx, client[, databases]`）が定義と呼び出しで一致。
- `NotionClient: Clone`、`DatabaseConfig: Clone`、`create_profile.select: HashMap<String,String>` は既存コードで確認済み。

**Placeholder scan:** プレースホルダなし。全コードステップに具体コードを記載。
