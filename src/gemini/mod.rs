mod gemini;
mod response_parse;

pub use gemini::{GeminiClient, DEFAULT_PROMPT};
pub use response_parse::{extract_schedule, ScheduleItem};