use chrono::{NaiveDateTime, Utc};

pub const TIMESTAMP_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

pub fn now() -> NaiveDateTime {
    let now = Utc::now();
    NaiveDateTime::from_timestamp_opt(now.timestamp(), now.timestamp_subsec_nanos())
        .expect("invalid timestamp")
}

pub fn format(t: &NaiveDateTime) -> String {
    t.format(TIMESTAMP_FORMAT).to_string()
}
