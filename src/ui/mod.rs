use ratatui::style::Color;

pub mod form;
pub mod sidebar;
pub mod week_view;

pub fn help_text() -> &'static str {
    "[H/L]週移動  [h/l]日選択  [j/k]カーソル  [n]新規  [e]編集  [dd]削除  [t]今日  [Tab]切替  [q]終了"
}

pub fn status_bar_text(loading: bool, error: Option<&str>) -> String {
    if let Some(msg) = error {
        format!("✗ {}", msg)
    } else if loading {
        "読み込み中...".to_string()
    } else {
        help_text().to_string()
    }
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

#[cfg(test)]
mod tests {
    use super::{help_text, status_bar_text};

    #[test]
    fn test_help_text_contains_main_keybindings() {
        let text = help_text();
        assert!(text.contains("[H/L]"));
        assert!(text.contains("[h/l]"));
        assert!(text.contains("[j/k]"));
        assert!(text.contains("[n]"));
        assert!(text.contains("[e]"));
        assert!(text.contains("[dd]"));
        assert!(text.contains("[t]"));
        assert!(text.contains("[Tab]"));
        assert!(text.contains("[q]"));
    }

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
}
