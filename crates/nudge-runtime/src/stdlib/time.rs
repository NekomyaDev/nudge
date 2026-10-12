//! std/time - Time and duration functions
//!
//! Provides time.now, time.sleep, and time.format.

use crate::vm::Value;
use std::collections::HashMap;
use std::fmt::Write;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MONTH_ABBR: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const MONTH_FULL: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAY_ABBR: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const WEEKDAY_FULL: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Returns (year, month, day) from days since 1970-01-01 (Unix epoch).
/// Based on Howard Hinnant's algorithm: https://howardhinnant.github.io/date_algorithms.html
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = y + if m <= 2 { 1 } else { 0 };
    (year, m as u32, d as u32)
}

/// Formats a UTC timestamp according to strftime format specifiers.
pub fn format_utc(secs: i64, nsecs: u32, fmt: &str) -> String {
    let days = secs.div_euclid(86400);
    let rem_secs = secs.rem_euclid(86400);
    let hour = (rem_secs / 3600) as u32;
    let minute = ((rem_secs % 3600) / 60) as u32;
    let second = (rem_secs % 60) as u32;

    let (year, month, day) = civil_from_days(days);
    let wday = (days + 4).rem_euclid(7) as usize;
    let m_idx = (month - 1) as usize;
    let hour12 = if hour == 0 {
        12
    } else if hour > 12 {
        hour - 12
    } else {
        hour
    };
    let ampm = if hour < 12 { "AM" } else { "PM" };

    let mut out = String::with_capacity(fmt.len() + 16);
    let mut chars = fmt.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '%' {
            if let Some(spec) = chars.next() {
                match spec {
                    'Y' => {
                        if year >= 0 {
                            let _ = write!(out, "{:04}", year);
                        } else {
                            let _ = write!(out, "{:05}", year);
                        }
                    }
                    'y' => {
                        let _ = write!(out, "{:02}", year.rem_euclid(100));
                    }
                    'm' => {
                        let _ = write!(out, "{:02}", month);
                    }
                    'd' => {
                        let _ = write!(out, "{:02}", day);
                    }
                    'H' => {
                        let _ = write!(out, "{:02}", hour);
                    }
                    'M' => {
                        let _ = write!(out, "{:02}", minute);
                    }
                    'S' => {
                        let _ = write!(out, "{:02}", second);
                    }
                    's' => {
                        let _ = write!(out, "{}", secs);
                    }
                    'f' => {
                        let _ = write!(out, "{:09}", nsecs);
                    }
                    'b' => out.push_str(MONTH_ABBR[m_idx]),
                    'B' => out.push_str(MONTH_FULL[m_idx]),
                    'a' => out.push_str(WEEKDAY_ABBR[wday]),
                    'A' => out.push_str(WEEKDAY_FULL[wday]),
                    'p' => out.push_str(ampm),
                    'I' => {
                        let _ = write!(out, "{:02}", hour12);
                    }
                    'Z' => out.push_str("UTC"),
                    '%' => out.push('%'),
                    other => {
                        out.push('%');
                        out.push(other);
                    }
                }
            } else {
                out.push('%');
            }
        } else {
            out.push(ch);
        }
    }

    out
}

const METHODS: [&str; 3] = ["time.now", "time.sleep", "time.format"];

pub fn register() -> HashMap<String, Value> {
    let mut functions = HashMap::new();
    for name in METHODS {
        functions.insert(name.to_string(), Value::Native(name));
    }
    functions
}

pub fn execute(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "time.now" => {
            if !args.is_empty() {
                return Err("time.now takes 0 arguments".to_string());
            }
            let duration = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| format!("time.now error: {}", e))?;
            let secs = duration.as_secs_f64();
            Ok(Value::Float(secs))
        }

        "time.sleep" => {
            if args.len() != 1 {
                return Err("time.sleep requires 1 argument (ms)".to_string());
            }
            let ms = match &args[0] {
                Value::Int(v) => *v as f64,
                Value::Float(v) => *v,
                _ => return Err("time.sleep: argument must be a number (int or float)".to_string()),
            };
            if ms < 0.0 {
                return Err("time.sleep: duration cannot be negative".to_string());
            }
            let duration = Duration::from_secs_f64(ms / 1000.0);
            thread::sleep(duration);
            Ok(Value::None)
        }

        "time.format" => {
            if args.len() != 2 {
                return Err(
                    "time.format requires 2 arguments (timestamp, format string)".to_string(),
                );
            }
            let ts = match &args[0] {
                Value::Float(v) => *v,
                Value::Int(v) => *v as f64,
                _ => {
                    return Err(
                        "time.format: first argument (timestamp) must be a number".to_string()
                    )
                }
            };
            let fmt = match &args[1] {
                Value::String(s) => s.clone(),
                _ => {
                    return Err("time.format: second argument (format) must be a string".to_string())
                }
            };

            if !ts.is_finite() || ts < -8_000_000_000_000.0 || ts > 8_000_000_000_000.0 {
                return Err(format!("time.format: invalid timestamp {}", ts));
            }

            let secs = ts.floor() as i64;
            let fract = (ts - ts.floor()).clamp(0.0, 0.999999999999);
            let nsecs = (fract * 1_000_000_000.0) as u32;

            let formatted = format_utc(secs, nsecs, &fmt);
            Ok(Value::String(formatted))
        }

        _ => Err(format!("Unknown time function: {}", name)),
    }
}
