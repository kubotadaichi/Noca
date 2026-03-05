[日本語版はこちら](README_ja.md)

# Noca

A read-only TUI calendar client for Notion Databases, written in Rust. View your Notion events in a weekly layout right in your terminal.

## Installation

### Homebrew (recommended)

```bash
brew tap kubotadaichi/noca
brew install noca
```

### Build from source

```bash
git clone https://github.com/kubotadaichi/Noca
cd Noca
rustup run stable cargo build --release
cp target/release/noca /usr/local/bin/
```

## Uninstall

```bash
brew uninstall noca
brew untap kubotadaichi/noca  # also remove the tap
```

## Features

- Left panel: mini month calendar + database list
- Right panel: weekly view (all-day row + time slots in 15-minute increments)
- Keyboard navigation for week/day movement and scrolling
- Auto-fallback for Notion date properties: tries `Date` then `日付`

## Requirements

- Notion Integration Token ([get one here](https://www.notion.so/my-integrations))
- Notion Database ID to display
- Integration must be shared with the target database

## Configuration

Reads config from `dirs::config_dir()/noca/config.toml`:

- macOS: `~/Library/Application Support/noca/config.toml`
- Linux: `~/.config/noca/config.toml`

Example:

```toml
[auth]
integration_token = "secret_xxx"

[[databases]]
id = "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
name = "My Calendar"
color = "green"
```

## Usage

```bash
noca
```

If built from source:

```bash
rustup run stable cargo run
```

## Keybindings

| Key | Action |
|-----|--------|
| `h` / `l` | Previous / next week |
| `j` / `k` | Scroll time slots down / up |
| `H` / `L` | Select previous / next day |
| `t` | Go to today |
| `Tab` | Toggle sidebar / calendar focus |
| `q` | Quit |

## Troubleshooting

**Screen is empty**
- There may be no events in the current week — use `h` / `l` to navigate.
- Check that your database has a date property named `Date` or `日付`.
- Make sure the Integration is shared with the database.

**Config file not found on startup**
- Place `config.toml` at the OS-specific path listed above.

## Releasing

Pushing a tag triggers GitHub Actions to build binaries and publish a GitHub Release automatically.

```bash
# After bumping version in Cargo.toml
git tag v0.x.0
git push origin v0.x.0
```

After the release, update `Formula/noca.rb` in the `homebrew-noca` repository (`version`, `url`, `sha256`).

## Development

```bash
rustup run stable cargo test
rustup run stable cargo build
```

## Current Limitations (MVP)

- Read-only (no create / edit / delete)
- No OAuth support (Integration Token only)
