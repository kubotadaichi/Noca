use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub auth: AuthConfig,
    #[serde(default)]
    pub databases: Vec<DatabaseConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthConfig {
    pub integration_token: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_color")]
    pub color: String,
    pub date_property: Option<String>,
    pub title_property: Option<String>,
    #[serde(default = "default_event_style")]
    pub event_style: String,
}

fn default_color() -> String {
    "blue".to_string()
}

fn default_event_style() -> String {
    "block".to_string()
}

pub fn config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("noca")
        .join("config.toml")
}

pub fn load() -> Result<Config> {
    let path = config_path();
    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("設定ファイルが見つかりません: {}", path.display()))?;
    let config: Config =
        toml::from_str(&content).with_context(|| "config.toml のパースに失敗しました")?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
[auth]
integration_token = "secret_test_token"

[[databases]]
id = "aaaa-bbbb"
name = "My DB"
color = "green"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.auth.integration_token, "secret_test_token");
        assert_eq!(config.databases.len(), 1);
        assert_eq!(config.databases[0].name, "My DB");
        assert_eq!(config.databases[0].color, "green");
    }

    #[test]
    fn test_default_color() {
        let toml_str = r#"
[auth]
integration_token = "secret_test"

[[databases]]
id = "aaaa-bbbb"
name = "My DB"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.databases[0].color, "blue");
    }

    #[test]
    fn test_optional_property_names() {
        let toml_str = r#"
[auth]
integration_token = "secret"

[[databases]]
id = "aaaa"
name = "Work"
color = "green"
date_property = "開催日"
title_property = "タスク名"
event_style = "bar"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let db = &config.databases[0];
        assert_eq!(db.date_property, Some("開催日".to_string()));
        assert_eq!(db.title_property, Some("タスク名".to_string()));
        assert_eq!(db.event_style, "bar");
    }

    #[test]
    fn test_optional_property_names_defaults() {
        let toml_str = r#"
[auth]
integration_token = "secret"

[[databases]]
id = "aaaa"
name = "Work"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        let db = &config.databases[0];
        assert_eq!(db.date_property, None);
        assert_eq!(db.title_property, None);
        assert_eq!(db.event_style, "block");
    }
}
