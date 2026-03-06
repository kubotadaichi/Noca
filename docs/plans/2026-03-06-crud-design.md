# Noca CRUD Design Document
Date: 2026-03-06

## Overview

Notion Calendar TUI クライアント Noca にイベントの作成・編集・削除機能を追加する。
読み取り専用の MVP に対して、Notion API の create/update/archive エンドポイントを利用した
フル CRUD 操作をキーボードのみで実現する。

## UI / キー操作

### カーソル移動（既存変更）

`j/k` にカーソル位置の概念を追加する。現在のスクロールに加え、`(selected_date, cursor_hour)` でイベントを選択状態にする。

- `j/k` → 時間カーソルを上下移動（画面端で自動スクロール）
- `H/L` → 日付選択（既存）
- カーソルのある `(selected_date, cursor_hour)` のイベントが「選択状態」

### 新規キーバインド

| キー | 動作 |
|------|------|
| `n` | 新規作成フォーム（選択日時をプリフィル） |
| `e` | 選択イベントを編集フォームで開く |
| `dd` | ステータスバーに確認表示 → `y` で削除 / `N` でキャンセル |

### フォームパネル（下部 4 行）

```
──────────────────────────────────────────────────────────
[新規] DB: [My Calendar    ]
タイトル: [                         ]  終日: [ ]
日付: [2026-03-06]  開始: [10:00]  終了: [11:00]
Tab で移動  Space で終日切替  Enter 確定  Esc キャンセル
```

- `Tab` / `Shift+Tab` でフィールド移動
- `Space` で終日トグル（ON 時は時刻フィールドをグレーアウト）
- `Enter` で確定（バリデーション → API 送信）
- `Esc` でキャンセル

## Architecture

### AppState への追加

```rust
pub enum AppMode {
    Normal,
    Form,
    Confirm(ConfirmAction),
}

pub enum ConfirmAction {
    DeleteEvent(String),  // page_id
}

// AppState に追加するフィールド
pub cursor_hour: u32,        // 時間カーソル (0-23), デフォルト 9
pub mode: AppMode,
pub form: Option<EventForm>,
```

### 新構造体 EventForm

```rust
pub enum FormMode { Create, Edit }

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

pub enum FormField {
    DbSelect, Title, Date, IsAllDay, StartTime, EndTime,
}
```

### 新ファイル

| ファイル | 役割 |
|----------|------|
| `src/ui/form.rs` | フォームパネルのレンダリング |
| `src/app/form_logic.rs` | フォーム入力処理・バリデーション |

### 既存ファイルの変更

| ファイル | 変更内容 |
|----------|---------|
| `src/app/mod.rs` | AppMode, EventForm, cursor_hour 追加 |
| `src/ui/week_view.rs` | カーソル描画追加 |
| `src/ui/mod.rs` | form モジュール追加 |
| `src/api/mod.rs` | create_page, update_page, archive_page 追加 |
| `src/main.rs` | Form/Confirm モードのキー処理追加 |

## API レイヤー

### 追加メソッド

```rust
impl NotionClient {
    pub async fn create_page(
        &self,
        database_id: &str,
        title: &str,
        date_start: &str,
        date_end: Option<&str>,
        title_prop: &str,
        date_prop: &str,
    ) -> Result<String>  // page_id を返す

    pub async fn update_page(
        &self,
        page_id: &str,
        title: &str,
        date_start: &str,
        date_end: Option<&str>,
        title_prop: &str,
        date_prop: &str,
    ) -> Result<()>

    pub async fn archive_page(&self, page_id: &str) -> Result<()>
}
```

### Notion API マッピング

| 操作 | HTTP | エンドポイント | ボディ |
|------|------|----------------|--------|
| Create | POST | `/pages` | `{"parent": {"database_id": "..."}, "properties": {...}}` |
| Update | PATCH | `/pages/{page_id}` | `{"properties": {...}}` |
| Delete | PATCH | `/pages/{page_id}` | `{"archived": true}` |

成功後は `fetch_events()` を呼んで週ビューを最新化する。

## バリデーション

| 条件 | エラーメッセージ |
|------|----------------|
| タイトルが空 | `タイトルを入力してください` |
| 日付フォーマット不正 | `日付は YYYY-MM-DD 形式で入力してください` |
| 終日 OFF かつ開始 ≥ 終了 | `終了時刻は開始時刻より後にしてください` |

バリデーションエラーはステータスバーに表示。フォームは閉じない。
API エラーも同様にステータスバーに表示し、フォームを残す（再試行可能）。

## テスト

| 対象 | 内容 |
|------|------|
| `app/form_logic.rs` | バリデーション関数の単体テスト |
| `api/mod.rs` | `build_create_body` / `build_update_body` の JSON 構造テスト |

## 追加修正（2026-03-06 追記）

### キーバインド再定義

- `H/L` : 前週 / 次週
- `h/l` : 前日 / 次日
- `j/k` : 時間カーソル上下（従来どおり）

### デフォルトヘルプ拡張

ステータスバーの通常ヘルプに CRUD キーを追加する。

- `n` : 新規作成
- `e` : 編集
- `dd` : 削除（確認あり）

### 作成時プロファイル（Config）

DBごとに、作成時のみ適用する Select プロパティ既定値を設定可能にする。

```toml
[[databases]]
id = "..."
name = "..."

[databases.create_profile.select]
GTD = "🕑Remind"
```

`create_page` では `create_profile.select` を `properties` にマージして送信する（編集時は未適用）。

### カーソル視認性改善

イベント上でもカーソル位置が消えないよう、選択行は `>` マーカー付きで強調表示する。
時間ラベル列・イベント列ともに同様の強調を適用する。

### 時刻編集UX改善

新規作成フォームの `開始/終了`（および日付）フィールドは初期値が埋まっているため、
最初の入力時に既存値をクリアして上書き入力できる仕様にする。
