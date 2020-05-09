use std::time::SystemTime;

use chrono::{DateTime, Utc};

pub fn get_time_now() -> DateTime<Utc> {
    SystemTime::now().into()
}

pub fn parse_time_imf(time: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_str(time, "%a, %d %b %Y %T GMT").ok().map(|t| t.with_timezone(&Utc))
}

pub fn format_time_imf(time: DateTime<Utc>) -> String {
    time.format("%a, %d %b %Y %T GMT").to_string()
}
