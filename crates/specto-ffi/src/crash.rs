//! A record of panics, for bug reports.
//!
//! A panic on Android leaves little behind: the message goes to a standard error stream nobody
//! reads, and most panics don't even end the app (a tile that fails to render is skipped, a failed
//! call surfaces in Kotlin as an exception). So each one is also appended to a log file of the
//! app's own, which the app shows in its settings and the user can share. Nothing is sent anywhere.
//!
//! The file is shared with the Kotlin side (`CrashLog.kt`), which writes uncaught exceptions to it
//! in the same form and keeps it short: entries start with a line of `--- `, the time in UTC, and
//! what happened.

use std::fs::OpenOptions;
use std::io::Write;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::sync::{Mutex, Once};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where panics are recorded, which is nowhere until [`log_panics_to`] says.
static LOG: Mutex<Option<PathBuf>> = Mutex::new(None);
static HOOK: Once = Once::new();

/// From now on, appends every panic in this library to the file at `path` (created if missing):
/// when, on which thread, the message and where in the code.
///
/// Whatever handled panics before still does. Call it once, when the app starts. Calling it again
/// changes the file.
#[uniffi::export]
pub fn log_panics_to(path: String) {
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) = Some(PathBuf::from(path));
    HOOK.call_once(|| {
        let before = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            record(info);
            before(info);
        }));
    });
}

/// Appends `info` to the log.
///
/// Never panics itself: a failure to write is ignored, there being nowhere left to report it.
fn record(info: &PanicHookInfo<'_>) {
    let Some(path) = LOG.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
        return;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let thread = std::thread::current();
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("(no message)");
    let mut entry = format!(
        "--- {} Rust panic\nthread '{}': {message}\n",
        utc(now),
        thread.name().unwrap_or("unnamed"),
    );
    if let Some(at) = info.location() {
        entry.push_str(&format!("at {}:{}:{}\n", at.file(), at.line(), at.column()));
    }
    entry.push('\n');
    if let Ok(mut file) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = file.write_all(entry.as_bytes());
    }
}

/// `seconds` since 1970 as a date and time in UTC, like "2026-11-15T18:42:10Z".
fn utc(seconds: u64) -> String {
    let d = specto_source::Date::from_unix(seconds);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        d.year, d.month, d.day, d.hour, d.minute, d.second
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_become_dates() {
        assert_eq!(utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(utc(951_782_399), "2000-02-28T23:59:59Z");
        assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(utc(951_868_800), "2000-03-01T00:00:00Z");
        assert_eq!(utc(1_794_768_130), "2026-11-15T18:42:10Z");
        assert_eq!(utc(4_102_444_799), "2099-12-31T23:59:59Z");
    }

    #[test]
    fn panics_are_recorded() {
        let path =
            std::env::temp_dir().join(format!("specto-ffi-crash-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);
        log_panics_to(path.to_string_lossy().into_owned());
        let caught = std::thread::Builder::new()
            .name("doomed".into())
            .spawn(|| panic!("number {} went wrong", 7))
            .unwrap()
            .join();
        assert!(caught.is_err());
        let log = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        // Other tests may panic on purpose meanwhile, but this one is in there.
        let entry = log
            .split("--- ")
            .find(|e| e.contains("number 7 went wrong"))
            .unwrap_or_else(|| panic!("not recorded: {log}"));
        let lines: Vec<&str> = entry.lines().collect();
        assert!(lines[0].ends_with("Z Rust panic"), "{entry}");
        assert_eq!(lines[1], "thread 'doomed': number 7 went wrong");
        assert!(
            lines[2].starts_with("at ") && lines[2].contains("crash.rs:"),
            "{entry}"
        );
    }
}
