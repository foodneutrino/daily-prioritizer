use regex::Regex;
use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub struct ScheduleItem {
    pub time_start: String,
    pub time_end: String,
    pub task: String,
}

impl ScheduleItem {
    pub fn new(time_start: &str, time_end: &str, task: &str) -> Self {
        Self {
            time_start: time_start.to_string(),
            time_end: time_end.to_string(),
            task: task.to_string(),
        }
    }
}

pub fn extract_schedule(input: &str) -> Result<Vec<ScheduleItem>> {
    let pattern = Regex::new(r"--__-- (\d{1,2}:\d{2}) - (\d{1,2}:\d{2}): (.+)").unwrap();
    let mut schedule = Vec::new();

    for line in input.lines() {
        if let Some(captures) = pattern.captures(line) {
            schedule.push(ScheduleItem::new(
                &captures[1],
                &captures[2],
                &captures[3],
            ));
        }
    }

    Ok(schedule)
}