//! Daily Prioritizer
//!
//! Combines Google Calendar free time analysis with Notion task management
//! to help prioritize your daily work.

mod calendar;
mod notion;
mod waveshare;
mod wifi;

use chrono::Local;
use esp_idf_hal::{gpio::PinDriver, gpio::Pins, spi::SPI2};
use log::info;
use anyhow::Result;

use esp_idf_sys as _;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::{EspSntp, SntpConf};
use std::time::Duration;

use wifi::wifi_up;

use waveshare::{Epd, FrameBuffer};

fn sync_time() -> anyhow::Result<()> {
    log::info!("Initializing SNTP...");

    let sntp = EspSntp::new_default()?;

    // Wait for time to sync (timeout after 30 seconds)
    let mut retries = 0;
    while sntp.get_sync_status() != esp_idf_svc::sntp::SyncStatus::Completed {
        if retries >= 30 {
            anyhow::bail!("SNTP sync timeout");
        }
        std::thread::sleep(Duration::from_secs(1));
        retries += 1;
        log::info!("Waiting for SNTP sync... ({}s)", retries);
    }

    log::info!("Time synchronized!");
    Ok(())
}

/// Fetch and display Google Calendar events and free time slots
fn fetch_calendar_events() -> Result<()> {
    info!("--- Google Calendar ---");

    let access_token = calendar::get_credentials()?;

    let events = calendar::get_todays_events(&access_token)?;

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
    Ok(())
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

fn set_up_display(esp_peripheral_pins: Pins, spi: SPI2) -> Result<Epd<'static>> {
    
        let sck = esp_peripheral_pins.gpio12; 
        let mosi = esp_peripheral_pins.gpio11;
        let miso = esp_peripheral_pins.gpio46;

        // Control pins
        let cs_pin = PinDriver::output(esp_peripheral_pins.gpio10)?;
        let dc_pin = PinDriver::output(esp_peripheral_pins.gpio9)?;
        let reset_pin = PinDriver::output(esp_peripheral_pins.gpio13)?;
        let busy_pin = PinDriver::input(esp_peripheral_pins.gpio14)?;

        Ok(Epd::new_explicit(sck, mosi, miso, cs_pin, dc_pin, reset_pin, busy_pin, spi))
}

fn display_todos(epd: &mut Epd, tasks: &[String]) -> Result<()> {
    let now = Local::now();
    info!("Date: {}\n", now.format("%A, %B %d, %Y"));

    // Create framebuffer
    let mut fb = FrameBuffer::new(epd.width(), epd.height());
    info!("Created buffer of size: {} bytes", fb.buffer().len());

    const BLACK: u8 = 0x00;
    const WHITE: u8 = 0x01;

    info!("Displaying tasks on the screen...");
    fb.fill(WHITE);
    let headline = format!("Tasks for {}", now.format("%B %d, %Y"));
    let mut y = 0;
    fb.text(&headline, 30, y, BLACK);
    for task in tasks {
        y += 10;
        fb.text(task, 10, y, BLACK);
    }

    epd.display(fb.buffer());

    Ok(())
}
fn main() -> Result<()> {
    EspLogger::initialize_default();

    info!("Daily Prioritizer");
    info!("{}", "=".repeat(50));

    let now = Local::now();
    info!("Date: {}\n", now.format("%A, %B %d, %Y"));

    esp_idf_sys::link_patches();

    let system_peripherals = esp_idf_hal::peripherals::Peripherals::take().unwrap();
    let sys_loop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    let session_wifi = wifi_up(system_peripherals.modem, sys_loop, nvs)?;
    info!(
        "DHCP server assigned IP address: {:?}",
        session_wifi.wifi().sta_netif().get_ip_info()?
    );

    sync_time()?;
    
    let _ = fetch_calendar_events()?;
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

    let mut epd = set_up_display(system_peripherals.pins, system_peripherals.spi2)?;

    info!("Resetting the screen...");
    epd.init();
    epd.clear();

    display_todos(&mut epd, &tasks)?;

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

    epd.sleep();

    Ok(())
}
