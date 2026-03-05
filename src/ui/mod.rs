pub mod sidebar;
pub mod week_view;

pub fn help_text() -> &'static str {
    "[h/l]週移動  [j/k]スクロール  [H/L]日選択  [t]今日  [Tab]切替  [q]終了"
}

#[cfg(test)]
mod tests {
    use super::help_text;

    #[test]
    fn test_help_text_contains_main_keybindings() {
        let text = help_text();
        assert!(text.contains("[h/l]"));
        assert!(text.contains("[j/k]"));
        assert!(text.contains("[H/L]"));
        assert!(text.contains("[t]"));
        assert!(text.contains("[Tab]"));
        assert!(text.contains("[q]"));
    }
}
