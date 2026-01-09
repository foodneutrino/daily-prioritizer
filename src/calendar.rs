//! Google Calendar Free Time Module
//!
//! Fetches events from Google Calendar for the current day and calculates free time slots.

use anyhow::{Context, Result};
use log::info;
use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use embedded_svc::http::client::Client;
use embedded_svc::http::Method;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

// Configuration
pub const WORK_START_HOUR: u32 = 9;
pub const WORK_END_HOUR: u32 = 17;
pub const CALENDAR_ID: &str = "foodneutrino@gmail.com";
const SCOPES: &str = "https://www.googleapis.com/auth/calendar.readonly";
const GOOGLE_CREDS: &str = include_str!("../free-time-calc-7daa6babd0ae.json");

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(Debug, Serialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
pub struct EventsResponse {
    pub items: Option<Vec<Event>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Event {
    pub summary: Option<String>,
    pub start: Option<EventTime>,
    pub end: Option<EventTime>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EventTime {
    #[serde(rename = "dateTime")]
    pub date_time: Option<String>,
    pub date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct BusyPeriod {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
    pub title: String,
}

#[derive(Debug, Clone)]
pub struct FreeSlot {
    pub start: NaiveDateTime,
    pub end: NaiveDateTime,
}

fn create_http_client() -> Result<Client<EspHttpConnection>> {
    info!("Creating HTTP client");
    let config = Configuration {
        use_global_ca_store: true,
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        ..Default::default()
    };
    info!("Creating HTTP client with config: {:?}", config);
    let connection = EspHttpConnection::new(&config)
        .context("Failed to create HTTP connection")?;
    
    info!("HTTP client created successfully");
    Ok(Client::wrap(connection))
}

fn get_access_token(key: &ServiceAccountKey) -> Result<String> {
    info!("Generating JWT for service account: {}", key.client_email);
    let now = Utc::now().timestamp();
    let claims = Claims {
        iss: key.client_email.clone(),
        scope: SCOPES.to_string(),
        aud: key.token_uri.clone(),
        exp: now + 3600,
        iat: now,
    };

    let header = Header::new(Algorithm::RS256);
    let encoding_key = EncodingKey::from_rsa_pem(key.private_key.as_bytes())?;

    let jwt = encode(&header, &claims, &encoding_key).context("Failed to encode JWT")?;
    info!("JWT generated successfully: {}", &jwt);

    let mut client = create_http_client()?;

    // Build form-encoded body
    let form_body = format!(
        "grant_type={}&assertion={}",
        urlencoded("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        urlencoded(&jwt)
    );
    let content_length = form_body.len().to_string();

    let headers = [
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Content-Length", content_length.as_str()),
    ];

    info!("Requesting access token from {}", key.token_uri);
    let mut request = client.request(Method::Post, &key.token_uri, &headers)
        .context("Failed to create token request")?;
    request.connection().write_all(form_body.as_bytes()).context("Failed to write request")?;
    // request.write(form_body.as_bytes())
    //     .context("Failed to get request writer")?;

    let mut response = request.submit()
        .context("Failed to request access token")?;

    let mut body = Vec::new();
    let buf = &mut [0u8; 1024];

    if response.status() == 200 {
        loop {
            match response.read(buf) {
                Ok(0) => break,
                Ok(len) => body.extend_from_slice(&buf[..len]),
                Err(e) => return Err(anyhow::anyhow!("Failed to read token response: {:?}", e)),
            }
        }
    }

    info!("Access token response received {:?}", body);
    let token_response: TokenResponse = serde_json::from_slice(&body)
        .context("Failed to parse token response")?;

    Ok(token_response.access_token)
}

pub fn get_credentials() -> Result<String> {
    info!("Google Credentials JSON: {}", GOOGLE_CREDS);
    let key: ServiceAccountKey =
        serde_json::from_str(&GOOGLE_CREDS).context("Failed to parse service account JSON")?;

    get_access_token(&key)
}

pub fn get_todays_events(access_token: &str) -> Result<Vec<Event>> {
    let now = Local::now();
    let start_of_day = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap();
    let end_of_day = start_of_day + Duration::days(1);

    let time_min = format!("{}Z", start_of_day.format("%Y-%m-%dT%H:%M:%S"));
    let time_max = format!("{}Z", end_of_day.format("%Y-%m-%dT%H:%M:%S"));

    // Build URL with query parameters
    let url = format!(
        "https://www.googleapis.com/calendar/v3/calendars/{}/events?timeMin={}&timeMax={}&singleEvents=true&orderBy=startTime",
        urlencoded(CALENDAR_ID),
        urlencoded(&time_min),
        urlencoded(&time_max)
    );

    let mut client = create_http_client()?;
    let auth_header = format!("Bearer {}", access_token);
    let headers = [
        ("Authorization", auth_header.as_str()),
    ];

    let request = client.request(Method::Get, &url, &headers)
        .context("Failed to create calendar request")?;
    let mut response = request.submit()
        .context("Failed to fetch calendar events")?;

    let mut body = Vec::new();
    let buf = &mut [0u8; 1024];

    if response.status() == 200 {
        loop {
            match response.read(buf) {
                Ok(0) => break,
                Ok(len) => body.extend_from_slice(&buf[..len]),
                Err(e) => return Err(anyhow::anyhow!("Failed to read event response: {:?}", e)),
            }
        }
    }

    let events_response: EventsResponse = serde_json::from_slice(&body)
        .context("Failed to parse events response")?;

    Ok(events_response.items.unwrap_or_default())
}

fn urlencoded(s: &str) -> String {
    s.replace("@", "%40")
        .replace(":", "%3A")
        .replace("+", "%2B")
}

pub fn parse_event_time(time: &Option<EventTime>) -> Option<NaiveDateTime> {
    let time = time.as_ref()?;

    if let Some(dt_str) = &time.date_time {
        // Parse RFC3339/ISO8601 datetime, stripping timezone for local handling
        if let Ok(dt) = DateTime::parse_from_rfc3339(dt_str) {
            return Some(dt.naive_local());
        }
        // Fallback: try parsing without timezone
        if let Ok(dt) = NaiveDateTime::parse_from_str(&dt_str[..19], "%Y-%m-%dT%H:%M:%S") {
            return Some(dt);
        }
    }

    if let Some(date_str) = &time.date {
        if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            return Some(date.and_hms_opt(0, 0, 0).unwrap());
        }
    }

    None
}

pub fn calculate_free_time(events: &[Event]) -> (Vec<BusyPeriod>, Vec<FreeSlot>) {
    let today = Local::now().date_naive();
    let work_start = today
        .and_time(NaiveTime::from_hms_opt(WORK_START_HOUR, 0, 0).unwrap());
    let work_end = today
        .and_time(NaiveTime::from_hms_opt(WORK_END_HOUR, 0, 0).unwrap());

    let mut busy_periods: Vec<BusyPeriod> = events
        .iter()
        .filter_map(|event| {
            let start = parse_event_time(&event.start)?;
            let end = parse_event_time(&event.end)?;

            // Clip to working hours
            let start = start.max(work_start);
            let end = end.min(work_end);

            if start < end {
                Some(BusyPeriod {
                    start,
                    end,
                    title: event.summary.clone().unwrap_or_else(|| "No title".to_string()),
                })
            } else {
                None
            }
        })
        .collect();

    busy_periods.sort_by_key(|p| p.start);

    // Calculate free slots
    let mut free_slots = Vec::new();
    let mut current_time = work_start;

    for period in &busy_periods {
        if current_time < period.start {
            free_slots.push(FreeSlot {
                start: current_time,
                end: period.start,
            });
        }
        current_time = current_time.max(period.end);
    }

    if current_time < work_end {
        free_slots.push(FreeSlot {
            start: current_time,
            end: work_end,
        });
    }

    (busy_periods, free_slots)
}

pub fn format_duration(duration: Duration) -> String {
    let total_minutes = duration.num_minutes();
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;

    match (hours, minutes) {
        (h, m) if h > 0 && m > 0 => format!("{}h {}m", h, m),
        (h, _) if h > 0 => format!("{}h", h),
        (_, m) => format!("{}m", m),
    }
}
