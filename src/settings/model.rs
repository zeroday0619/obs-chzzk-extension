#[derive(Clone)]
pub(crate) struct PluginSettings {
    pub(crate) chzzk_client_id: String,
    pub(crate) chzzk_client_secret: String,
    pub(crate) chzzk_api_base_url: String,
    pub(crate) discord_application_id: String,
    pub(crate) discord_presence_enabled: bool,
    pub(crate) discord_activity_name: String,
    pub(crate) chzzk_authorization_token: String,
    pub(crate) chzzk_auth_status: String,
    pub(crate) chzzk_stream_key_status: String,
}

impl Default for PluginSettings {
    fn default() -> Self {
        Self {
            chzzk_client_id: String::new(),
            chzzk_client_secret: String::new(),
            chzzk_api_base_url: "https://openapi.chzzk.naver.com".to_string(),
            discord_application_id: String::new(),
            discord_presence_enabled: true,
            discord_activity_name: "CHZZK Live".to_string(),
            chzzk_authorization_token: String::new(),
            chzzk_auth_status: "CHZZK account not linked".to_string(),
            chzzk_stream_key_status: "CHZZK stream key not set".to_string(),
        }
    }
}

pub(crate) fn auth_status_for_token(token: &str) -> String {
    if token.trim().is_empty() {
        "CHZZK account not linked".to_string()
    } else {
        "CHZZK account linked".to_string()
    }
}

pub(crate) fn sync_auth_status(settings: &mut PluginSettings) {
    settings.chzzk_auth_status = auth_status_for_token(&settings.chzzk_authorization_token);
}

pub(crate) fn stream_key_status_for_value(stream_key: &str, is_chzzk_service: bool) -> String {
    if !is_chzzk_service {
        return "CHZZK stream key not set (OBS service is not CHZZK)".to_string();
    }

    if stream_key.trim().is_empty() {
        "CHZZK stream key not set".to_string()
    } else {
        "CHZZK stream key set".to_string()
    }
}
