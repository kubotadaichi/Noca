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
}

fn default_color() -> String {
    "blue".to_string()
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
}
