# Noca Design Document
Date: 2026-03-05

## Overview

Notion CalendarのTUIクライアント。RustとratatuiでNotion Calendar相当の操作をターミナル上で実現する。

## Goals

- MVP: Notion DBのカレンダービュー・タスク一覧の閲覧（読み取り専用）
- 将来: イベント/タスクのCRUD、タイムブロッキング、OAuthによる認証

## Architecture

レイヤード構造（1クレート）:

```
noca/
├── src/
│   ├── main.rs
│   ├── api/        # Notion APIクライアント（認証・HTTP）
│   ├── app/        # アプリ状態・ビジネスロジック
│   ├── ui/         # ratatuiウィジェット・レンダリング
│   └── config/     # 設定管理（~/.config/noca/）
```

## UI Layout

Notion Calendarデスクトップアプリを参考にした週ビュー中心のレイアウト。

```
┌────────────────┬────────────────────────────────────────────────────┐
│  ミニ月カレンダー │  2026 3月  10週目        [週] [今日] [<][>]       │
│                │─────┬──────┬──────┬──────┬──────┬──────┬──────    │
│  スケジュール   │終日 │(月)  │(火)  │(水)  │(木)  │(金)  │(土)(日)  │
│────────────────│─────┼──────┼──────┼──────┼──────┼──────┼──────    │
│  DBリスト       │07:00│      │      │      │      │      │          │
│  ■ DB名        │08:00│      │      │      │      │      │          │
│  ■ DB名        │09:00│━━━━━━━━━━━━━━(現在時刻)━━━━━━━━━━━━━━━━      │
│  + DB追加       │10:00│      │      │      │      │      │          │
└────────────────┴─────┴──────┴──────┴──────┴──────┴──────┴──────────┘
 [h/l]週移動  [j/k]スクロール  [tab]パネル切替  [d]日ビュー  [q]終了
```

- 左サイドバー: ミニ月カレンダー + Notion DBリスト（色付きアイコン）
- メインエリア: 週ビュー（デフォルト）
- 終日行: 時刻なしイベントを表示
- 時間スロット: 時刻ありイベントをブロック表示
- 現在時刻ライン表示

## Data Flow

```
起動
 ↓
~/.config/noca/config.toml 読み込み
 ↓
Notion API → DB一覧・イベント取得（tokio非同期）
 ↓
AppState に格納（現在週・イベントMap<日付, Vec<Event>>）
 ↓
ratatui レンダリングループ
 ↓
キー入力 → AppState更新 → 再描画
```

キャッシュ戦略（MVP）:
- 起動時に前後2週分を取得
- 週移動時に未取得分を非同期フェッチ
- メモリキャッシュのみ（ファイルキャッシュは将来）

## Configuration

`~/.config/noca/config.toml`:

```toml
[auth]
integration_token = "secret_xxx"

[[databases]]
id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
name = "All.2!.db"
color = "green"

[[databases]]
id = "yyyyyyyy-yyyy-yyyy-yyyy-yyyyyyyyyyyy"
name = "Tasks.3"
color = "yellow"
```

初回起動時にconfigが存在しない場合、セットアップガイドをTUI上で表示する。

## Dependencies

| 用途       | クレート                  |
|-----------|--------------------------|
| TUI       | `ratatui` + `crossterm`  |
| HTTP      | `reqwest`                |
| 非同期     | `tokio`                  |
| JSON      | `serde` + `serde_json`   |
| 設定      | `toml` + `dirs`          |
| 日付      | `chrono`                 |
| エラー     | `anyhow`                 |

## Error Handling

- `anyhow` でアプリ全体のエラーを統一
- API失敗時はTUIのステータスバーに表示（クラッシュしない）
- ネットワーク断時はキャッシュデータで表示継続

## Authentication

- MVP: Notion Integration Token（`~/.config/noca/config.toml`に保存）
- 将来: OAuth 2.0（ブラウザ経由）へ移行可能な設計

## Testing

- `api/`: モックHTTPサーバーで単体テスト
- `app/`: AppState更新ロジックを純粋関数として単体テスト
- `ui/`: ratatuiの`TestBackend`でスナップショットテスト
- E2E: MVP対象外

## Future Roadmap

1. イベント・タスクのCRUD
2. タイムブロッキング
3. 複数DB対応（既存DB + テンプレートDB自動作成）
4. OAuth認証
5. 日ビュー・月ビュー切り替え
