use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::logging::{error as log_error, info, warn};

mod constants;
mod model;
mod oauth;
mod runtime;
mod storage;

use constants::{
    KEY_CHZZK_API_BASE_URL, KEY_CHZZK_AUTHORIZATION_TOKEN, KEY_CHZZK_AUTH_STATUS,
    KEY_CHZZK_CLIENT_ID, KEY_CHZZK_CLIENT_SECRET, KEY_DISCORD_ACTIVITY_NAME,
    KEY_DISCORD_APPLICATION_ID, KEY_DISCORD_PRESENCE_ENABLED, MENU_TITLE, OBS_GROUP_NORMAL,
    OBS_SOURCE_TYPE_INPUT, OBS_TEXT_DEFAULT, OBS_TEXT_PASSWORD, SOURCE_ID, SOURCE_NAME,
};
use model::sync_auth_status;
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

fn sync_source_auth_fields(settings: &PluginSettings) {
    let source = SETTINGS_SOURCE.load(Ordering::SeqCst);
    if source.is_null() {
        return;
    }

    unsafe {
        let data = obs_source_get_settings(source);
        if data.is_null() {
            warn("obs-chzzk-extension: failed to get source settings for auth sync");
            return;
        }

        let Some(token) = c_string(&settings.chzzk_authorization_token) else {
            log_error("obs-chzzk-extension: invalid authorization token value for source sync");
            obs_data_release(data);
            return;
        };
        let Some(status) = c_string(&settings.chzzk_auth_status) else {
            log_error("obs-chzzk-extension: invalid auth status value for source sync");
            obs_data_release(data);
            return;
        };

        obs_data_set_string(data, c_char_ptr(KEY_CHZZK_AUTHORIZATION_TOKEN), token.as_ptr());
        obs_data_set_string(data, c_char_ptr(KEY_CHZZK_AUTH_STATUS), status.as_ptr());
        obs_source_update(source, data);
        obs_data_release(data);
    }
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
    };

    let mut parsed = parsed;
    sync_auth_status(&mut parsed);
    parsed
}

unsafe extern "C" fn settings_source_get_name(_type_data: *mut c_void) -> *const c_char {
    c_char_ptr(SOURCE_NAME)
}

unsafe extern "C" fn settings_source_create(settings: *mut c_void, _source: *mut c_void) -> *mut c_void {
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

    if let Some(value) = c_string(&current.chzzk_client_id) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_CHZZK_CLIENT_ID), value.as_ptr());
    }
    if let Some(value) = c_string(&current.chzzk_client_secret) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_CHZZK_CLIENT_SECRET), value.as_ptr());
    }
    if let Some(value) = c_string(&current.chzzk_api_base_url) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_CHZZK_API_BASE_URL), value.as_ptr());
    }
    if let Some(value) = c_string(&current.discord_application_id) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_DISCORD_APPLICATION_ID), value.as_ptr());
    }
    obs_data_set_default_bool(
        settings,
        c_char_ptr(KEY_DISCORD_PRESENCE_ENABLED),
        current.discord_presence_enabled,
    );
    if let Some(value) = c_string(&current.discord_activity_name) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_DISCORD_ACTIVITY_NAME), value.as_ptr());
    }
    if let Some(value) = c_string(&current.chzzk_authorization_token) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_CHZZK_AUTHORIZATION_TOKEN), value.as_ptr());
    }
    if let Some(value) = c_string(&current.chzzk_auth_status) {
        obs_data_set_default_string(settings, c_char_ptr(KEY_CHZZK_AUTH_STATUS), value.as_ptr());
    }
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

    obs_frontend_open_source_properties(source);
}

unsafe extern "C" fn oauth_button_clicked(_properties: *mut c_void, _property: *mut c_void) -> bool {
    info("OAuth button clicked - initiating authorization flow");

    let settings = current_settings();
    match request_authorization_token(&settings) {
        Ok(token) => {
            info("Successfully obtained access token");
            let mut updated_settings = settings.clone();
            updated_settings.chzzk_authorization_token = token;
            sync_auth_status(&mut updated_settings);
            apply_runtime_settings(updated_settings.clone());
            persist_settings_to_file(&updated_settings);
            sync_source_auth_fields(&updated_settings);
            true
        }
        Err(error) => {
            log_error(error);
            false
        }
    }
}

unsafe extern "C" fn revoke_button_clicked(_properties: *mut c_void, _property: *mut c_void) -> bool {
    info("Revoke button clicked - revoking CHZZK token");

    let settings = current_settings();
    match revoke_token(&settings) {
        Ok(()) => {
            info("Successfully revoked CHZZK token");
            let mut updated_settings = settings.clone();
            updated_settings.chzzk_authorization_token.clear();
            sync_auth_status(&mut updated_settings);
            apply_runtime_settings(updated_settings.clone());
            persist_settings_to_file(&updated_settings);
            sync_source_auth_fields(&updated_settings);
            true
        }
        Err(error) => {
            log_error(error);
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

pub(crate) fn initialize_gui_settings() {
    info("initializing GUI settings");
    let loaded = load_profile_settings();
    apply_runtime_settings(loaded);

    register_settings_source();
    create_settings_source();
    sync_source_auth_fields(&current_settings());

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

    fn obs_frontend_open_source_properties(source: *mut c_void);
}
