use std::env;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

static LOG_LEVEL: OnceLock<LogLevel> = OnceLock::new();
static LOG_FILE: OnceLock<Option<Mutex<File>>> = OnceLock::new();
const LOG_TAG: &str = "obs-chzzk-extension";

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

fn level_from_env() -> LogLevel {
    let value = env::var("OBS_CHZZK_EXTENSION_LOG_LEVEL").unwrap_or_else(|_| "debug".to_string());
    match value.trim().to_ascii_lowercase().as_str() {
        "debug" => LogLevel::Debug,
        "info" => LogLevel::Info,
        "warn" | "warning" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Debug,
    }
}

fn configured_level() -> LogLevel {
    *LOG_LEVEL.get_or_init(level_from_env)
}

fn log_file_path() -> String {
    env::var("OBS_CHZZK_EXTENSION_LOG_FILE")
        .unwrap_or_else(|_| "/tmp/obs-chzzk-extension.log".to_string())
}

fn open_log_file() -> Option<Mutex<File>> {
    let path = log_file_path();
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(Mutex::new)
}

fn maybe_log_file() -> &'static Option<Mutex<File>> {
    LOG_FILE.get_or_init(open_log_file)
}

fn timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn log(level: LogLevel, message: impl AsRef<str>) {
    if level < configured_level() {
        return;
    }

    let rendered = format!(
        "[{}][{}][{}][pid:{}][tid:{:?}] {}",
        timestamp_ms(),
        level_label(level),
        LOG_TAG,
        std::process::id(),
        std::thread::current().id(),
        message.as_ref()
    );

    eprintln!("{}", rendered);

    if let Some(file_mutex) = maybe_log_file() {
        if let Ok(mut file) = file_mutex.lock() {
            let _ = writeln!(file, "{}", rendered);
            let _ = file.flush();
        }
    }
}

pub(crate) fn debug(message: impl AsRef<str>) {
    log(LogLevel::Debug, message);
}

pub(crate) fn info(message: impl AsRef<str>) {
    log(LogLevel::Info, message);
}

pub(crate) fn warn(message: impl AsRef<str>) {
    log(LogLevel::Warn, message);
}

pub(crate) fn error(message: impl AsRef<str>) {
    log(LogLevel::Error, message);
}
