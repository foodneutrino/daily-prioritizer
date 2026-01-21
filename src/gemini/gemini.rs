use anyhow::{Result, Context};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json;
use std::time::Duration;
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};

const GEMINI_API_BASE_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models";
const DEFAULT_MODEL: &str = "gemini-3-flash-preview";

pub const DEFAULT_PROMPT: &str = r#"
I want you to act as a high-performance productivity coach. I have a list of tasks and specific free time windows. Your goal is to break these tasks into chunks of 15–30 minutes so I can make progress without feeling overwhelmed.

My Tasks:
    {{tasks}}

My Free Time Slots:
    {{timeslots}}

Requirements:
    Decompose: Break each task into a sequence of 'Micro-Steps.' No step should take more than 20 minutes.
    Energy Mapping: Match high-effort brain tasks to my morning slot and physical/administrative tasks to my afternoon slot.
    Format: Present this as a chronological schedule. I need to programmatically process the response so identify the format should be '--__-- HH:MM - HH:MM: Task to do"
"#;

#[derive(Serialize)]
pub struct PromptTemplate {
    pub timeslots: Vec<String>,
    pub tasks: Vec<String>,
}

#[derive(Debug, Serialize)]
struct GenerateContentRequest {
    contents: Vec<Content>,
}

#[derive(Debug, Serialize)]
struct Content {
    parts: Vec<Part>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Part {
    text: String,
}

#[derive(Debug, Deserialize)]
struct GenerateContentResponse {
    candidates: Vec<Candidate>,
}

#[derive(Debug, Deserialize)]
struct Candidate {
    content: ContentResponse,
}

#[derive(Debug, Deserialize)]
struct ContentResponse {
    parts: Vec<Part>,
}

pub struct GeminiClient {
    client: Option<Client<EspHttpConnection>>,
    api_key: String,
    model: String,
    base_url: String,
}

impl GeminiClient {
    pub fn new(apikey: &str) -> Self {
        Self {
            client: None,
            api_key: apikey.to_string(),
            model: DEFAULT_MODEL.to_string(),
            base_url: GEMINI_API_BASE_URL.to_string(),
        }
    }

    fn create_client() -> Result<Client<EspHttpConnection>> {
        let config = Configuration {
            use_global_ca_store: true,
            crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
            timeout: Some(Duration::from_secs(60)),
            ..Default::default()
        };
        let connection = EspHttpConnection::new(&config)
            .context("Failed to create HTTP connection")?;
        Ok(Client::wrap(connection))
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = base_url.to_string();
        self
    }

    pub fn generate_content(&mut self, prompt: &str) -> Result<String> {

        if self.client.is_none() {
            self.client = Some(Self::create_client()?);
        }
        let local_client = self.client.as_mut().unwrap();

        let url = format!(
            "{}/{}:generateContent?key={}",
            self.base_url, self.model, self.api_key
        );

        let request_body = GenerateContentRequest {
            contents: vec![Content {
                parts: vec![Part {
                    text: prompt.to_string(),
                }],
            }],
        };
        let body_str = serde_json::to_string(&request_body)?;
        let content_length = body_str.len().to_string();

        let auth_str = format!("x-goog-api-key:{}", self.api_key);
        let headers = [
            ("Authorization", auth_str.as_str()),
            ("Content-Type", "application/json"),
            ("Content-Length", content_length.as_str()),
        ];

        info!("Gemini Request [URL: {}] with [body: {}]", url, body_str);

        let mut response = match local_client.request(Method::Post, &url, &headers) {
            Ok(mut req) => {
                req.write(&body_str.as_bytes()).context("Failed to get request writer")?;

                req.submit().context("Failed to submit request")?

            },
            Err(e) => {
                info!("Failed to create request: {:?}", e);
                return Err(anyhow::anyhow!("Failed to create request: {:?}", e));
            }
        };

        info!("Response status: {}", response.status());

        let mut buf = [0u8; 10240];
        let mut response_body = Vec::<u8>::new();
        loop {
            match response.read(&mut buf) {
                Ok(0) => break,
                Ok(len) => {
                    info!("Read {} bytes from response", len);
                    response_body.extend_from_slice(&buf[..len]);
                }
                Err(e) => {
                    info!("Error reading response: {:?}", e);
                    return Err(anyhow::anyhow!("Error reading response: {:?}", e));
                }
            }
        }

        self.client = None;

        let response: GenerateContentResponse = serde_json::from_slice(&response_body)
            .context("Failed to parse JSON response")?;

        Ok(response
            .candidates
            .first()
            .and_then(|c| c.content.parts.first())
            .map(|p| p.text.clone())
            .ok_or_else(|| anyhow::anyhow!("No text in response"))?)
    }
}