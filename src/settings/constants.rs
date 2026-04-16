pub(crate) const SOURCE_ID: &[u8] = b"obs_chzzk_extension_settings\0";
pub(crate) const SOURCE_NAME: &[u8] = b"OBS Chzzk Extension Settings\0";
pub(crate) const MENU_TITLE: &[u8] = b"OBS Chzzk Extension Settings...\0";
pub(crate) const LIVE_DOCK_ID: &[u8] = b"obs_chzzk_live_editor_dock\0";
pub(crate) const LIVE_DOCK_TITLE: &[u8] = b"CHZZK Live Editor\0";
pub(crate) const SETTINGS_FILE_NAME: &str = "obs_chzzk_extension.json";

pub(crate) const KEY_CHZZK_CLIENT_ID: &[u8] = b"chzzk_client_id\0";
pub(crate) const KEY_CHZZK_CLIENT_SECRET: &[u8] = b"chzzk_client_secret\0";
pub(crate) const KEY_CHZZK_API_BASE_URL: &[u8] = b"chzzk_api_base_url\0";
pub(crate) const KEY_DISCORD_APPLICATION_ID: &[u8] = b"discord_application_id\0";
pub(crate) const KEY_DISCORD_PRESENCE_ENABLED: &[u8] = b"discord_presence_enabled\0";
pub(crate) const KEY_DISCORD_ACTIVITY_NAME: &[u8] = b"discord_activity_name\0";
pub(crate) const KEY_CHZZK_AUTHORIZATION_TOKEN: &[u8] = b"chzzk_authorization_token\0";
pub(crate) const KEY_CHZZK_AUTH_STATUS: &[u8] = b"chzzk_auth_status\0";
pub(crate) const KEY_CHZZK_STREAM_KEY_STATUS: &[u8] = b"chzzk_stream_key_status\0";

pub(crate) const OBS_SOURCE_TYPE_INPUT: i32 = 0;
pub(crate) const OBS_TEXT_DEFAULT: i32 = 0;
pub(crate) const OBS_TEXT_PASSWORD: i32 = 1;
pub(crate) const OBS_GROUP_NORMAL: i32 = 0;

pub(crate) const OAUTH_CALLBACK_PORT: u16 = 20132;
pub(crate) const OAUTH_CALLBACK_WAIT_TIMEOUT_SECS: u64 = 120;
pub(crate) const OAUTH_CALLBACK_POLL_INTERVAL_MS: u64 = 200;
