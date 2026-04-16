use std::env;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;

use crate::notification_popup::ffi::show_notification_popup;

#[derive(Clone)]
struct PopupState {
    level: LogLevel,
    timestamp_ms: u64,
    message: String,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
#[repr(i32)]
pub(crate) enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

static LOG_LEVEL: OnceLock<LogLevel> = OnceLock::new();
static POPUP_STATE: OnceLock<Mutex<Option<PopupState>>> = OnceLock::new();
const LOG_TAG: &str = "obs-chzzk-extension";
const POPUP_DEDUP_WINDOW_MS: u64 = 2_000;

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
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
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

fn popup_title(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "obs-chzzk-extension Error",
        _ => "obs-chzzk-extension",
    }
}

fn should_show_popup(level: LogLevel, message: &str) -> bool {
    if level != LogLevel::Error || message.trim().is_empty() {
        return false;
    }

    let popup_state = POPUP_STATE.get_or_init(|| Mutex::new(None));
    let now = timestamp_ms();
    let mut guard = popup_state.lock().unwrap_or_else(|error| error.into_inner());

    if let Some(previous) = guard.as_ref() {
        let duplicate = previous.level == level
            && previous.message == message
            && now.saturating_sub(previous.timestamp_ms) <= POPUP_DEDUP_WINDOW_MS;
        if duplicate {
            return false;
        }
    }

    *guard = Some(PopupState {
        level,
        timestamp_ms: now,
        message: message.to_string(),
    });
    true
}

fn notify_popup(level: LogLevel, message: &str) {
    if !should_show_popup(level, message) {
        return;
    }

    let title: QString = popup_title(level).into();
    let body: QString = message.into();
    show_notification_popup(level as i32, &title, &body);
}

fn render_log_line(level: LogLevel, message: &str) -> String {
    format!(
        "[{}][{}][{}][pid:{}][tid:{:?}] {}",
        timestamp_ms(),
        level_label(level),
        LOG_TAG,
        std::process::id(),
        std::thread::current().id(),
        message
    )
}

pub(crate) fn log(level: LogLevel, message: impl AsRef<str>) {
    let message = message.as_ref();
    notify_popup(level, message);

    if level < configured_level() {
        return;
    }

    let rendered = render_log_line(level, message);

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
