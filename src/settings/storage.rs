use std::env;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::logging::{debug, error as log_error, warn};

use super::constants::SETTINGS_FILE_NAME;
use super::model::{sync_auth_status, PluginSettings};

fn settings_config_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = env::var_os("APPDATA") {
            return PathBuf::from(appdata)
                .join("obs-studio")
                .join("plugin_config");
        }
        if let Some(userprofile) = env::var_os("USERPROFILE") {
            return PathBuf::from(userprofile)
                .join("AppData")
                .join("Roaming")
                .join("obs-studio")
                .join("plugin_config");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("obs-studio")
                .join("plugin_config");
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(config_home) = env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home)
                .join("obs-studio")
                .join("plugin_config");
        }
        if let Some(home) = env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("obs-studio")
                .join("plugin_config");
        }
    }

    env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("obs-studio")
        .join("plugin_config")
}

fn settings_file_path() -> PathBuf {
    settings_config_dir().join(SETTINGS_FILE_NAME)
}

fn file_value(root: &Value, key: &str, fallback: &str) -> String {
    root.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
        .unwrap_or_else(|| fallback.to_string())
}

fn file_bool_value(root: &Value, key: &str, fallback: bool) -> bool {
    root.get(key)
        .and_then(|value| {
            value
                .as_bool()
                .or_else(|| value.as_i64().map(|num| num != 0))
                .or_else(|| {
                    value
                        .as_str()
                        .map(|text| text.eq_ignore_ascii_case("true") || text == "1")
                })
        })
        .unwrap_or(fallback)
}

fn load_settings_from_file() -> Option<PluginSettings> {
    let path = settings_file_path();
    let mut content = String::new();
    let mut file = fs::File::open(&path).ok()?;
    file.read_to_string(&mut content).ok()?;

    let root = serde_json::from_str::<Value>(&content).ok()?;
    let defaults = PluginSettings::default();
    let mut settings = PluginSettings {
        chzzk_client_id: file_value(&root, "chzzk_client_id", ""),
        chzzk_client_secret: file_value(&root, "chzzk_client_secret", ""),
        chzzk_api_base_url: file_value(&root, "chzzk_api_base_url", &defaults.chzzk_api_base_url),
        discord_application_id: file_value(
            &root,
            "discord_application_id",
            &defaults.discord_application_id,
        ),
        discord_presence_enabled: file_bool_value(
            &root,
            "discord_presence_enabled",
            defaults.discord_presence_enabled,
        ),
        discord_activity_name: file_value(
            &root,
            "discord_activity_name",
            &defaults.discord_activity_name,
        ),
        chzzk_authorization_token: file_value(&root, "chzzk_authorization_token", ""),
        chzzk_auth_status: file_value(&root, "chzzk_auth_status", ""),
    };

    sync_auth_status(&mut settings);
    Some(settings)
}

pub(crate) fn load_profile_settings() -> PluginSettings {
    debug("loading settings from plugin_config file");
    let loaded = load_settings_from_file().unwrap_or_default();

    debug(format!(
        "loaded profile settings: has_client_id={}, has_client_secret={}, has_discord_app_id={}, presence_enabled={}, api_base={}",
        !loaded.chzzk_client_id.is_empty(),
        !loaded.chzzk_client_secret.is_empty(),
        !loaded.discord_application_id.is_empty(),
        loaded.discord_presence_enabled,
        loaded.chzzk_api_base_url
    ));

    loaded
}

pub(crate) fn persist_settings_to_file(settings: &PluginSettings) {
    let path = settings_file_path();
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            log_error(format!(
                "obs-chzzk-extension: failed to create settings dir: {}",
                error
            ));
            return;
        }
    }

    let mut root = serde_json::Map::new();
    root.insert(
        "chzzk_client_id".to_string(),
        Value::String(settings.chzzk_client_id.clone()),
    );
    root.insert(
        "chzzk_client_secret".to_string(),
        Value::String(settings.chzzk_client_secret.clone()),
    );
    root.insert(
        "chzzk_api_base_url".to_string(),
        Value::String(settings.chzzk_api_base_url.clone()),
    );
    root.insert(
        "discord_application_id".to_string(),
        Value::String(settings.discord_application_id.clone()),
    );
    root.insert(
        "discord_presence_enabled".to_string(),
        Value::Bool(settings.discord_presence_enabled),
    );
    root.insert(
        "discord_activity_name".to_string(),
        Value::String(settings.discord_activity_name.clone()),
    );
    root.insert(
        "chzzk_authorization_token".to_string(),
        Value::String(settings.chzzk_authorization_token.clone()),
    );
    root.insert(
        "chzzk_auth_status".to_string(),
        Value::String(settings.chzzk_auth_status.clone()),
    );

    let rendered = match serde_json::to_string_pretty(&Value::Object(root)) {
        Ok(text) => text,
        Err(error) => {
            log_error(format!(
                "obs-chzzk-extension: failed to encode settings: {}",
                error
            ));
            return;
        }
    };

    let temp_path = path.with_extension("json.tmp");
    match create_private_file(&temp_path).and_then(|mut file| file.write_all(rendered.as_bytes())) {
        Ok(()) => {
            if let Err(error) = fs::rename(&temp_path, &path) {
                log_error(format!(
                    "obs-chzzk-extension: failed to move settings file: {}",
                    error
                ));
                let _ = fs::remove_file(&temp_path);
            } else {
                #[cfg(unix)]
                enforce_private_permissions(&path);
                debug(format!("settings saved to {}", path.display()));
            }
        }
        Err(error) => {
            log_error(format!(
                "obs-chzzk-extension: failed to write settings file: {}",
                error
            ));
            let _ = fs::remove_file(&temp_path);
        }
    }
}

fn create_private_file(path: &Path) -> std::io::Result<fs::File> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
    }

    #[cfg(not(unix))]
    {
        OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
    }
}

#[cfg(unix)]
fn enforce_private_permissions(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        if let Err(error) = fs::set_permissions(path, perms) {
            warn(format!(
                "obs-chzzk-extension: failed to set private permissions on {}: {}",
                path.display(),
                error
            ));
        }
    }
}
