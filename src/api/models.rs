use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NotionEvent {
    pub id: String,
    pub title: String,
    pub date_start: Option<NaiveDate>,
    pub datetime_start: Option<DateTime<Utc>>,
    pub datetime_end: Option<DateTime<Utc>>,
    pub is_all_day: bool,
    pub database_id: String,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueryResponse {
    pub results: Vec<PageObject>,
    pub has_more: bool,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PageObject {
    pub id: String,
    pub properties: serde_json::Value,
}
