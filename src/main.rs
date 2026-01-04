//! Daily Prioritizer
//!
//! Combines Google Calendar free time analysis with Notion task management
//! to help prioritize your daily work.

mod calendar;
mod notion;
mod waveshare;

use anyhow::Result;
use chrono::Local;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    println!("Daily Prioritizer");
    println!("{}", "=".repeat(50));

    let now = Local::now();
    println!("Date: {}\n", now.format("%A, %B %d, %Y"));

    // Fetch calendar free time
    println!("--- Google Calendar ---");
    match calendar::get_credentials() {
        Ok(access_token) => {
            match calendar::get_todays_events(&access_token) {
                Ok(events) => {
                    let (busy_periods, free_slots) = calendar::calculate_free_time(&events);

                    println!(
                        "Working hours: {}:00 - {}:00",
                        calendar::WORK_START_HOUR, calendar::WORK_END_HOUR
                    );

                    if !busy_periods.is_empty() {
                        println!("\nScheduled events:");
                        for period in &busy_periods {
                            let duration = calendar::format_duration(period.end - period.start);
                            println!(
                                "  {} - {} ({}): {}",
                                period.start.format("%H:%M"),
                                period.end.format("%H:%M"),
                                duration,
                                period.title
                            );
                        }
                    } else {
                        println!("\nNo events scheduled today!");
                    }

                    println!("\nFree time slots:");
                    if !free_slots.is_empty() {
                        let mut total_free = chrono::Duration::zero();
                        for slot in &free_slots {
                            let duration = slot.end - slot.start;
                            total_free = total_free + duration;
                            println!(
                                "  {} - {} ({})",
                                slot.start.format("%H:%M"),
                                slot.end.format("%H:%M"),
                                calendar::format_duration(duration)
                            );
                        }
                        println!("\nTotal free time: {}", calendar::format_duration(total_free));
                    } else {
                        println!("  No free time available during working hours!");
                    }
                }
                Err(e) => println!("Failed to fetch calendar events: {}", e),
            }
        }
        Err(e) => println!("Failed to get calendar credentials: {}", e),
    }

    println!("\n{}", "-".repeat(50));

    // Fetch Notion tasks
    println!("\n--- Notion Tasks ---");
    match env::var("NOTION_API_KEY") {
        Ok(api_key) => {
            let notion_client = notion::NotionClient::new(&api_key);

            match notion_client.query_datasource(notion::SOURCE_ID, None).await {
                Ok(datasource_response) => {
                    let tasks = notion::extract_active_tasks(&datasource_response);

                    if !tasks.is_empty() {
                        println!("Active tasks (To Do / Doing):");
                        for task in &tasks {
                            println!("  - {}", task);
                        }
                        println!("\nTotal tasks: {}", tasks.len());
                    } else {
                        println!("No active tasks found!");
                    }
                }
                Err(e) => println!("Failed to query Notion datasource: {}", e),
            }
        }
        Err(_) => println!("NOTION_API_KEY not set. Skipping Notion tasks."),
    }

    println!("\n{}", "=".repeat(50));
    println!("Daily planning complete!");

    Ok(())
}
