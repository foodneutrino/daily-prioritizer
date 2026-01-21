//! Daily Prioritizer
//!
//! Combines Google Calendar free time analysis with Notion task management
//! to help prioritize your daily work.

mod calendar;
mod notion;
mod waveshare;
mod wifi;
mod gemini;

use chrono::Local;
use esp_idf_hal::{gpio::Pins, spi::SPI2};
use log::info;
use anyhow::Result;
use minijinja::{Environment, context};

use esp_idf_sys as _;
use esp_idf_svc::log::EspLogger;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sntp::EspSntp;
use esp_idf_sys::tzset;
use std::time::Duration;

use wifi::wifi_up;
use waveshare::{Epd, FrameBuffer};
use gemini::{ScheduleItem, PromptTemplate};
use crate::{calendar::FreeSlot, gemini::DEFAULT_PROMPT};

// Display Color Values
const BLACK: u8 = 0x00;
const WHITE: u8 = 0x01;

fn sync_time() -> anyhow::Result<()> {
    log::info!("Initializing SNTP...");

    // Set timezone (do this before or after sync)
    std::env::set_var("TZ", "EST5EDT,M3.2.0,M11.1.0");
    unsafe { tzset(); }

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
fn fetch_calendar_events() -> Result<Vec<FreeSlot>> {
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
    Ok(free_slots)
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

fn ask_gemini(prompt: &str) -> Result<Vec<ScheduleItem>> {
    info!("--- Gemini AI ---");

    let api_key = match option_env!("GEMINI_API_KEY") {
        Some(key) => key,
        None => {
            info!("GEMINI_API_KEY not set. Skipping Gemini AI.");
            return Err(anyhow::anyhow!("No Gemini API Key"));
        }
    };

    let mut gemini_client = gemini::GeminiClient::new(api_key);

    let response = gemini_client.generate_content(prompt)?;

    info!("Gemini Plan: {}", response);
    gemini::extract_schedule(&response)
}

fn set_up_display(esp_peripheral_pins: Pins, spi: SPI2) -> Result<Epd<'static>> {    
        Ok(Epd::new_explicit(
            esp_peripheral_pins.gpio12,   // any pin for sck
            esp_peripheral_pins.gpio11,   // any pin for mosi
            esp_peripheral_pins.gpio46,   // any pin for miso
            esp_peripheral_pins.gpio10,   // any pin for cs
            esp_peripheral_pins.gpio9,   // any pin for dc
            esp_peripheral_pins.gpio13,  // any pin for reset
            esp_peripheral_pins.gpio14,  // any pin for busy
            spi,
        ))
}

fn display_daily_plan(fb: &mut FrameBuffer, todays_tasks: &[ScheduleItem], start_row: u32) -> Result<u32> {
    info!("Displaying today's prioritized tasks on the screen");

    let mut y = start_row;
    for item in todays_tasks.iter() {
        let line = format!("{} - {}: {}", item.time_start, item.time_end, item.task);
        let mut chars_to_print = line.len();
        let mut slice_start = 0;
        while chars_to_print > 0 {
            let slice_end = if chars_to_print > 40 {40} else {chars_to_print};
            let line_slice = &line[slice_start..slice_start + slice_end];
            slice_start += slice_end;
            y += 10;
            fb.text(line_slice,4,y,BLACK);
            chars_to_print -= slice_end;
        }
    }
    Ok(y)
}

fn create_todos_display(fb: &mut FrameBuffer,tasks: &[String], start_row: u32) -> Result<u32> {
    let now = Local::now();
    info!("Date: {}\n", now.format("%A, %B %d, %Y"));

    info!("Displaying tasks on the screen...");
    let headline = format!("Tasks for {}", now.format("%B %d, %Y"));
    let mut y = start_row;
    fb.text(&headline, 30, y, BLACK);
    for task in tasks {
        y += 10;
        fb.text(task, 10, y, BLACK);
    }

    Ok(y)
}

fn create_free_time_display(fb: &mut FrameBuffer, free_slots: &[FreeSlot], start_row: u32) -> Result<u32> {
    info!("Displaying free time slots on the screen...");
    let headline = format!("Today's Free Time Slots");
    let mut y = start_row;
    fb.text(&headline, 30, y, BLACK);
    for slot in free_slots {
        y += 10;
        fb.text(&format!("{} - {}", slot.start.format("%H:%M"), slot.end.format("%H:%M")), 10, y, BLACK);
    }

    Ok(y)
}

fn main() -> Result<()> {
    EspLogger::initialize_default();

    info!("Daily Prioritizer");
    info!("{}", "=".repeat(50));

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

    let free_slots = fetch_calendar_events()?;
    info!("\n{}", "-".repeat(50));

    let tasks = fetch_notion_tasks()?;
    info!("Active tasks (To Do / Doing):");
    for task in &tasks {
        info!("  - {}", task);
    }
    info!("\nTotal tasks: {}", tasks.len());

    info!("\n{}", "=".repeat(50));

    let prompt_data = PromptTemplate {
        timeslots: free_slots.iter().map(|slot| format!("\t[Time: {} - {}\n]", slot.start.format("%H:%M"), slot.end.format("%H:%M"))).collect(),
        tasks: tasks.iter().map(|task| format!("\t[Task: {}\n]", task)).collect(),
    };
    let rendered = Environment::new().render_str(DEFAULT_PROMPT, context! {
        timeslots => prompt_data.timeslots,
        tasks => prompt_data.tasks
    })?;
    let todays_tasks = ask_gemini(&rendered)?;
    info!("\n Gemini says: \n {:?}", todays_tasks);

    info!("Daily planning complete!");

    let mut epd = set_up_display(system_peripherals.pins, system_peripherals.spi2)?;

    info!("Resetting the screen...");
    epd.init();
    epd.clear();

    // Create framebuffer
    let mut fb = FrameBuffer::new(epd.width(), epd.height());
    fb.fill(WHITE);
    info!("Created buffer of size: {} bytes", fb.buffer().len());

    let end_row = display_daily_plan(&mut fb, &todays_tasks, 0)?;
    fb.hline(0, end_row + 20, 200, BLACK);

    info!("Writing FrameBuffer to display");
    epd.display(fb.buffer());

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
