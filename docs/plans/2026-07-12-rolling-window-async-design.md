# ローリング7日ウィンドウ + 全通信の非同期化 設計書

作成日: 2026-07-12

## 背景・目的

現状の Noca には2つの操作上の不満がある。

1. **週表示が月曜固定で日単位にずらせない**
   表示ウィンドウは `current_week_start`（月曜始まり）に固定され、`h`/`l` で選択日を動かして週境界を跨ぐと `current_week_start` が7日単位で更新される。結果、表示が1週間ガクッと飛び、「1日ずつずらして見る」ことができない。

2. **通信中に操作が完全停止する**
   `fetch_events` / `create_page` / `update_page` / `archive_page` をイベントループ内で `.await` するため、通信が返るまで再描画も入力も止まる。作成・削除・週移動のたびに UI がフリーズする。

本設計はこの2点を解消する。

## 要件

- 表示される7日間の起点を1日ずつスライドできる（ローリング7日ウィンドウ）。月曜固定は廃止。
- 全ての Notion API 通信（取得・作成・編集・削除）をバックグラウンドで実行し、通信中も再描画・入力・ナビゲーションを継続できる。
- 送信（作成/編集/削除）時はフォームを即座に閉じてナビゲーションに戻り、完了後に自動で一覧を更新する。
- 既存の68テストを壊さない（該当するものは新仕様に合わせて更新する）。

## パート1: ローリング7日ウィンドウ

### 状態モデルの変更 (`src/app/mod.rs`)

- `current_week_start: NaiveDate` を **`view_start: NaiveDate`** にリネームし、意味を「表示ウィンドウの左端日」に変更する（月曜始まりの制約を外す）。
- `week_dates()` は変更なし: `view_start` から7日を返す。

### ナビゲーションロジック

- `select_next_day()`:
  - `selected_date += 1 day`
  - `if selected_date > view_start + 6 days { view_start += 1 day }`（**1日だけ**追従）
- `select_prev_day()`:
  - `selected_date -= 1 day`
  - `if selected_date < view_start { view_start -= 1 day }`（**1日だけ**追従）
- `next_week()` / `prev_week()`（H/L に割り当て）: `view_start` を7日スライド。従来通り。
- `go_to_today()`: `selected_date = today`、`view_start = week_start_of(today)`。
  - `t` は「月曜始まりの今日の週へスナップして復帰」する役割。日次ローリングは `h`/`l` で行う。

`week_start_of()` は `go_to_today()` および初期化 (`AppState::new`) でのみ使用する（引き続き月曜計算に利用）。

### キーバインド

変更なし（挙動のみ変わる）。
- `h`/`l`: 前日/次日選択（1日ローリング）
- `H`/`L`: 前週/次週（7日スライド）
- `t`: 今日の週（月曜始まり）へスナップ

## パート2: 全通信の非同期化

### アーキテクチャ概要

メインループとネットワーク処理を `tokio::sync::mpsc` チャネルで分離する。

```
┌─────────────┐   spawn(fetch/create/update/archive)   ┌──────────────┐
│  main loop  │ ─────────────────────────────────────▶ │ tokio tasks  │
│ (draw+input)│ ◀──────── AppMessage via mpsc ───────── │ (NotionClient)│
└─────────────┘                                         └──────────────┘
```

- メインループは各周回で `event::poll(200ms)` により入力を待ちつつ、`rx.try_recv()`（ノンブロッキング）でバックグラウンド結果を取り込む。待機中も UI は回り続ける。
- `NotionClient` を `Arc<NotionClient>`（または内部で `Arc` を持つ `Clone` 実装）にして各 spawn へ move する。
- 各タスクは処理完了後、結果を `AppMessage` として `tx.send()` する。

### メッセージ型 (`src/app/mod.rs` もしくは新規 `src/app/message.rs`)

```rust
pub enum AppMessage {
    /// fetch 完了。generation はリクエスト発行時の世代番号。
    FetchResult {
        generation: u64,
        events: HashMap<NaiveDate, Vec<NotionEvent>>,
        error: Option<String>,
    },
    /// 作成/編集/削除 完了。成功時は再 fetch をトリガする。
    MutationResult {
        error: Option<String>,
    },
}
```

### 状態フィールドの追加・変更 (`AppState`)

- `loading: bool` → **`pending_requests: usize`**。spawn 時に +1、結果受信時に -1。`> 0` の間ローディング表示。
- `fetch_generation: u64` を追加。fetch を発行するたびに +1 し、その値をタスクへ渡す。
- `fetched_start: Option<NaiveDate>` / `fetched_end: Option<NaiveDate>` を追加。取得済み範囲を記録。

### stale 結果対策

`h`/`l` 連打で複数 fetch が in-flight になり得る。`FetchResult` を受信したら `generation == fetch_generation`（最新）のときだけ `replace_events` する。古い世代の結果は破棄（`pending_requests` のデクリメントのみ行う）。

### fetch トリガ戦略（プリフェッチ）

- 3週間分プリフェッチは維持する。fetch 範囲は `[view_start - 1週, view_start + 2週]`。
- 取得済み範囲 `fetched_start`/`fetched_end` を保持し、`view_start` が範囲端のマージン（例: 3日）内に近づいたとき、または未取得（初回・`t`・週移動で範囲外）のときだけ新たな fetch を spawn する。
- 判定ヘルパ `fn needs_fetch(&self) -> bool` を `AppState` に用意する。
- ローリングのたびに毎回 API を叩かない。かつ非同期なので UI は止まらない。

### 送信フロー (`handle_form_submit`)

1. バリデーション（同期、失敗なら `status_message` にセットして return）。
2. 成功なら **即座に `state.close_form()`** してナビゲーションへ戻す。
3. `create_page` / `update_page` をバックグラウンド spawn。`pending_requests += 1`。
4. タスク完了 → `MutationResult` 送信。
5. メインループ受信時: エラーなら `status_message`、成功なら fetch を spawn して一覧更新。

### 削除フロー (`handle_delete_confirm`)

1. `Confirm` モードから `page_id` を取り出し、即 `Normal` モードへ戻す。
2. `archive_page` をバックグラウンド spawn。`pending_requests += 1`。
3. 完了 → `MutationResult` → 成功なら fetch を spawn。

### メインループの変更 (`src/main.rs`)

- `run_app` の冒頭で `let (tx, mut rx) = mpsc::unbounded_channel();`（または bounded）。
- ループ先頭で `while let Ok(msg) = rx.try_recv() { apply_message(state, msg, &tx, &client, databases); }` を回す。
- 各キーハンドラの `fetch_events(...).await` を「fetch を spawn する」ヘルパ `spawn_fetch(state, &tx, &client, databases)` 呼び出しに置き換える。`.await` はループから消える。
- `spawn_fetch` は `fetch_generation += 1`、`pending_requests += 1`、`view_start` を捕捉して `tokio::spawn` 内で `query_database` をループ実行し `FetchResult` を送る。

## エラーハンドリング

- API エラーは従来通り `status_message` に格納して赤色ステータスバー表示。
- 複数 DB のうち一部成功時は成功分を反映（既存の `had_success` ロジックをタスク内へ移す）。
- チャネル送信失敗（受信側 drop）はアプリ終了時のみ発生し得るので無視。

## テスト方針

ユニットテスト（`#[cfg(test)]`）で検証可能な純ロジックを対象とする。ネットワーク・tokio spawn 部分は薄いオーケストレーションに留め、ロジックを `AppState` メソッドへ寄せてテスト可能にする。

- `view_start` リネームに伴う既存テスト（`test_next_prev_week`, `test_select_next_day_follows_week` 等）の更新。
- `select_next_day` / `select_prev_day` がウィンドウを**1日だけ**動かすことの検証（旧: 週単位 → 新: 日単位）。
- `go_to_today` が `view_start = week_start_of(today)` にすることの検証。
- `needs_fetch()` の境界テスト（範囲内では false、マージン内・範囲外では true）。
- `fetch_generation` インクリメントと stale 判定のテスト（最新世代のみ反映）。
- `pending_requests` カウンタの増減テスト。

## 影響範囲まとめ

| ファイル | 変更内容 |
|---------|---------|
| `src/app/mod.rs` | `current_week_start`→`view_start`、`select_*_day` の1日追従化、`go_to_today`、`pending_requests`/`fetch_generation`/`fetched_start`/`fetched_end` 追加、`needs_fetch()`、`AppMessage` 型、既存テスト更新 |
| `src/main.rs` | mpsc チャネル導入、`spawn_fetch`/`apply_message` ヘルパ、キーハンドラの `.await` 除去、送信/削除の非同期化、`Arc<NotionClient>` |
| `src/api/mod.rs` | `NotionClient` を `Arc` で共有可能に（必要なら `Clone` 導出/内部 Arc 化） |
| `src/ui/*` | `current_week_start` 参照箇所を `view_start` に追随。ローディング表示を `pending_requests > 0` に追随 |
| `CLAUDE.md` / `MEMORY.md` | 挙動変更の追記（実装後） |

## 非対象（YAGNI）

- 楽観的更新（送信結果を待たずローカルにイベントを追加/削除して即描画）は行わない。完了後の再 fetch で反映する。
- リクエストのキャンセル（in-flight fetch の中断）は行わない。stale 判定で結果を捨てるのみ。
- 日ビュー・月ビューなど別ビューの追加は対象外（別途ロードマップ）。
