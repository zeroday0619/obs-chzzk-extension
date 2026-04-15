use std::sync::{Mutex, OnceLock};

use crate::logging::debug;

use super::model::PluginSettings;

static SETTINGS: OnceLock<Mutex<PluginSettings>> = OnceLock::new();

fn settings_store() -> &'static Mutex<PluginSettings> {
    SETTINGS.get_or_init(|| Mutex::new(PluginSettings::default()))
}

pub(crate) fn apply_runtime_settings(next: PluginSettings) {
    debug("applying runtime settings snapshot");
    let mut current = settings_store().lock().unwrap_or_else(|error| error.into_inner());
    *current = next;
}

pub(crate) fn current_settings() -> PluginSettings {
    settings_store()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}
