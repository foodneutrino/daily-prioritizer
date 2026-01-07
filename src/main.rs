//! Daily Prioritizer
//!
//! Combines Google Calendar free time analysis with Notion task management
//! to help prioritize your daily work.

mod calendar;
mod notion;
mod waveshare;
mod wifi;

use chrono::Local;
use log::info;
use anyhow::Result;

use esp_idf_sys as _;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

use wifi::wifi_up;

use waveshare::{Epd, FrameBuffer};

/// Fetch and display Google Calendar events and free time slots
fn fetch_calendar_events() {
    info!("--- Google Calendar ---");

    let access_token = match calendar::get_credentials() {
        Ok(token) => token,
        Err(e) => {
            info!("Failed to get calendar credentials: {}", e);
            return;
        }
    };

    let events = match calendar::get_todays_events(&access_token) {
        Ok(events) => events,
        Err(e) => {
            info!("Failed to fetch calendar events: {}", e);
            return;
        }
    };

    let (busy_periods, free_slots) = calendar::calculate_free_time(&events);

    info!(
        "Working hours: {}:00 - {}:00",
        calendar::WORK_START_HOUR, calendar::WORK_END_HOUR
    );

    if !busy_periods.is_empty() {
        info!("\nScheduled events:");
        for period in &busy_periods {
            let duration = calendar::format_duration(period.end - period.start);
            info!(
                "  {} - {} ({}): {}",
                period.start.format("%H:%M"),
                period.end.format("%H:%M"),
                duration,
                period.title
            );
        }
    } else {
        info!("\nNo events scheduled today!");
    }

    info!("\nFree time slots:");
    if !free_slots.is_empty() {
        let mut total_free = chrono::Duration::zero();
        for slot in &free_slots {
            let duration = slot.end - slot.start;
            total_free = total_free + duration;
            info!(
                "  {} - {} ({})",
                slot.start.format("%H:%M"),
                slot.end.format("%H:%M"),
                calendar::format_duration(duration)
            );
        }
        info!("\nTotal free time: {}", calendar::format_duration(total_free));
    } else {
        info!("  No free time available during working hours!");
    }
}

/// Fetch and display active Notion tasks
fn fetch_notion_tasks() -> Result<Vec<String>>{
    info!("--- Notion Tasks ---");

    let api_key = match option_env!("NOTION_API_KEY") {
        Some(key) => key,
        None => {
            info!("NOTION_API_KEY not set. Skipping Notion tasks.");
            return Err(anyhow::anyhow!("No Notion API Key"));
        }
    };

    info!("API Key found, querying Notion {}", api_key);
    let notion_client = notion::NotionClient::new(api_key);

    let datasource_response = notion_client.query_datasource(notion::SOURCE_ID, None)?;

    Ok(notion::extract_active_tasks(&datasource_response))
}

fn main() -> Result<()> {
    EspLogger::initialize_default();

    info!("Daily Prioritizer");
    info!("{}", "=".repeat(50));

    let now = Local::now();
    info!("Date: {}\n", now.format("%A, %B %d, %Y"));

    // Setup wifi
    esp_idf_sys::link_patches();

    let system_peripherals = esp_idf_hal::peripherals::Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;
    let session_wifi = wifi_up(system_peripherals.modem, sys_loop, nvs)?;
    info!(
        "DHCP server assigned IP address: {:?}",
        session_wifi.wifi().sta_netif().get_ip_info()?
    );

    // fetch_calendar_events();
    info!("\n{}", "-".repeat(50));
    let tasks = fetch_notion_tasks()?;
    if !tasks.is_empty() {
        info!("Active tasks (To Do / Doing):");
        for task in &tasks {
            info!("  - {}", task);
        }
        info!("\nTotal tasks: {}", tasks.len());
    } else {
        info!("No active tasks found!");
    }

    info!("\n{}", "=".repeat(50));
    info!("Daily planning complete!");

    let mut epd = Epd::new(system_peripherals.pins, system_peripherals.spi2);

    info!("Resetting the screen...");
    epd.init();
    epd.clear();

    // Create framebuffer
    let mut fb = FrameBuffer::new(epd.width(), epd.height());
    info!("Created buffer of size: {} bytes", fb.buffer().len());

    const BLACK: u8 = 0x00;
    const WHITE: u8 = 0x01;

    info!("Displaying tasks on the screen...");
    fb.fill(WHITE);
    fb.text("Current Tasks", 30, 10, BLACK);
    let mut y = 20;
    for task in &tasks {
        fb.text(task, 10, y, BLACK);
        y += 10;
    }
    // fb.pixel(30, 10, BLACK);
    // fb.hline(30, 30, 10, BLACK);
    // fb.vline(30, 50, 10, BLACK);
    // fb.line(30, 70, 40, 80, BLACK);
    // fb.rect(30, 90, 10, 10, BLACK);
    // fb.fill_rect(30, 110, 10, 10, BLACK);
    // for row in 0..36 {
    //     let row_str = row.to_string();
    //     fb.text(&row_str, 0, row * 8, BLACK);
    // }
    // fb.text("Line 36", 0, 288, BLACK);

    epd.display(fb.buffer());
    epd.sleep();

    Ok(())
}
