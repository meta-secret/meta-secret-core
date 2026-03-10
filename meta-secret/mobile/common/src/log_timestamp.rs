use time::macros::format_description;
use time::OffsetDateTime;

const LOG_TIMESTAMP_FMT: &[time::format_description::FormatItem<'_>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

pub fn log_timestamp_utc() -> String {
    OffsetDateTime::now_utc()
        .format(LOG_TIMESTAMP_FMT)
        .unwrap_or_else(|_| "?".into())
}
