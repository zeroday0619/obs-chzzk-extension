use std::collections::HashSet;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

use serde_json::Value;

use crate::chzzk::{ChzzkClient, ChzzkLiveSettingUpdate};
use crate::logging::{error as log_error, info, warn};

mod constants;
mod model;
mod oauth;
mod runtime;
mod storage;

use constants::{
    KEY_CHZZK_API_BASE_URL, KEY_CHZZK_AUTHORIZATION_TOKEN, KEY_CHZZK_AUTH_STATUS,
    KEY_CHZZK_CLIENT_ID, KEY_CHZZK_CLIENT_SECRET, KEY_CHZZK_STREAM_KEY_STATUS,
    KEY_DISCORD_ACTIVITY_NAME, KEY_DISCORD_APPLICATION_ID, KEY_DISCORD_PRESENCE_ENABLED,
    LIVE_DOCK_ID, LIVE_DOCK_TITLE, MENU_TITLE, OBS_GROUP_NORMAL, OBS_SOURCE_TYPE_INPUT,
    OBS_TEXT_DEFAULT, OBS_TEXT_PASSWORD, SOURCE_ID, SOURCE_NAME,
};
use model::{stream_key_status_for_value, sync_auth_status};
use oauth::{request_authorization_token, revoke_token};
use runtime::apply_runtime_settings;
use storage::{load_profile_settings, persist_settings_to_file};

pub(crate) use model::PluginSettings;
pub(crate) use runtime::current_settings;

static SETTINGS_SOURCE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

#[repr(C)]
struct ObsSourceInfoPartial {
    id: *const c_char,
    type_: i32,
    output_flags: u32,
    get_name: Option<unsafe extern "C" fn(*mut c_void) -> *const c_char>,
    create: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void>,
    destroy: Option<unsafe extern "C" fn(*mut c_void)>,
    get_width: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    get_height: Option<unsafe extern "C" fn(*mut c_void) -> u32>,
    get_defaults: Option<unsafe extern "C" fn(*mut c_void)>,
    get_properties: Option<unsafe extern "C" fn(*mut c_void) -> *mut c_void>,
    update: Option<unsafe extern "C" fn(*mut c_void, *mut c_void)>,
}

fn trim_value(value: &str) -> String {
    value.trim().to_string()
}

fn c_char_ptr(bytes: &'static [u8]) -> *const c_char {
    bytes.as_ptr().cast()
}

fn c_string(value: &str) -> Option<CString> {
    CString::new(value).ok()
}

unsafe fn ptr_to_string(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }

    CStr::from_ptr(value).to_string_lossy().to_string()
}

unsafe fn obs_data_value(data: *mut c_void, key: &'static [u8]) -> String {
    trim_value(&ptr_to_string(obs_data_get_string(data, c_char_ptr(key))))
}

unsafe fn obs_data_bool(data: *mut c_void, key: &'static [u8]) -> bool {
    obs_data_get_bool(data, c_char_ptr(key))
}

fn key_name(key: &'static [u8]) -> String {
    let key = key.strip_suffix(b"\0").unwrap_or(key);
    String::from_utf8_lossy(key).into_owned()
}

unsafe fn obs_data_set_value(data: *mut c_void, key: &'static [u8], value: &str) {
    let Some(value) = c_string(value) else {
        log_error(format!(
            "obs-chzzk-extension: invalid string value for key {}",
            key_name(key)
        ));
        return;
    };

    obs_data_set_string(data, c_char_ptr(key), value.as_ptr());
}

unsafe fn obs_data_set_default_value(data: *mut c_void, key: &'static [u8], value: &str) {
    if let Some(value) = c_string(value) {
        obs_data_set_default_string(data, c_char_ptr(key), value.as_ptr());
    }
}

fn update_source_text_fields(fields: &[(&'static [u8], &str)]) -> bool {
    let source = SETTINGS_SOURCE.load(Ordering::SeqCst);
    if source.is_null() {
        warn("obs-chzzk-extension: settings source is not initialized");
        return false;
    }

    unsafe {
        let data = obs_source_get_settings(source);
        if data.is_null() {
            warn("obs-chzzk-extension: failed to get source settings for update");
            return false;
        }

        for (key, value) in fields {
            obs_data_set_value(data, *key, value);
        }

        obs_source_update(source, data);
        obs_data_release(data);
    }

    true
}

#[derive(Clone, Default)]
pub(crate) struct LiveDockCategoryEntry {
    pub(crate) category_type: String,
    pub(crate) category_id: String,
    pub(crate) category_name: String,
    pub(crate) poster_image_url: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct LiveDockResponse {
    pub(crate) ok: bool,
    pub(crate) status: String,
    pub(crate) live_title: Option<String>,
    pub(crate) category_type: Option<String>,
    pub(crate) category_id: Option<String>,
    pub(crate) category_name: Option<String>,
    pub(crate) poster_image_url: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
    pub(crate) categories: Option<Vec<LiveDockCategoryEntry>>,
}

impl LiveDockResponse {
    pub(crate) fn success(status: &str) -> Self {
        Self {
            ok: true,
            status: status.to_string(),
            ..Self::default()
        }
    }

    pub(crate) fn error(status: String) -> Self {
        Self {
            ok: false,
            status,
            ..Self::default()
        }
    }
}

fn sanitize_c_text(value: &str) -> String {
    value.replace('\0', " ")
}

fn response_json_ptr(response: LiveDockResponse) -> *mut c_char {
    let mut payload = serde_json::Map::new();
    payload.insert("ok".to_string(), Value::Bool(response.ok));
    payload.insert(
        "status".to_string(),
        Value::String(sanitize_c_text(&response.status)),
    );

    if let Some(live_title) = response.live_title {
        payload.insert(
            "liveTitle".to_string(),
            Value::String(sanitize_c_text(&live_title)),
        );
    }
    if let Some(category_type) = response.category_type {
        payload.insert(
            "categoryType".to_string(),
            Value::String(sanitize_c_text(&category_type)),
        );
    }
    if let Some(category_id) = response.category_id {
        payload.insert(
            "categoryId".to_string(),
            Value::String(sanitize_c_text(&category_id)),
        );
    }
    if let Some(category_name) = response.category_name {
        payload.insert(
            "categoryName".to_string(),
            Value::String(sanitize_c_text(&category_name)),
        );
    }
    if let Some(poster_image_url) = response.poster_image_url {
        payload.insert(
            "posterImageUrl".to_string(),
            Value::String(sanitize_c_text(&poster_image_url)),
        );
    }
    if let Some(tags) = response.tags {
        let values = tags
            .into_iter()
            .map(|tag| Value::String(sanitize_c_text(&tag)))
            .collect::<Vec<_>>();
        payload.insert("tags".to_string(), Value::Array(values));
    }
    if let Some(categories) = response.categories {
        let values = categories
            .into_iter()
            .map(|item| {
                let mut entry = serde_json::Map::new();
                entry.insert(
                    "categoryType".to_string(),
                    Value::String(sanitize_c_text(&item.category_type)),
                );
                entry.insert(
                    "categoryId".to_string(),
                    Value::String(sanitize_c_text(&item.category_id)),
                );
                entry.insert(
                    "categoryName".to_string(),
                    Value::String(sanitize_c_text(&item.category_name)),
                );
                if let Some(url) = item.poster_image_url {
                    entry.insert(
                        "posterImageUrl".to_string(),
                        Value::String(sanitize_c_text(&url)),
                    );
                }
                Value::Object(entry)
            })
            .collect::<Vec<_>>();

        payload.insert("categories".to_string(), Value::Array(values));
    }

    let rendered = Value::Object(payload).to_string();
    let text = CString::new(rendered).unwrap_or_else(|_| {
        CString::new("{\"ok\":false,\"status\":\"response encoding failed\"}")
            .expect("static string must be valid c string")
    });
    text.into_raw()
}

fn require_linked_settings() -> Result<PluginSettings, String> {
    let settings = current_settings();
    if settings.chzzk_authorization_token.trim().is_empty() {
        return Err("CHZZK account is not linked. Run OAuth first.".to_string());
    }
    Ok(settings)
}

fn reload_live_setting_or_status(success_status: &str, action_label: &str) -> LiveDockResponse {
    match load_live_setting_response(success_status) {
        Ok(response) => response,
        Err(error) => LiveDockResponse::success(&format!(
            "{}, but failed to refresh fields: {}",
            action_label, error
        )),
    }
}

fn apply_update_and_reload(
    settings: &PluginSettings,
    update: &ChzzkLiveSettingUpdate,
    success_status: &str,
    action_label: &str,
    error_context: &str,
) -> Result<LiveDockResponse, String> {
    let access_token = settings.chzzk_authorization_token.trim();
    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    client
        .update_live_settings(access_token, update)
        .map_err(|error| format!("{}: {}", error_context, error))?;

    Ok(reload_live_setting_or_status(success_status, action_label))
}

fn response_from_result(response: Result<LiveDockResponse, String>) -> LiveDockResponse {
    match response {
        Ok(response) => response,
        Err(error) => {
            log_error(&error);
            LiveDockResponse::error(error)
        }
    }
}

fn response_json_from_result(response: Result<LiveDockResponse, String>) -> *mut c_char {
    response_json_ptr(response_from_result(response))
}

pub(crate) fn load_live_setting_response(status: &str) -> Result<LiveDockResponse, String> {
    let settings = require_linked_settings()?;
    let access_token = settings.chzzk_authorization_token.trim();

    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    let live = client
        .fetch_live_settings(access_token)
        .map_err(|error| format!("Failed to fetch live setting: {}", error))?;

    let live_title = live.live_title.unwrap_or_default();
    let category_type = live.category_type.unwrap_or_default();
    let category_id = live.category_id.unwrap_or_default();
    let category_name = live.category_name.unwrap_or_default();
    let tags = live.tags.unwrap_or_default();

    let poster_image_url = resolve_current_category_thumbnail(
        &settings,
        &client,
        &category_type,
        &category_id,
        &category_name,
    );

    Ok(LiveDockResponse {
        ok: true,
        status: status.to_string(),
        live_title: Some(live_title),
        category_type: Some(category_type),
        category_id: Some(category_id),
        category_name: Some(category_name),
        poster_image_url,
        tags: Some(tags),
        categories: None,
    })
}

fn resolve_current_category_thumbnail(
    settings: &PluginSettings,
    client: &ChzzkClient,
    category_type: &str,
    category_id: &str,
    category_name: &str,
) -> Option<String> {
    let category_id = category_id.trim();
    if category_id.is_empty() {
        return None;
    }

    let client_id = settings.chzzk_client_id.trim();
    let client_secret = settings.chzzk_client_secret.trim();
    if client_id.is_empty() || client_secret.is_empty() {
        return None;
    }

    let query = if category_name.trim().is_empty() {
        category_id
    } else {
        category_name.trim()
    };

    let category_type = category_type.trim();

    let try_pick = |items: &[crate::chzzk::ChzzkCategory]| {
        items
            .iter()
            .find(|item| {
                item.category_id == category_id
                    && (category_type.is_empty() || item.category_type == category_type)
            })
            .or_else(|| items.iter().find(|item| item.category_id == category_id))
            .and_then(|item| item.poster_image_url.clone())
    };

    match client.search_categories(client_id, client_secret, query, Some(20)) {
        Ok(items) => {
            if let Some(url) = try_pick(&items) {
                return Some(url);
            }
        }
        Err(error) => {
            warn(format!(
                "Failed to resolve category thumbnail on load (query='{}'): {}",
                query, error
            ));
            return None;
        }
    }

    if query != category_id {
        match client.search_categories(client_id, client_secret, category_id, Some(20)) {
            Ok(items) => {
                if let Some(url) = try_pick(&items) {
                    return Some(url);
                }
            }
            Err(error) => {
                warn(format!(
                    "Failed fallback category thumbnail lookup (category_id='{}'): {}",
                    category_id, error
                ));
            }
        }
    }

    None
}

pub(crate) fn search_category_response(query: &str) -> Result<LiveDockResponse, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("Category Search Query is empty".to_string());
    }

    let settings = current_settings();
    if settings.chzzk_client_id.trim().is_empty() || settings.chzzk_client_secret.trim().is_empty()
    {
        return Err("CHZZK Client ID/Secret are required for category search".to_string());
    }

    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    let categories = client
        .search_categories(
            &settings.chzzk_client_id,
            &settings.chzzk_client_secret,
            query,
            Some(20),
        )
        .map_err(|error| format!("Failed to search category: {}", error))?;

    let entries = categories
        .into_iter()
        .map(|category| LiveDockCategoryEntry {
            category_type: category.category_type,
            category_id: category.category_id,
            category_name: category.category_value,
            poster_image_url: category.poster_image_url,
        })
        .collect::<Vec<_>>();

    let first = entries
        .first()
        .ok_or_else(|| "No category found for the query".to_string())?;

    Ok(LiveDockResponse {
        ok: true,
        status: format!(
            "Found {} categories. Select one from the list.",
            entries.len()
        ),
        category_type: Some(first.category_type.clone()),
        category_id: Some(first.category_id.clone()),
        category_name: Some(first.category_name.clone()),
        categories: Some(entries),
        ..LiveDockResponse::default()
    })
}

fn parse_tags_input(raw: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = HashSet::new();

    for part in raw.split(|ch| ch == ',' || ch == '\n' || ch == '\r') {
        let tag = part.trim();
        if tag.is_empty() {
            continue;
        }

        let dedup_key = tag.to_ascii_lowercase();
        if seen.insert(dedup_key) {
            tags.push(tag.to_string());
        }
    }

    tags
}

pub(crate) fn apply_live_update_response(
    live_title: &str,
    category_type: &str,
    category_id: &str,
    tags_input: &str,
) -> Result<LiveDockResponse, String> {
    let settings = require_linked_settings()?;

    let mut update = ChzzkLiveSettingUpdate::default();
    let live_title = live_title.trim();
    let category_type = category_type.trim();
    let category_id = category_id.trim();

    if !live_title.is_empty() {
        update.default_live_title = Some(live_title.to_string());
    }

    let has_category_type = !category_type.is_empty();
    let has_category_id = !category_id.is_empty();

    if has_category_type != has_category_id {
        return Err("Set both category type and category ID, or leave both empty".to_string());
    }

    if has_category_type {
        update.category_type = Some(category_type.to_string());
        update.category_id = Some(category_id.to_string());
    }

    let tags = parse_tags_input(tags_input);
    if !tags.is_empty() {
        update.tags = Some(tags);
    }

    if update.is_empty() {
        return Err("No live setting changes to apply".to_string());
    }

    apply_update_and_reload(
        &settings,
        &update,
        "Live setting updated",
        "Live setting updated",
        "Failed to update live setting",
    )
}

pub(crate) fn clear_live_tags_response() -> Result<LiveDockResponse, String> {
    let settings = require_linked_settings()?;

    let clear_update = ChzzkLiveSettingUpdate {
        tags: Some(Vec::new()),
        ..ChzzkLiveSettingUpdate::default()
    };

    apply_update_and_reload(
        &settings,
        &clear_update,
        "Live tags cleared",
        "Live tags cleared",
        "Failed to clear live tags",
    )
}

pub(crate) fn clear_live_category_response() -> Result<LiveDockResponse, String> {
    let settings = require_linked_settings()?;

    let clear_update = ChzzkLiveSettingUpdate {
        category_id: Some(String::new()),
        ..ChzzkLiveSettingUpdate::default()
    };

    apply_update_and_reload(
        &settings,
        &clear_update,
        "Live category cleared",
        "Live category cleared",
        "Failed to clear live category",
    )
}

unsafe fn c_input_value(value: *const c_char) -> String {
    if value.is_null() {
        return String::new();
    }

    trim_value(&ptr_to_string(value))
}

#[no_mangle]
pub extern "C" fn obs_chzzk_live_dock_load_current_json() -> *mut c_char {
    response_json_from_result(load_live_setting_response("Loaded current live setting"))
}

#[no_mangle]
pub unsafe extern "C" fn obs_chzzk_live_dock_search_category_json(
    query: *const c_char,
) -> *mut c_char {
    response_json_from_result(search_category_response(&c_input_value(query)))
}

#[no_mangle]
pub unsafe extern "C" fn obs_chzzk_live_dock_apply_update_json(
    live_title: *const c_char,
    category_type: *const c_char,
    category_id: *const c_char,
    tags: *const c_char,
) -> *mut c_char {
    response_json_from_result(apply_live_update_response(
        &c_input_value(live_title),
        &c_input_value(category_type),
        &c_input_value(category_id),
        &c_input_value(tags),
    ))
}

#[no_mangle]
pub extern "C" fn obs_chzzk_live_dock_clear_category_json() -> *mut c_char {
    response_json_from_result(clear_live_category_response())
}

#[no_mangle]
pub extern "C" fn obs_chzzk_live_dock_clear_tags_json() -> *mut c_char {
    response_json_from_result(clear_live_tags_response())
}

#[no_mangle]
pub unsafe extern "C" fn obs_chzzk_live_dock_free_json(json_text: *mut c_char) {
    if !json_text.is_null() {
        let _ = CString::from_raw(json_text);
    }
}

fn sync_source_auth_fields(settings: &PluginSettings) {
    let _ = update_source_text_fields(&[
        (
            KEY_CHZZK_AUTHORIZATION_TOKEN,
            settings.chzzk_authorization_token.as_str(),
        ),
        (KEY_CHZZK_AUTH_STATUS, settings.chzzk_auth_status.as_str()),
        (
            KEY_CHZZK_STREAM_KEY_STATUS,
            settings.chzzk_stream_key_status.as_str(),
        ),
    ]);
}

fn current_stream_key_status() -> String {
    let service = unsafe {
        let raw = obs_frontend_get_streaming_service();
        if raw.is_null() {
            core::ptr::null_mut()
        } else {
            obs_service_get_ref(raw)
        }
    };
    if service.is_null() {
        return stream_key_status_for_value("", false);
    }

    let (service_name, stream_key) = unsafe {
        let settings = obs_service_get_settings(service);
        let values = if settings.is_null() {
            (String::new(), String::new())
        } else {
            let values = (
                obs_data_value(settings, b"service\0"),
                obs_data_value(settings, b"key\0"),
            );
            obs_data_release(settings);
            values
        };
        obs_service_release(service);
        values
    };

    stream_key_status_for_value(&stream_key, service_name.eq_ignore_ascii_case("CHZZK"))
}

fn refresh_stream_key_status(settings: &mut PluginSettings) {
    settings.chzzk_stream_key_status = current_stream_key_status();
}

fn update_settings_runtime_snapshot(mut settings: PluginSettings) {
    sync_auth_status(&mut settings);
    refresh_stream_key_status(&mut settings);
    apply_runtime_settings(settings.clone());
    persist_settings_to_file(&settings);
    sync_source_auth_fields(&settings);
}

fn set_obs_chzzk_stream_key(stream_key: &str) -> Result<(), String> {
    let stream_key = stream_key.trim();
    if stream_key.is_empty() {
        return Err("CHZZK stream key is empty".to_string());
    }

    const CHZZK_SERVICE_ID: &str = "rtmp_common";
    const CHZZK_SERVICE_NAME: &str = "CHZZK";
    const CHZZK_PROTOCOL: &str = "RTMP";
    const CHZZK_SERVER: &str = "rtmp://global-rtmp.lip2.navercorp.com:8080/relay";
    const CHZZK_STREAM_KEY_LINK: &str = "https://studio.chzzk.naver.com/setting";

    let service = unsafe {
        let raw = obs_frontend_get_streaming_service();
        if raw.is_null() {
            core::ptr::null_mut()
        } else {
            obs_service_get_ref(raw)
        }
    };
    if service.is_null() {
        return Err("OBS streaming service is not available".to_string());
    }

    let result = unsafe {
        let service_type = ptr_to_string(obs_service_get_type(service));
        let settings = obs_service_get_settings(service);
        if settings.is_null() {
            obs_service_release(service);
            return Err("Failed to load OBS streaming service settings".to_string());
        }

        if service_type != CHZZK_SERVICE_ID {
            obs_service_release(service);
            obs_data_release(settings);
            return Err(format!(
                "OBS streaming service type '{}' is not supported for automatic CHZZK switching",
                if service_type.is_empty() {
                    "unset"
                } else {
                    service_type.as_str()
                }
            ));
        }

        obs_data_set_value(settings, b"service\0", CHZZK_SERVICE_NAME);
        obs_data_set_value(settings, b"protocol\0", CHZZK_PROTOCOL);
        obs_data_set_value(settings, b"server\0", CHZZK_SERVER);
        obs_data_set_value(settings, b"stream_key_link\0", CHZZK_STREAM_KEY_LINK);
        obs_data_set_value(settings, b"key\0", stream_key);
        obs_service_update(service, settings);
        obs_frontend_save_streaming_service();
        obs_data_release(settings);
        obs_service_release(service);
        Ok(())
    };

    result
}

fn settings_from_obs_data(data: *mut c_void) -> PluginSettings {
    let defaults = PluginSettings::default();
    let current = current_settings();

    let chzzk_api_base_url = unsafe { obs_data_value(data, KEY_CHZZK_API_BASE_URL) };
    let chzzk_client_id = unsafe { obs_data_value(data, KEY_CHZZK_CLIENT_ID) };
    let discord_application_id = unsafe { obs_data_value(data, KEY_DISCORD_APPLICATION_ID) };
    let discord_activity_name = unsafe { obs_data_value(data, KEY_DISCORD_ACTIVITY_NAME) };

    let parsed = PluginSettings {
        chzzk_client_id,
        chzzk_client_secret: unsafe { obs_data_value(data, KEY_CHZZK_CLIENT_SECRET) },
        chzzk_api_base_url: if chzzk_api_base_url.is_empty() {
            defaults.chzzk_api_base_url
        } else {
            chzzk_api_base_url
        },
        discord_application_id: if discord_application_id.is_empty() {
            defaults.discord_application_id
        } else {
            discord_application_id
        },
        discord_presence_enabled: unsafe { obs_data_bool(data, KEY_DISCORD_PRESENCE_ENABLED) },
        discord_activity_name: if discord_activity_name.is_empty() {
            defaults.discord_activity_name
        } else {
            discord_activity_name
        },
        // Keep auth token/status from runtime snapshot because these fields are not editable in properties UI.
        chzzk_authorization_token: current.chzzk_authorization_token,
        chzzk_auth_status: current.chzzk_auth_status,
        chzzk_stream_key_status: current.chzzk_stream_key_status,
    };

    let mut parsed = parsed;
    sync_auth_status(&mut parsed);
    refresh_stream_key_status(&mut parsed);
    parsed
}

unsafe extern "C" fn settings_source_get_name(_type_data: *mut c_void) -> *const c_char {
    c_char_ptr(SOURCE_NAME)
}

unsafe extern "C" fn settings_source_create(
    settings: *mut c_void,
    _source: *mut c_void,
) -> *mut c_void {
    info("settings source create called");
    let next = settings_from_obs_data(settings);
    apply_runtime_settings(next.clone());
    persist_settings_to_file(&next);

    Box::into_raw(Box::new(())) as *mut c_void
}

unsafe extern "C" fn settings_source_destroy(data: *mut c_void) {
    if !data.is_null() {
        let _ = Box::<()>::from_raw(data.cast());
    }
}

unsafe extern "C" fn settings_source_defaults(settings: *mut c_void) {
    let current = current_settings();

    obs_data_set_default_value(settings, KEY_CHZZK_CLIENT_ID, &current.chzzk_client_id);
    obs_data_set_default_value(
        settings,
        KEY_CHZZK_CLIENT_SECRET,
        &current.chzzk_client_secret,
    );
    obs_data_set_default_value(settings, KEY_CHZZK_API_BASE_URL, &current.chzzk_api_base_url);
    obs_data_set_default_value(
        settings,
        KEY_DISCORD_APPLICATION_ID,
        &current.discord_application_id,
    );
    obs_data_set_default_bool(
        settings,
        c_char_ptr(KEY_DISCORD_PRESENCE_ENABLED),
        current.discord_presence_enabled,
    );
    obs_data_set_default_value(
        settings,
        KEY_DISCORD_ACTIVITY_NAME,
        &current.discord_activity_name,
    );
    obs_data_set_default_value(
        settings,
        KEY_CHZZK_AUTHORIZATION_TOKEN,
        &current.chzzk_authorization_token,
    );
    obs_data_set_default_value(settings, KEY_CHZZK_AUTH_STATUS, &current.chzzk_auth_status);
    obs_data_set_default_value(
        settings,
        KEY_CHZZK_STREAM_KEY_STATUS,
        &current.chzzk_stream_key_status,
    );
}

unsafe extern "C" fn settings_source_properties(_data: *mut c_void) -> *mut c_void {
    let properties = obs_properties_create();

    let chzzk_api_group = obs_properties_create();
    obs_properties_add_text(
        chzzk_api_group,
        c_char_ptr(KEY_CHZZK_CLIENT_ID),
        c_char_ptr(b"CHZZK Client ID\0"),
        OBS_TEXT_PASSWORD,
    );
    obs_properties_add_text(
        chzzk_api_group,
        c_char_ptr(KEY_CHZZK_CLIENT_SECRET),
        c_char_ptr(b"CHZZK Client Secret\0"),
        OBS_TEXT_PASSWORD,
    );
    obs_properties_add_text(
        chzzk_api_group,
        c_char_ptr(KEY_CHZZK_API_BASE_URL),
        c_char_ptr(b"CHZZK API Base URL\0"),
        OBS_TEXT_DEFAULT,
    );
    obs_properties_add_group(
        properties,
        c_char_ptr(b"chzzk_api_group\0"),
        c_char_ptr(b"CHZZK API Settings\0"),
        OBS_GROUP_NORMAL,
        chzzk_api_group,
    );

    let account_group = obs_properties_create();
    obs_properties_add_button(
        account_group,
        c_char_ptr(b"chzzk_link_oauth\0"),
        c_char_ptr(b"Link CHZZK Account\0"),
        Some(oauth_button_clicked),
    );
    obs_properties_add_button(
        account_group,
        c_char_ptr(b"chzzk_revoke_oauth\0"),
        c_char_ptr(b"Revoke CHZZK Token\0"),
        Some(revoke_button_clicked),
    );
    obs_properties_add_text(
        account_group,
        c_char_ptr(KEY_CHZZK_AUTH_STATUS),
        c_char_ptr(b"Authorization Status\0"),
        OBS_TEXT_DEFAULT,
    );
    obs_properties_add_button(
        account_group,
        c_char_ptr(b"chzzk_set_stream_key\0"),
        c_char_ptr(b"Set CHZZK Stream Key\0"),
        Some(set_stream_key_button_clicked),
    );
    obs_properties_add_text(
        account_group,
        c_char_ptr(KEY_CHZZK_STREAM_KEY_STATUS),
        c_char_ptr(b"CHZZK Stream Key\0"),
        OBS_TEXT_DEFAULT,
    );
    obs_properties_add_group(
        properties,
        c_char_ptr(b"chzzk_account_group\0"),
        c_char_ptr(b"CHZZK Account\0"),
        OBS_GROUP_NORMAL,
        account_group,
    );

    let discord_group = obs_properties_create();
    obs_properties_add_text(
        discord_group,
        c_char_ptr(KEY_DISCORD_APPLICATION_ID),
        c_char_ptr(b"Discord Application ID\0"),
        OBS_TEXT_PASSWORD,
    );
    obs_properties_add_bool(
        discord_group,
        c_char_ptr(KEY_DISCORD_PRESENCE_ENABLED),
        c_char_ptr(b"Enable Discord Rich Presence\0"),
    );
    obs_properties_add_text(
        discord_group,
        c_char_ptr(KEY_DISCORD_ACTIVITY_NAME),
        c_char_ptr(b"Discord Activity Name\0"),
        OBS_TEXT_DEFAULT,
    );
    obs_properties_add_group(
        properties,
        c_char_ptr(b"discord_group\0"),
        c_char_ptr(b"Discord Presence\0"),
        OBS_GROUP_NORMAL,
        discord_group,
    );

    properties
}

unsafe extern "C" fn settings_source_update(_data: *mut c_void, settings: *mut c_void) {
    info("settings source update called");
    let previous = current_settings();
    let next = settings_from_obs_data(settings);

    apply_runtime_settings(next.clone());
    persist_settings_to_file(&next);

    if previous.discord_presence_enabled && !next.discord_presence_enabled {
        info("Discord Rich Presence disabled from GUI settings; stopping active presence");
        crate::stop_presence();
    }
}

unsafe extern "C" fn open_settings_dialog(_private_data: *mut c_void) {
    let source = SETTINGS_SOURCE.load(Ordering::SeqCst);
    if source.is_null() {
        warn("obs-chzzk-extension: settings source is not initialized");
        return;
    }

    // Re-check OBS streaming service/key right before opening the UI to avoid stale status text.
    update_settings_runtime_snapshot(current_settings());

    obs_frontend_open_source_properties(source);
}

unsafe extern "C" fn oauth_button_clicked(
    _properties: *mut c_void,
    _property: *mut c_void,
) -> bool {
    info("OAuth button clicked - initiating authorization flow");

    let settings = current_settings();
    match request_authorization_token(&settings) {
        Ok(token) => {
            info("Successfully obtained access token");
            let mut updated_settings = settings.clone();
            updated_settings.chzzk_authorization_token = token;
            update_settings_runtime_snapshot(updated_settings);
            true
        }
        Err(error) => {
            log_error(error);
            false
        }
    }
}

unsafe extern "C" fn revoke_button_clicked(
    _properties: *mut c_void,
    _property: *mut c_void,
) -> bool {
    info("Revoke button clicked - revoking CHZZK token");

    let settings = current_settings();
    match revoke_token(&settings) {
        Ok(()) => {
            info("Successfully revoked CHZZK token");
            let mut updated_settings = settings.clone();
            updated_settings.chzzk_authorization_token.clear();
            update_settings_runtime_snapshot(updated_settings);
            true
        }
        Err(error) => {
            log_error(error);
            false
        }
    }
}

unsafe extern "C" fn set_stream_key_button_clicked(
    _properties: *mut c_void,
    _property: *mut c_void,
) -> bool {
    info("Set stream key button clicked - fetching CHZZK stream key");

    let settings = current_settings();
    let access_token = settings.chzzk_authorization_token.trim();
    if access_token.is_empty() {
        log_error("CHZZK account is not linked. Run OAuth first.");
        return false;
    }

    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    match client.fetch_stream_key(access_token) {
        Ok(response) => match set_obs_chzzk_stream_key(&response.stream_key) {
            Ok(()) => {
                info("Successfully updated OBS CHZZK stream key");
                update_settings_runtime_snapshot(settings);
                true
            }
            Err(error) => {
                log_error(error);
                false
            }
        },
        Err(error) => {
            log_error(format!("Failed to fetch CHZZK stream key: {}", error));
            false
        }
    }
}

fn register_settings_source() {
    let info = ObsSourceInfoPartial {
        id: c_char_ptr(SOURCE_ID),
        type_: OBS_SOURCE_TYPE_INPUT,
        output_flags: 0,
        get_name: Some(settings_source_get_name),
        create: Some(settings_source_create),
        destroy: Some(settings_source_destroy),
        get_width: None,
        get_height: None,
        get_defaults: Some(settings_source_defaults),
        get_properties: Some(settings_source_properties),
        update: Some(settings_source_update),
    };

    unsafe {
        obs_register_source_s(
            (&info as *const ObsSourceInfoPartial).cast(),
            core::mem::size_of::<ObsSourceInfoPartial>(),
        );
    }
}

fn create_settings_source() {
    let source = unsafe {
        obs_source_create_private(
            c_char_ptr(SOURCE_ID),
            c_char_ptr(SOURCE_NAME),
            core::ptr::null_mut(),
        )
    };

    if source.is_null() {
        log_error("obs-chzzk-extension: failed to create settings source");
        return;
    }

    SETTINGS_SOURCE.store(source, Ordering::SeqCst);
}

fn register_live_editor_dock() {
    unsafe {
        let widget = obs_chzzk_live_dock_create_widget();
        if widget.is_null() {
            log_error("obs-chzzk-extension: failed to create CHZZK live dock widget");
            return;
        }

        if !obs_frontend_add_dock_by_id(
            c_char_ptr(LIVE_DOCK_ID),
            c_char_ptr(LIVE_DOCK_TITLE),
            widget,
        ) {
            log_error("obs-chzzk-extension: failed to register CHZZK live dock");
            obs_chzzk_live_dock_destroy_widget(widget);
            return;
        }

        info("CHZZK live editor dock registered");
    }
}

pub(crate) fn initialize_gui_settings() {
    info("initializing GUI settings");
    let loaded = load_profile_settings();
    update_settings_runtime_snapshot(loaded);

    register_settings_source();
    create_settings_source();
    sync_source_auth_fields(&current_settings());
    register_live_editor_dock();

    unsafe {
        obs_frontend_add_tools_menu_item(
            c_char_ptr(MENU_TITLE),
            Some(open_settings_dialog),
            core::ptr::null_mut(),
        );
    }
}

pub(crate) fn shutdown_gui_settings() {
    info("shutting down GUI settings");
    unsafe {
        obs_frontend_remove_dock(c_char_ptr(LIVE_DOCK_ID));
    }

    let source = SETTINGS_SOURCE.swap(core::ptr::null_mut(), Ordering::SeqCst);
    if !source.is_null() {
        unsafe {
            obs_source_release(source);
        }
    }
}

#[allow(improper_ctypes)]
extern "C" {
    fn obs_register_source_s(info: *const c_void, size: usize);

    fn obs_source_create_private(
        id: *const c_char,
        name: *const c_char,
        settings: *mut c_void,
    ) -> *mut c_void;

    fn obs_source_release(source: *mut c_void);

    fn obs_source_get_settings(source: *mut c_void) -> *mut c_void;

    fn obs_source_update(source: *mut c_void, settings: *mut c_void);

    fn obs_data_get_string(data: *mut c_void, name: *const c_char) -> *const c_char;

    fn obs_data_get_bool(data: *mut c_void, name: *const c_char) -> bool;

    fn obs_data_set_string(data: *mut c_void, name: *const c_char, value: *const c_char);

    fn obs_data_release(data: *mut c_void);

    fn obs_data_set_default_string(data: *mut c_void, name: *const c_char, value: *const c_char);

    fn obs_data_set_default_bool(data: *mut c_void, name: *const c_char, value: bool);

    fn obs_properties_create() -> *mut c_void;

    fn obs_properties_add_text(
        props: *mut c_void,
        name: *const c_char,
        desc: *const c_char,
        text_type: i32,
    ) -> *mut c_void;

    fn obs_properties_add_bool(
        props: *mut c_void,
        name: *const c_char,
        desc: *const c_char,
    ) -> *mut c_void;

    fn obs_properties_add_group(
        props: *mut c_void,
        name: *const c_char,
        desc: *const c_char,
        group_type: i32,
        group: *mut c_void,
    ) -> *mut c_void;

    fn obs_properties_add_button(
        props: *mut c_void,
        name: *const c_char,
        text: *const c_char,
        callback: Option<unsafe extern "C" fn(*mut c_void, *mut c_void) -> bool>,
    ) -> *mut c_void;

    fn obs_frontend_add_tools_menu_item(
        name: *const c_char,
        callback: Option<unsafe extern "C" fn(*mut c_void)>,
        private_data: *mut c_void,
    );

    fn obs_frontend_add_dock_by_id(
        id: *const c_char,
        title: *const c_char,
        widget: *mut c_void,
    ) -> bool;

    fn obs_frontend_remove_dock(id: *const c_char);

    fn obs_frontend_open_source_properties(source: *mut c_void);

    fn obs_frontend_get_streaming_service() -> *mut c_void;

    fn obs_frontend_save_streaming_service();

    fn obs_service_get_settings(service: *mut c_void) -> *mut c_void;

    fn obs_service_update(service: *mut c_void, settings: *mut c_void);

    fn obs_service_get_type(service: *mut c_void) -> *const c_char;

    fn obs_service_get_ref(service: *mut c_void) -> *mut c_void;

    fn obs_service_release(service: *mut c_void);

    fn obs_chzzk_live_dock_create_widget() -> *mut c_void;

    fn obs_chzzk_live_dock_destroy_widget(widget: *mut c_void);
}
