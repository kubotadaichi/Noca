# Noca UX Polish Design

**Date:** 2026-03-05
**Scope:** Visual and UX improvements to the MVP

---

## Goal

Improve usability and visual clarity of the Noca TUI without adding new major features. Focus areas:

1. Config-driven property name mapping (removes hardcoded property names)
2. DB-color-based event rendering with selectable styles
3. Navigation consistency (H/L week auto-follow)
4. Status bar showing loading/error state

---

## Architecture

No structural changes to module layout. All changes are within existing modules:

- `src/config/mod.rs` — extend `DatabaseConfig`
- `src/api/mod.rs` — use config-supplied property names with fallback
- `src/app/mod.rs` — fix `select_next_day` / `select_prev_day` to auto-advance week
- `src/ui/week_view.rs` — render events using DB color and event_style
- `src/main.rs` — update status bar rendering and H/L key handler

---

## Section 1: Config Extension

Add optional fields to `DatabaseConfig`:

```toml
[[databases]]
id = "xxx"
name = "仕事カレンダー"
color = "green"
date_property = "開催日"     # optional, defaults to "Date" → "日付" fallback
title_property = "タスク名"  # optional, defaults to "Name" → "名前" fallback
event_style = "block"        # optional: "block" | "text" | "bar", default: "block"
```

- If `date_property` is unset, the existing `["Date", "日付"]` fallback loop is preserved.
- If `title_property` is unset, the existing `["名前", "Name", "title", "Title"]` fallback loop is preserved.
- `event_style` defaults to `"block"` if unset.

Future: a `noca --setup` interactive TUI will populate these fields by querying the Notion database schema.

---

## Section 2: Navigation Consistency

In `AppState::select_next_day` and `select_prev_day`, after updating `selected_date`, check if it falls outside the current display week and advance/retreat `current_week_start` accordingly.

```
if selected_date < current_week_start:
    current_week_start -= 1 week
if selected_date >= current_week_start + 7 days:
    current_week_start += 1 week
```

In `main.rs`, the `H` and `L` key handlers trigger `fetch_events` when the week changes (same as `h/l`).

---

## Section 3: Event Rendering

In `render_time_slots` (and `render_all_day_row`), replace the fixed green `Color::Green` style with event color derived from `event.color` (set from `db.color` during fetch).

Three styles controlled by `DatabaseConfig::event_style`:

| Style   | Appearance                                      |
|---------|-------------------------------------------------|
| `block` | Background color fill (default)                 |
| `text`  | Foreground text color only (current behavior)   |
| `bar`   | `▌` prefix in DB color + white text             |

Color lookup uses the existing `color_from_str()` helper.

---

## Section 4: Status Bar

Replace `render_help_bar` with `render_status_bar` that shows context-aware content in the bottom 1-line area:

| State       | Display                                          | Style    |
|-------------|--------------------------------------------------|----------|
| Normal      | `[h/l]週移動  [j/k]スクロール  [H/L]日選択  [t]今日  [q]終了` | DarkGray |
| Loading     | `読み込み中...`                                  | Yellow   |
| Error       | `✗ <status_message>`                            | Red      |

Priority: Error > Loading > Normal. After a successful fetch, `status_message` is cleared (already implemented in `fetch_events`).

---

## Testing

- Unit tests for updated `select_next_day` / `select_prev_day` (week auto-follow)
- Unit tests for config parsing of new optional fields with defaults
- Existing API tests remain valid; `query_database` signature unchanged
- Build verification after each task
