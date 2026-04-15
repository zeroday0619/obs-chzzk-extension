use std::env;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

static LOG_LEVEL: OnceLock<LogLevel> = OnceLock::new();
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
