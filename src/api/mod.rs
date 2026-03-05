pub mod models;

use anyhow::{anyhow, Context, Result};
use models::{NotionEvent, PageObject, QueryResponse};
use reqwest::Client;
use serde_json::json;
use std::time::Duration;

const NOTION_API_BASE: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";

#[derive(Clone)]
pub struct NotionClient {
    client: Client,
    token: String,
}

impl NotionClient {
    pub fn new(token: String) -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(8))
                .build()
                .expect("failed to build reqwest client"),
            token,
        }
    }

    pub async fn query_database(
        &self,
        database_id: &str,
        start_date: &str,
        end_date: &str,
    ) -> Result<Vec<PageObject>> {
        let url = format!("{}/databases/{}/query", NOTION_API_BASE, database_id);
        for date_property in ["Date", "日付"] {
            let body = build_query_body(date_property, start_date, end_date);
            let response = self
                .client
                .post(&url)
                .header("Authorization", format!("Bearer {}", self.token))
                .header("Notion-Version", NOTION_VERSION)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .context("Notion APIへの接続に失敗しました")?;

            let status = response.status();
            let raw = response
                .text()
                .await
                .context("Notion APIレスポンスの読み取りに失敗しました")?;

            if status.is_success() {
                let query_response: QueryResponse =
                    serde_json::from_str(&raw).context("Notion APIレスポンスのパースに失敗しました")?;
                return Ok(query_response.results);
            }

            let error_json: serde_json::Value = serde_json::from_str(&raw).unwrap_or_else(|_| {
                json!({
                    "code": "unknown_error",
                    "message": raw,
                })
            });

            if is_missing_property_error(&error_json) {
                continue;
            }

            let message = error_json
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(anyhow!("Notion APIがエラーを返しました ({}): {}", status, message));
        }

        Err(anyhow!(
            "Notion DBに対応する日付プロパティが見つかりません（Date/日付）"
        ))
    }
}

fn build_query_body(date_property: &str, start_date: &str, end_date: &str) -> serde_json::Value {
    json!({
        "page_size": 100,
        "filter": {
            "and": [
                {
                    "property": date_property,
                    "date": { "on_or_after": start_date }
                },
                {
                    "property": date_property,
                    "date": { "on_or_before": end_date }
                }
            ]
        }
    })
}

fn is_missing_property_error(error_json: &serde_json::Value) -> bool {
    let code = error_json
        .get("code")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let message = error_json
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    code == "validation_error" && message.contains("Could not find property with name or id")
}

/// PageObject から NotionEvent に変換する
pub fn parse_event(page: &PageObject, database_id: &str) -> Option<NotionEvent> {
    let props = &page.properties;

    // タイトルを取得（"名前" or "Name" プロパティ）
    let title = extract_title(props)?;

    // 日付を取得（"日付" or "Date" プロパティ）
    let (date_start, datetime_start, datetime_end, is_all_day) = extract_date(props)?;

    Some(NotionEvent {
        id: page.id.clone(),
        title,
        date_start,
        datetime_start,
        datetime_end,
        is_all_day,
        database_id: database_id.to_string(),
        color: None,
    })
}

fn extract_title(props: &serde_json::Value) -> Option<String> {
    for key in &["名前", "Name", "title", "Title"] {
        if let Some(title_prop) = props.get(key) {
            if let Some(arr) = title_prop["title"].as_array() {
                let text: String = arr
                    .iter()
                    .filter_map(|t| t["plain_text"].as_str())
                    .collect();
                if !text.is_empty() {
                    return Some(text);
                }
            }
        }
    }
    None
}

fn extract_date(
    props: &serde_json::Value,
) -> Option<(
    Option<chrono::NaiveDate>,
    Option<chrono::DateTime<chrono::Utc>>,
    Option<chrono::DateTime<chrono::Utc>>,
    bool,
)> {
    for key in &["日付", "Date", "date"] {
        if let Some(date_prop) = props.get(key) {
            if let Some(start_str) = date_prop["date"]["start"].as_str() {
                // 日付のみ（YYYY-MM-DD）か日時（YYYY-MM-DDTHH:MM:SS...）か判定
                if start_str.len() == 10 {
                    let date = start_str.parse::<chrono::NaiveDate>().ok()?;
                    return Some((Some(date), None, None, true));
                } else {
                    let dt = chrono::DateTime::parse_from_rfc3339(start_str)
                        .ok()?
                        .with_timezone(&chrono::Utc);
                    let end_dt = date_prop["date"]["end"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|d| d.with_timezone(&chrono::Utc));
                    return Some((None, Some(dt), end_dt, false));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_page(props: serde_json::Value) -> PageObject {
        PageObject {
            id: "test-id".to_string(),
            properties: props,
        }
    }

    #[test]
    fn test_parse_all_day_event() {
        let page = make_page(json!({
            "Name": { "title": [{ "plain_text": "テストイベント" }] },
            "Date": { "date": { "start": "2026-03-05" } }
        }));
        let event = parse_event(&page, "db-1").unwrap();
        assert_eq!(event.title, "テストイベント");
        assert!(event.is_all_day);
        assert!(event.datetime_start.is_none());
    }

    #[test]
    fn test_parse_timed_event() {
        let page = make_page(json!({
            "Name": { "title": [{ "plain_text": "会議" }] },
            "Date": { "date": {
                "start": "2026-03-05T10:00:00+09:00",
                "end": "2026-03-05T11:00:00+09:00"
            }}
        }));
        let event = parse_event(&page, "db-1").unwrap();
        assert_eq!(event.title, "会議");
        assert!(!event.is_all_day);
        assert!(event.datetime_start.is_some());
        assert!(event.datetime_end.is_some());
    }

    #[test]
    fn test_parse_no_date_returns_none() {
        let page = make_page(json!({
            "Name": { "title": [{ "plain_text": "日付なし" }] }
        }));
        let event = parse_event(&page, "db-1");
        assert!(event.is_none());
    }

    #[test]
    fn test_build_query_body_uses_selected_date_property() {
        let body = build_query_body("Date", "2025-12-11", "2025-12-11");
        assert_eq!(body["filter"]["and"][0]["property"], "Date");
        assert_eq!(body["filter"]["and"][1]["property"], "Date");
    }

    #[test]
    fn test_is_missing_property_error() {
        let err = json!({
            "object": "error",
            "status": 400,
            "code": "validation_error",
            "message": "Could not find property with name or id: 日付"
        });
        assert!(is_missing_property_error(&err));

        let other = json!({
            "object": "error",
            "status": 401,
            "code": "unauthorized",
            "message": "Invalid token"
        });
        assert!(!is_missing_property_error(&other));
    }
}
