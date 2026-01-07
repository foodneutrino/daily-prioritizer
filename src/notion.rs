//! Notion API Client Module
//!
//! Provides synchronous access to the Notion API for querying databases, pages, and datasources.

use anyhow::{Context, Result};
use log::info;
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use serde_json::{json, Value};

pub const NOTION_API_VERSION: &str = "2025-09-03";
pub const NOTION_BASE_URL: &str = "https://api.notion.com/v1";
pub const SOURCE_ID: &str = "93f885016df945c8ade315557cefd023";

pub struct NotionClient {
    api_key: String,
    base_url: String,
}

impl NotionClient {
    pub fn new(api_key: &str) -> Self {
        NotionClient {
            api_key: api_key.to_string(),
            base_url: NOTION_BASE_URL.to_string(),
        }
    }

    fn create_client(&self) -> Result<Client<EspHttpConnection>> {
        let config = Configuration {
            use_global_ca_store: true,
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            ..Default::default()
        };
        let connection = EspHttpConnection::new(&config)
            .context("Failed to create HTTP connection")?;
        Ok(Client::wrap(connection))
    }

    fn make_get_request(&self, url: &str) -> Result<Value> {
        let mut client = self.create_client()?;
        let auth_header = format!("Bearer {}", self.api_key);
        let headers = [
            ("Authorization", auth_header.as_str()),
            ("Content-Type", "application/json"),
            ("Notion-Version", NOTION_API_VERSION),
        ];

        let request = client.request(Method::Get, url, &headers)?;
            // .context("Failed to create request")?;
        let mut response = request.submit()?;
            // .context("Failed to submit request")?;

        let mut body = Vec::new();
        if response.status() == 200 {
            response.read(&mut body)?;
        }
        
        // response.read_to_end(&mut body)
        //     .context("Failed to read response body")?;

        serde_json::from_slice(&body)
            .context("Failed to parse JSON response")
    }

    fn make_post_request(&self, url: &str, body: &Value) -> Result<Value> {
        let mut client = self.create_client()?;
        let auth_header = format!("Bearer {}", self.api_key);
        let body_str = serde_json::to_string(body)?;
        let content_length = body_str.len().to_string();
        let headers = [
            ("Authorization", auth_header.as_str()),
            ("Content-Type", "application/json"),
            ("Content-Length", content_length.as_str()),
            ("Notion-Version", NOTION_API_VERSION),
        ];

        info!("Making POST request to URL: {} with body: {}", url, body_str);
        let mut request = client.request(Method::Post, url, &headers)
            .context("Failed to create request")?;
        request.write(&body_str.as_bytes())
            .context("Failed to get request writer")?;

        let mut response: esp_idf_svc::http::client::Response<&mut EspHttpConnection> = request.submit()
            .context("Failed to submit request")?;

        let mut buf = [0u8; 1024];
        let mut response_body = Vec::<u8>::new();
        info!("Response status: {}", response.status());

        loop {
            match response.read(&mut buf) {
                Ok(0) => break,
                Ok(len) => {
                    info!("Read {} bytes from response", len);
                    response_body.extend_from_slice(&buf[..len]);
                }
                Err(e) => {
                    info!("Error reading response: {:?}", e);
                    break;
                }
            }
        }

        serde_json::from_slice(&response_body)
            .context("Failed to parse JSON response")
    }

    /// List all users in the workspace.
    pub fn list_users(&self) -> Result<Value> {
        let url = format!("{}/users", self.base_url);
        self.make_get_request(&url)
    }

    /// Search for pages by title.
    pub fn search_pages(&self, query: &str) -> Result<Value> {
        let url = format!("{}/search", self.base_url);
        let body = json!({
            "query": query,
            "filter": {
                "property": "object",
                "value": "page"
            }
        });
        self.make_post_request(&url, &body)
    }

    /// Retrieve a database by ID.
    pub fn get_database(&self, database_id: &str) -> Result<Value> {
        let url = format!("{}/databases/{}", self.base_url, database_id);
        self.make_get_request(&url)
    }

    /// Query a database with optional filters.
    pub fn query_database(
        &self,
        database_id: &str,
        filter_params: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}/databases/{}/query", self.base_url, database_id);
        let body = match filter_params {
            Some(filter) => json!({ "filter": filter }),
            None => json!({}),
        };
        self.make_post_request(&url, &body)
    }

    /// Query a specific datasource database with optional filters.
    pub fn query_datasource(
        &self,
        source_id: &str,
        filter_params: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}/data_sources/{}/query", self.base_url, source_id);
        let body = filter_params.unwrap_or(json!({}));
        self.make_post_request(&url, &body)
    }

    /// Retrieve a page by ID.
    pub fn get_page(&self, page_id: &str) -> Result<Value> {
        let url = format!("{}/pages/{}", self.base_url, page_id);
        self.make_get_request(&url)
    }

    /// Get all child blocks of a page or block.
    pub fn get_block_children(&self, block_id: &str) -> Result<Value> {
        let url = format!("{}/blocks/{}/children", self.base_url, block_id);
        self.make_get_request(&url)
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
