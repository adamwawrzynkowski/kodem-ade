use chrono::{Datelike, Local, TimeZone};

/// Format a unix timestamp like the history list: "Merged 16 August at 02:27".
pub fn history_date(unix: i64, merged: bool) -> String {
    let Some(time) = Local.timestamp_opt(unix, 0).single() else {
        return String::new();
    };
    let now = Local::now();
    let prefix = if merged { "Merged " } else { "" };
    if time.date_naive() == now.date_naive() {
        format!("{prefix}Today at {}", time.format("%H:%M"))
    } else if now.date_naive().pred_opt() == Some(time.date_naive()) {
        format!("{prefix}Yesterday at {}", time.format("%H:%M"))
    } else if time.year() == now.year() {
        format!("{prefix}{}", time.format("%-d %B at %H:%M"))
    } else {
        format!("{prefix}{}", time.format("%-d %B %Y at %H:%M"))
    }
}

/// Relative age like "2h ago".
pub fn relative_date(unix: i64) -> String {
    let Some(time) = Local.timestamp_opt(unix, 0).single() else {
        return String::new();
    };
    let delta = Local::now().signed_duration_since(time);
    let minutes = delta.num_minutes();
    if minutes < 1 {
        "just now".into()
    } else if minutes < 60 {
        format!("{minutes}m ago")
    } else if minutes < 60 * 24 {
        format!("{}h ago", minutes / 60)
    } else if minutes < 60 * 24 * 30 {
        format!("{}d ago", minutes / (60 * 24))
    } else {
        time.format("%-d %b %Y").to_string()
    }
}
