use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_string() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    millis.to_string()
}

/// Normalizes persisted millisecond epochs and ACP ISO-8601 UTC timestamps for ordering.
pub(crate) fn activity_millis(value: &str) -> Option<i128> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return trimmed.parse::<i128>().ok();
    }
    parse_iso_utc_millis(trimmed)
}

fn parse_iso_utc_millis(value: &str) -> Option<i128> {
    let (date, time_with_offset) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let (time, offset_minutes) = parse_time_and_offset(time_with_offset)?;
    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    let second_and_millis = time_parts.next()?;
    if time_parts.next().is_some() {
        return None;
    }
    let (second, millis) = match second_and_millis.split_once('.') {
        Some((second, fraction)) => {
            if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
                return None;
            }
            let millis = fraction.chars().take(3).collect::<String>();
            let millis = format!("{millis:0<3}").parse::<u32>().ok()?;
            (second.parse::<u32>().ok()?, millis)
        }
        None => (second_and_millis.parse::<u32>().ok()?, 0),
    };
    if !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let local_millis = (((days * 24 + hour as i128) * 60 + minute as i128) * 60 + second as i128)
        * 1000
        + millis as i128;
    Some(local_millis - i128::from(offset_minutes) * 60 * 1000)
}

fn parse_time_and_offset(value: &str) -> Option<(&str, i32)> {
    if let Some(time) = value.strip_suffix('Z') {
        return Some((time, 0));
    }

    let Some(offset_start) = value.rfind(['+', '-']) else {
        // Persisted ACP data predating strict RFC 3339 validation omitted the UTC suffix.
        return Some((value, 0));
    };
    let (time, offset) = value.split_at(offset_start);
    let sign = match offset.as_bytes().first()? {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let (hours, minutes) = offset.get(1..)?.split_once(':')?;
    if hours.len() != 2 || minutes.len() != 2 {
        return None;
    }
    let hours = hours.parse::<i32>().ok()?;
    let minutes = minutes.parse::<i32>().ok()?;
    if hours > 23 || minutes > 59 {
        return None;
    }
    Some((time, sign * (hours * 60 + minutes)))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 31,
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i128 {
    let year = year as i128 - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i128;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i128 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[path = "time_tests.rs"]
mod tests;
