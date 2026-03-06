[English](README.md)

# Noca

Notion Database を週ビューで閲覧・作成・編集・削除できる、Rust 製の TUI カレンダークライアントです。

## インストール

### Homebrew（推奨）

```bash
brew tap kubotadaichi/noca
brew install noca
```

### ソースからビルド

```bash
git clone https://github.com/kubotadaichi/Noca
cd Noca
rustup run stable cargo build --release
cp target/release/noca /usr/local/bin/
```

## アンインストール

```bash
brew uninstall noca
brew untap kubotadaichi/noca  # tap ごと削除する場合
```

## 主な機能

- 左: ミニ月カレンダー + DBリスト
- 右: 週ビュー（終日行 + 15分スロット）
- キー操作で週移動・日選択・スクロール
- イベントの作成・編集・削除（キーボードのみ）
- Notion の日付プロパティを `Date` / `日付` の順で自動フォールバック

## 必要環境

- Notion Integration Token（[こちらから取得](https://www.notion.so/my-integrations)）
- 参照対象の Notion Database ID
- Integration を対象 DB に Share 済みであること

## 設定ファイル

`dirs::config_dir()/noca/config.toml` を読み込みます。

- macOS: `~/Library/Application Support/noca/config.toml`
- Linux: `~/.config/noca/config.toml`

例:

```toml
[auth]
integration_token = "secret_xxx"

[[databases]]
id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
name = "My Calendar"
color = "green"
# オプション: DBのプロパティ名が異なる場合に指定
# date_property = "日付"
# title_property = "タスク名"

# オプション: イベント作成時に自動設定するデフォルト select プロパティ
[databases.create_profile.select]
# GTD = "🕑Remind"
```

## 起動

```bash
noca
```

ソースからビルドした場合:

```bash
rustup run stable cargo run
```

## キーバインド

### ノーマルモード

| キー | 動作 |
|------|------|
| `h` / `l` | 前週 / 次週 |
| `j` / `k` | カーソルを上 / 下へ移動（時間スロット） |
| `H` / `L` | 前日 / 次日を選択 |
| `t` | 今日へ移動 |
| `Tab` | サイドバー / カレンダー切替 |
| `n` | 新規作成フォームを開く（選択日・カーソル時刻をプリセット） |
| `e` | カーソル位置のイベントを編集 |
| `dd` | カーソル位置のイベントを削除（確認あり） |
| `q` | 終了 |

### フォームモード（`n` / `e`）

| キー | 動作 |
|------|------|
| `Tab` / `Shift+Tab` | 次 / 前のフィールドへ移動 |
| `Space` | 終日フラグをトグル（IsAllDay フィールド選択中） |
| `←` / `→` | DB を切り替え（DB フィールド選択中） |
| `Enter` | 保存 |
| `Esc` | キャンセル |

## トラブルシュート

- 画面が空のまま
  - 対象週にイベントが無い可能性があります。`h` / `l` で週を移動してください。
  - DB 側の日付プロパティ名が `Date` または `日付` であることを確認してください。
  - Integration が DB に Share されていることを確認してください。

- 起動時に設定ファイルが見つからない
  - 上記 OS 別パスに `config.toml` を配置してください。

## リリース

タグを push すると GitHub Actions が自動でバイナリをビルドし、GitHub Releases に公開します。

```bash
# Cargo.toml の version を更新後
git tag v0.x.0
git push origin v0.x.0
```

リリース後は `homebrew-noca` リポジトリの `Formula/noca.rb` を更新してください（`version`、`url`、`sha256`）。

## 開発者向け

```bash
rustup run stable cargo test
rustup run stable cargo build
```

## 現状の制約

- OAuth 未対応（Integration Token 前提）
