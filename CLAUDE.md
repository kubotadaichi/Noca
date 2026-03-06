# Noca - CLAUDE.md

Notion CalendarのTUIクライアント。RustとratatuiでNotion Calendarの週ビューをターミナル上で実現するCLIツール。

## ビルド・実行

```bash
# ビルド
rustup run stable cargo build

# 実行
rustup run stable cargo run

# テスト
rustup run stable cargo test

# リリースビルド
rustup run stable cargo build --release
```

**重要: cargoは必ず `rustup run stable cargo` を使うこと。**

## プロジェクト構成

```
src/
├── main.rs          # tokio main, TUIループ, キー入力処理, fetch_events, handle_form_submit, handle_delete_confirm
├── api/
│   ├── mod.rs       # NotionClient, parse_event_with_keys, build_query_body, build_create_body, build_update_body
│   └── models.rs    # NotionEvent, QueryResponse, PageObject
├── app/
│   ├── mod.rs       # AppState, AppMode, EventForm, FormField, FormMode, ConfirmAction, week_start_of
│   └── form_logic.rs # validate_form, form_to_date_strings, EventForm のフィールド操作メソッド
└── ui/
    ├── mod.rs        # color_from_str, help_text, status_bar_text
    ├── form.rs       # render_form_panel
    ├── sidebar.rs    # render_sidebar (ミニ月カレンダー + DBリスト)
    └── week_view.rs  # render_week_view (ヘッダー + 終日行 + 時間スロット + カーソルハイライト)
```

## 依存クレート

| クレート | 用途 |
|---------|------|
| ratatui + crossterm | TUI描画・入力 |
| reqwest | HTTP (timeout: 8s, features: json) |
| tokio | 非同期ランタイム |
| serde + serde_json | シリアライズ |
| toml + dirs | 設定ファイル |
| chrono | 日付・時刻 |
| anyhow | エラーハンドリング |

## 設定ファイル

`~/.config/noca/config.toml`（macOSは `~/Library/Application Support/noca/config.toml`）

```toml
[auth]
integration_token = "secret_xxx"

[[databases]]
id = "your-database-id"
name = "My Calendar"
color = "green"
# オプション
date_property = "日付"      # デフォルト: "Date" → "日付" の順で自動検出
title_property = "タスク名"  # デフォルト: 名前/Name/title/Title を自動検出
event_style = "block"       # "block" or "bar"

# イベント作成時のデフォルト select プロパティ値
[databases.create_profile.select]
GTD = "🕑Remind"
```

## キーバインド

### ノーマルモード

| キー | 動作 |
|------|------|
| `h` / `l` | 前週 / 次週移動 |
| `H` / `L` | 前日 / 次日選択（週跨ぎ対応） |
| `j` / `k` | カーソル移動（時間スロット）+ スクロール自動追従 |
| `t` | 今日に戻る |
| `Tab` | サイドバー / カレンダー切替 |
| `n` | 新規イベント作成フォームを開く（選択日・カーソル時刻プリセット） |
| `e` | カーソル位置のイベントを編集フォームで開く |
| `dd` | カーソル位置のイベントを削除（確認モードへ遷移） |
| `q` | 終了 |

### フォームモード（`n` / `e` で開く）

| キー | 動作 |
|------|------|
| `Tab` / `Shift+Tab` | 次 / 前のフィールドへ |
| `Space` | 終日フラグをトグル（IsAllDay フィールド選択中） |
| `←` / `→` | DB を切り替え（DbSelect フィールド選択中） |
| `Enter` | 保存（バリデーション → API送信 → fetch_events） |
| `Esc` | キャンセル |

### 確認モード（`dd` 後）

| キー | 動作 |
|------|------|
| `y` / `Y` | 削除を実行（archive_page → fetch_events） |
| その他 | キャンセル |

## 主要な設計ポイント

### AppState (`src/app/mod.rs`)
- `scroll_offset`: u16, 15分単位 (07:00 = 28, 22:00 = 88)
- `cursor_hour`: u32, 0-23。`j`/`k` で移動しスクロールを自動追従
- `mode`: AppMode::Normal / Form / Confirm(ConfirmAction)
- `form`: Option<EventForm>。フォームが開いているときのみ Some
- `replace_events()`: ID重複排除してイベントを差し替え（古いデータは消える）
- `select_next/prev_day()`: 週境界を超えたら `current_week_start` も自動更新
- `event_at_cursor()`: 選択日の `cursor_hour` と一致する datetime_start を持つイベントを返す
- 週はMonday始まり

### form_logic (`src/app/form_logic.rs`)
- `validate_form()`: タイトル必須・日付形式・開始<終了時刻チェック
- `form_to_date_strings()`: 終日は "YYYY-MM-DD"、時刻付きはローカルTZ offset 付き ISO8601
- `EventForm` メソッド: `next_field`, `prev_field`, `input_char`, `delete_char`, `toggle_all_day`, `db_next`, `db_prev`

### API (`src/api/mod.rs`)
- `query_database()`: フォールバック試行 "Date" → "日付"
- `create_page()`: parent.database_id + properties（title/date/select defaults）
- `update_page()`: PATCH /pages/{id}（title/date のみ更新）
- `archive_page()`: PATCH /pages/{id} `{ "archived": true }`
- `parse_event_with_keys()`: `title_property`, `date_property` はオプション指定可
- プロパティ自動検出順: 名前/Name/title/Title, 日付/Date/date

### fetch_events (`src/main.rs`)
- 現在週から3週間分を一括取得
- 複数DB対応: すべてのDBをループしてイベントをマージ
- エラーは `state.status_message` に格納（成功したDBがあれば表示）

## テスト

ユニットテストは各モジュールの `#[cfg(test)]` に記述 (合計68テスト):
- `src/app/mod.rs`: AppState のロジックテスト（週移動, スクロール, 重複排除, CRUD状態, cursor_hour など）
- `src/app/form_logic.rs`: バリデーション, フィールド操作, 日付文字列変換
- `src/api/mod.rs`: parse_event, build_query/create/update_body, is_missing_property_error
- `src/config/mod.rs`: config.toml パースのテスト（create_profile 含む）

```bash
rustup run stable cargo test
```

## 現状

週ビュー表示 + イベントCRUD（作成・編集・削除）実装済み。

## 今後の実装予定

1. タイムブロッキング
2. OAuth認証
3. 日ビュー・月ビュー切り替え
4. ファイルキャッシュ（オフライン対応）
