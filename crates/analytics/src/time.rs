use chrono::{NaiveDateTime, Utc};

pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub fn now() -> NaiveDateTime {
    Utc::now().naive_utc()
}

pub fn format(t: &NaiveDateTime) -> String {
    t.format(TIMESTAMP_FORMAT).to_string()
}
