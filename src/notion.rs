//! Notion API Client Module
//!
//! Provides async access to the Notion API for querying databases, pages, and datasources.

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde_json::{json, Value};

pub const NOTION_API_VERSION: &str = "2025-09-03";
pub const NOTION_BASE_URL: &str = "https://api.notion.com/v1";
pub const SOURCE_ID: &str = "93f885016df945c8ade315557cefd023";

pub struct NotionClient {
    client: reqwest::Client,
    base_url: String,
}

impl NotionClient {
    pub fn new(api_key: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert(
            "Notion-Version",
            HeaderValue::from_static(NOTION_API_VERSION),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        NotionClient {
            client,
            base_url: NOTION_BASE_URL.to_string(),
        }
    }

    /// List all users in the workspace.
    pub async fn list_users(&self) -> Result<Value, reqwest::Error> {
        let url = format!("{}/users", self.base_url);
        let response = self.client.get(&url).send().await?;
        response.json().await
    }

    /// Search for pages by title.
    pub async fn search_pages(&self, query: &str) -> Result<Value, reqwest::Error> {
        let url = format!("{}/search", self.base_url);
        let body = json!({
            "query": query,
            "filter": {
                "property": "object",
                "value": "page"
            }
        });
        let response = self.client.post(&url).json(&body).send().await?;
        response.json().await
    }

    /// Retrieve a database by ID.
    pub async fn get_database(&self, database_id: &str) -> Result<Value, reqwest::Error> {
        let url = format!("{}/databases/{}", self.base_url, database_id);
        let response = self.client.get(&url).send().await?;
        response.json().await
    }

    /// Query a database with optional filters.
    pub async fn query_database(
        &self,
        database_id: &str,
        filter_params: Option<Value>,
    ) -> Result<Value, reqwest::Error> {
        let url = format!("{}/databases/{}/query", self.base_url, database_id);
        let body = match filter_params {
            Some(filter) => json!({ "filter": filter }),
            None => json!({}),
        };
        let response = self.client.post(&url).json(&body).send().await?;
        response.json().await
    }

    /// Query a specific datasource database with optional filters.
    pub async fn query_datasource(
        &self,
        source_id: &str,
        filter_params: Option<Value>,
    ) -> Result<Value, reqwest::Error> {
        let url = format!("{}/data_sources/{}/query", self.base_url, source_id);
        let body = filter_params.unwrap_or(json!({}));
        let response = self.client.post(&url).json(&body).send().await?;
        response.json().await
    }

    /// Retrieve a page by ID.
    pub async fn get_page(&self, page_id: &str) -> Result<Value, reqwest::Error> {
        let url = format!("{}/pages/{}", self.base_url, page_id);
        let response = self.client.get(&url).send().await?;
        response.json().await
    }

    /// Get all child blocks of a page or block.
    pub async fn get_block_children(&self, block_id: &str) -> Result<Value, reqwest::Error> {
        let url = format!("{}/blocks/{}/children", self.base_url, block_id);
        let response = self.client.get(&url).send().await?;
        response.json().await
    }
}

/// Extract tasks from a datasource response that have "To Do" or "Doing" status.
pub fn extract_active_tasks(datasource_response: &Value) -> Vec<String> {
    let mut tasks = Vec::new();

    if let Some(results) = datasource_response
        .get("results")
        .and_then(|r| r.as_array())
    {
        for res in results {
            let status_name = res
                .get("properties")
                .and_then(|p| p.get("Status"))
                .and_then(|s| s.get("select"))
                .and_then(|s| s.get("name"))
                .and_then(|n| n.as_str());

            if matches!(status_name, Some("To Do" | "Doing")) {
                if let Some(titles) = res
                    .get("properties")
                    .and_then(|p| p.get("Name"))
                    .and_then(|name_prop| name_prop.get("title"))
                    .and_then(|title_arr| title_arr.as_array())
                {
                    for title in titles {
                        if let Some(text) = title.get("plain_text").and_then(|t| t.as_str()) {
                            tasks.push(text.to_string());
                        }
                    }
                }
            }
        }
    }

    tasks
}
