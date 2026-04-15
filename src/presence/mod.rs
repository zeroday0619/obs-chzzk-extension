use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::chzzk::ChzzkClient;
use crate::logging::{debug, warn, info};
use crate::settings::current_settings;

#[allow(dead_code)]
#[derive(Clone, Copy)]
pub(crate) enum ActivityType {
    Playing = 0,
    Streaming = 1,
    Listening = 2,
    Watching = 3,
    Custom = 4,
    Competing = 5,
}

#[derive(Clone, Default)]
pub(crate) struct ActivityTimestamps {
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
}

#[allow(dead_code)]
#[derive(Clone, Default)]
pub(crate) struct ActivityAssets {
    pub(crate) large_image: Option<String>,
    pub(crate) large_text: Option<String>,
    pub(crate) small_image: Option<String>,
    pub(crate) small_text: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ActivityButton {
    pub(crate) label: String,
    pub(crate) url: String,
}

#[derive(Clone)]
pub(crate) struct PresenceActivity {
    pub(crate) name: String,
    pub(crate) kind: ActivityType,
    pub(crate) details: Option<String>,
    pub(crate) state: Option<String>,
    pub(crate) url: Option<String>,
    pub(crate) timestamps: ActivityTimestamps,
    pub(crate) assets: Option<ActivityAssets>,
    pub(crate) buttons: Vec<ActivityButton>,
}

#[allow(dead_code)]
impl PresenceActivity {
    pub(crate) fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            kind: ActivityType::Playing,
            details: None,
            state: None,
            url: None,
            timestamps: ActivityTimestamps::default(),
            assets: None,
            buttons: Vec::new(),
        }
    }

    pub(crate) fn playing(name: impl Into<String>) -> Self {
        Self::new(name).activity_type(ActivityType::Playing)
    }

    pub(crate) fn streaming(name: impl Into<String>) -> Self {
        Self::new(name).activity_type(ActivityType::Streaming)
    }

    pub(crate) fn listening(name: impl Into<String>) -> Self {
        Self::new(name).activity_type(ActivityType::Listening)
    }

    pub(crate) fn watching(name: impl Into<String>) -> Self {
        Self::new(name).activity_type(ActivityType::Watching)
    }

    pub(crate) fn activity_type(mut self, kind: ActivityType) -> Self {
        self.kind = kind;
        self
    }

    pub(crate) fn details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub(crate) fn state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub(crate) fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    pub(crate) fn timestamps(mut self, start: Option<u64>, end: Option<u64>) -> Self {
        self.timestamps = ActivityTimestamps { start, end };
        self
    }

    pub(crate) fn assets(mut self, assets: ActivityAssets) -> Self {
        self.assets = Some(assets);
        self
    }

    pub(crate) fn button(mut self, label: impl Into<String>, url: impl Into<String>) -> Self {
        if self.buttons.len() < 2 {
            self.buttons.push(ActivityButton {
                label: label.into(),
                url: url.into(),
            });
        }
        self
    }
}

#[derive(Clone)]
pub(crate) struct PresenceConfig {
    pub(crate) application_id: String,
    pub(crate) activity: PresenceActivity,
}

pub(crate) enum PresenceCommand {
    Stop,
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 2);

    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0C}' => escaped.push_str("\\f"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                write!(&mut escaped, "\\u{:04x}", ch as u32).expect("write to string");
            }
            ch => escaped.push(ch),
        }
    }

    escaped
}

fn json_string(value: &str) -> String {
    format!("\"{}\"", json_escape(value))
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn nonce() -> String {
    format!("{:x}", unix_now_ms())
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn to_discord_asset_image(value: &str) -> String {
    value
        .trim()
        .replace("{type}", "480")
        .replace("%7Btype%7D", "480")
        .replace("%7btype%7d", "480")
}

fn is_supported_stream_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    normalized.starts_with("https://twitch.tv/")
        || normalized.starts_with("https://www.twitch.tv/")
        || normalized.starts_with("https://youtube.com/")
        || normalized.starts_with("https://www.youtube.com/")
        || normalized.starts_with("https://chzzk.naver.com/")
}

pub(crate) fn build_presence_config() -> Option<PresenceConfig> {
    let settings = current_settings();
    if !settings.discord_presence_enabled {
        info("Discord Rich Presence is disabled in settings; skipping config build");
        return None;
    }

    debug(format!(
        "presence build precheck: has_auth_token={}, has_client_id={}, has_client_secret={}",
        !settings.chzzk_authorization_token.trim().is_empty(),
        !settings.chzzk_client_id.trim().is_empty(),
        !settings.chzzk_client_secret.trim().is_empty()
    ));

    let Some(application_id) = non_empty(&settings.discord_application_id) else {
        warn("Discord Application ID is empty; configure it in settings.");
        return None;
    };
    let mut user_channel_info = None;

    let api_base_url = non_empty(&settings.chzzk_api_base_url)
        .unwrap_or_else(|| "https://openapi.chzzk.naver.com".to_string());

    let client = ChzzkClient::new(&api_base_url);
    let Some(user_access_token) = non_empty(&settings.chzzk_authorization_token) else {
        warn("CHZZK authorization token is empty; OAuth login is required.");
        return None;
    };

    match client.fetch_user_channel_info(&user_access_token) {
        Ok(info) => {
            debug(format!(
                "CHZZK user channel loaded: has_channel_id={}, has_channel_name={}",
                info.channel_id.is_some(),
                info.channel_name.is_some()
            ));
            user_channel_info = Some(info);
        }
        Err(error) => {
            warn(format!("CHZZK user info is unavailable: {}", error));
        }
    }

    let live_settings_result = client.fetch_live_settings(&user_access_token);

    if let Err(error) = &live_settings_result {
        warn(format!("CHZZK API data is unavailable: {}", error));
    }

    let live_settings = live_settings_result.ok();
    debug(format!(
        "building presence: token_len={}, api_base={}",
        user_access_token.len(),
        api_base_url
    ));

    if live_settings.is_none() {
        warn("CHZZK API data is unavailable; using fallback presence text.");
    }

    let live_title = live_settings
        .as_ref()
        .and_then(|value| value.live_title.as_ref())
        .map(|title| title.trim())
        .filter(|title| !title.is_empty())
        .map(|title| title.to_string());

    let channel_name = user_channel_info
        .as_ref()
        .and_then(|value| value.channel_name.as_ref())
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string());

    let configured_activity_name = non_empty(&settings.discord_activity_name);
    let activity_name = match configured_activity_name {
        Some(name) if name != "CHZZK Live" => name,
        _ => match (&channel_name, &live_title) {
            (Some(channel), Some(title)) => format!("{} - {}", channel, title),
            (Some(channel), None) => format!("{} Live", channel),
            (None, Some(title)) => title.clone(),
            (None, None) => "CHZZK Live".to_string(),
        },
    };

    let details = live_title.unwrap_or_else(|| "치지직 라이브 송출 중".to_string());

    let state = live_settings
        .as_ref()
        .and_then(|value| {
            value
                .category_name
                .as_ref()
                .map(|category| format!("카테고리: {}", category))
        })
        .or_else(|| {
            channel_name.clone()
        })
        .unwrap_or_else(|| "CHZZK".to_string());

    let live_thumbnail_image_url = match client.fetch_live_thumbnail_image_url(
        user_channel_info
            .as_ref()
            .and_then(|value| value.channel_id.as_deref()),
    ) {
        Ok(url) => url,
        Err(error) => {
            warn(format!("CHZZK live thumbnail is unavailable: {}", error));
            None
        }
    };

    let button_url = user_channel_info
        .as_ref()
        .and_then(|value| value.channel_id.as_ref())
        .map(|channel_id| format!("https://chzzk.naver.com/live/{}", channel_id));

    let stream_url = button_url
        .as_ref()
        .filter(|url| is_supported_stream_url(url))
        .cloned();

    let activity_type = ActivityType::Playing;

    if button_url.is_some() && stream_url.is_none() {
        warn("streaming URL is not twitch/youtube; activity type remains PLAYING without url field");
    }

    let mut activity = PresenceActivity::new(activity_name.clone())
        .activity_type(activity_type)
        .details(details.clone())
        .state(state.clone())
        .timestamps(Some(unix_now_ms()), None);

    if let Some(url) = stream_url {
        activity = activity.url(url);
    }

    if let Some(url) = button_url {
        activity = activity.button("치지직 방송 보러가기", url);
    }

    if let Some(thumbnail_url) = live_thumbnail_image_url {
        let mut assets = ActivityAssets::default();
        assets.large_image = Some(to_discord_asset_image(&thumbnail_url));
        assets.large_text = Some(activity_name.clone());
        activity = activity.assets(assets);
    }

    info(format!(
        "presence fields ready: details_len={}, state_len={}, button_url={}, thumbnail={}",
        details.len(),
        state.len(),
        activity.url.is_some(),
        activity
            .assets
            .as_ref()
            .and_then(|value| value.large_image.as_ref())
            .is_some()
    ));

    Some(PresenceConfig {
        application_id,
        activity,
    })
}

fn build_activity_payload(config: &PresenceConfig) -> String {
    let mut fields = vec![
        format!("\"type\":{}", config.activity.kind as u8),
        format!("\"name\":{}", json_string(&config.activity.name)),
    ];

    if let Some(details) = &config.activity.details {
        fields.push(format!("\"details\":{}", json_string(details)));
    }

    if let Some(state) = &config.activity.state {
        fields.push(format!("\"state\":{}", json_string(state)));
    }

    if config.activity.timestamps.start.is_some() || config.activity.timestamps.end.is_some() {
        let mut ts_fields = Vec::new();
        if let Some(start) = config.activity.timestamps.start {
            ts_fields.push(format!("\"start\":{}", start));
        }
        if let Some(end) = config.activity.timestamps.end {
            ts_fields.push(format!("\"end\":{}", end));
        }
        fields.push(format!("\"timestamps\":{{{}}}", ts_fields.join(",")));
    }

    if let Some(url) = &config.activity.url {
        fields.push(format!("\"url\":{}", json_string(url)));
    }

    if let Some(assets) = &config.activity.assets {
        let mut assets_fields = Vec::new();
        if let Some(v) = &assets.large_image {
            assets_fields.push(format!("\"large_image\":{}", json_string(v)));
        }
        if let Some(v) = &assets.large_text {
            assets_fields.push(format!("\"large_text\":{}", json_string(v)));
        }
        if let Some(v) = &assets.small_image {
            assets_fields.push(format!("\"small_image\":{}", json_string(v)));
        }
        if let Some(v) = &assets.small_text {
            assets_fields.push(format!("\"small_text\":{}", json_string(v)));
        }
        if !assets_fields.is_empty() {
            fields.push(format!("\"assets\":{{{}}}", assets_fields.join(",")));
        }
    }

    if !config.activity.buttons.is_empty() {
        let buttons = config
            .activity
            .buttons
            .iter()
            .take(2)
            .map(|button| {
                format!(
                    "{{\"label\":{},\"url\":{}}}",
                    json_string(&button.label),
                    json_string(&button.url)
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        fields.push(format!("\"buttons\":[{}]", buttons));
    }

    format!("{{{}}}", fields.join(","))
}

pub(crate) fn build_set_activity_payload(config: &PresenceConfig) -> String {
    let activity = build_activity_payload(config);
    debug(format!(
        "building SET_ACTIVITY payload: activity_name='{}', has_button={}",
        config.activity.name,
        !config.activity.buttons.is_empty()
    ));
    format!(
        "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{},\"activity\":{}}},\"nonce\":{}}}",
        process::id(),
        activity,
        json_string(&nonce())
    )
}

pub(crate) fn build_clear_activity_payload() -> String {
    debug("building CLEAR_ACTIVITY payload");
    format!(
        "{{\"cmd\":\"SET_ACTIVITY\",\"args\":{{\"pid\":{},\"activity\":null}},\"nonce\":{}}}",
        process::id(),
        json_string(&nonce())
    )
}

pub(crate) fn build_handshake_payload(application_id: &str) -> String {
    debug(format!("building Discord handshake payload for app_id={}", application_id));
    format!("{{\"v\":1,\"client_id\":{}}}", json_string(application_id))
}
