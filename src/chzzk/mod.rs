use std::fmt;
use std::time::Duration;

use serde_json::Value;

use crate::logging::{debug, error as log_error, info};

pub(crate) mod oauth_server;

const DEFAULT_API_BASE: &str = "https://openapi.chzzk.naver.com";
const DEFAULT_AUTH_BASE: &str = "https://chzzk.naver.com";
const DEFAULT_SERVICE_BASE: &str = "https://api.chzzk.naver.com";
const DEFAULT_UNOFFICIAL_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";
const MASKED: &str = "***";

#[derive(Clone)]
pub(crate) struct ChzzkLiveSettings {
    pub(crate) live_title: Option<String>,
    pub(crate) category_type: Option<String>,
    pub(crate) category_id: Option<String>,
    pub(crate) category_name: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
}

#[derive(Clone)]
pub(crate) struct ChzzkCategory {
    pub(crate) category_type: String,
    pub(crate) category_id: String,
    pub(crate) category_value: String,
    pub(crate) poster_image_url: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct ChzzkLiveSettingUpdate {
    pub(crate) default_live_title: Option<String>,
    pub(crate) category_type: Option<String>,
    pub(crate) category_id: Option<String>,
    pub(crate) tags: Option<Vec<String>>,
}

impl ChzzkLiveSettingUpdate {
    pub(crate) fn is_empty(&self) -> bool {
        self.default_live_title.is_none()
            && self.category_type.is_none()
            && self.category_id.is_none()
            && self.tags.is_none()
    }
}

#[derive(Clone)]
pub(crate) struct ChzzkUserChannelInfo {
    pub(crate) channel_id: Option<String>,
    pub(crate) channel_name: Option<String>,
}

#[derive(Clone)]
pub(crate) struct ChzzkStreamKey {
    pub(crate) stream_key: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ChzzkApiError(pub(crate) String);

impl fmt::Display for ChzzkApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub(crate) struct ChzzkClient {
    api_base: String,
    auth_base: String,
    agent: ureq::Agent,
}

impl ChzzkClient {
    pub(crate) fn new(api_base: &str) -> Self {
        let base = if api_base.trim().is_empty() {
            DEFAULT_API_BASE.to_string()
        } else {
            api_base.trim().trim_end_matches('/').to_string()
        };
        debug(format!("CHZZK client init: api_base={}", base));

        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(3))
            .timeout_read(Duration::from_secs(3))
            .timeout_write(Duration::from_secs(3))
            .build();

        Self {
            api_base: base,
            auth_base: DEFAULT_AUTH_BASE.to_string(),
            agent,
        }
    }

    pub(crate) fn generate_authorization_uri(
        &self,
        client_id: &str,
        redirect_uri: &str,
        state: Option<&str>,
    ) -> Result<String, ChzzkApiError> {
        let client_id = client_id.trim();
        let redirect_uri = redirect_uri.trim();

        if client_id.is_empty() {
            return Err(ChzzkApiError("client_id is required".to_string()));
        }
        if redirect_uri.is_empty() {
            return Err(ChzzkApiError("redirect_uri is required".to_string()));
        }

        let mut uri = format!(
            "{}/account-interlock?responseType=code&clientId={}&redirectUri={}",
            self.auth_base,
            simple_url_encode(client_id),
            simple_url_encode(redirect_uri)
        );

        if let Some(s) = state {
            let state = s.trim();
            if !state.is_empty() {
                uri.push_str(&format!("&state={}", simple_url_encode(state)));
            }
        }

        debug(format!("Generated CHZZK authorization URI"));
        Ok(uri)
    }

    pub(crate) fn exchange_authorization_code(
        &self,
        client_id: &str,
        client_secret: &str,
        authorization_code: &str,
        state: &str,
    ) -> Result<String, ChzzkApiError> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        let code = authorization_code.trim();
        let state_val = state.trim();

        if client_id.is_empty() {
            return Err(ChzzkApiError("client_id is required".to_string()));
        }
        if client_secret.is_empty() {
            return Err(ChzzkApiError("client_secret is required".to_string()));
        }
        if code.is_empty() {
            return Err(ChzzkApiError("authorization_code is required".to_string()));
        }
        if state_val.is_empty() {
            return Err(ChzzkApiError("state is required".to_string()));
        }

        let endpoint = format!("{}/auth/v1/token", self.api_base);
        info(format!("CHZZK token exchange request: {}", endpoint));

        let mut body = serde_json::Map::new();
        body.insert(
            "grantType".to_string(),
            Value::String("authorization_code".to_string()),
        );
        body.insert("clientId".to_string(), Value::String(client_id.to_string()));
        body.insert(
            "clientSecret".to_string(),
            Value::String(client_secret.to_string()),
        );
        body.insert("code".to_string(), Value::String(code.to_string()));
        body.insert("state".to_string(), Value::String(state_val.to_string()));

        let root = self.post_json(&endpoint, Value::Object(body))?;
        let access_token = extract_access_token(&root)?;

        info(format!(
            "CHZZK access token exchanged: len={}",
            access_token.len()
        ));
        Ok(access_token)
    }

    pub(crate) fn fetch_live_settings(
        &self,
        access_token: &str,
    ) -> Result<ChzzkLiveSettings, ChzzkApiError> {
        let endpoint = format!("{}/open/v1/lives/setting", self.api_base);
        let authorization = format!("Bearer {}", access_token);
        let masked_authorization = format!("Bearer {}", mask_secret(access_token));

        debug(format!(
            "CHZZK API request: method=GET endpoint={} headers={{\"Authorization\":\"{}\",\"Content-Type\":\"application/json\"}}",
            endpoint, masked_authorization
        ));

        let root = self
            .agent
            .get(&endpoint)
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .call()
            .map_err(|error| to_api_error(error, "CHZZK live-setting request failed"))?
            .into_json::<Value>()
            .map_err(|error| ChzzkApiError(format!("CHZZK live-setting parse failed: {error}")))?;

        debug(format!(
            "CHZZK API response: method=GET endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        let payload = extract_payload(&root);
        let category = payload.get("category");

        let live_title = extract_string(payload, "defaultLiveTitle");
        let category_type = category
            .and_then(|cat| extract_string(cat, "categoryType"))
            .or_else(|| extract_string(payload, "categoryType"));
        let category_id = category
            .and_then(|cat| extract_string(cat, "categoryId"))
            .or_else(|| extract_string(payload, "categoryId"))
            .or_else(|| extract_string(payload, "liveCategory"));
        let category_name = category
            .and_then(|cat| extract_string(cat, "categoryValue"))
            .or_else(|| extract_string(payload, "liveCategoryValue"));
        let tags = extract_string_array(payload, "tags");

        debug(format!(
            "CHZZK live-setting parsed: title={}, category_type={}, category_id={}, category={}, tags={}",
            live_title.is_some(),
            category_type.is_some(),
            category_id.is_some(),
            category_name.is_some(),
            tags.is_some()
        ));

        Ok(ChzzkLiveSettings {
            live_title,
            category_type,
            category_id,
            category_name,
            tags,
        })
    }

    pub(crate) fn fetch_stream_key(
        &self,
        access_token: &str,
    ) -> Result<ChzzkStreamKey, ChzzkApiError> {
        let access_token = access_token.trim();
        if access_token.is_empty() {
            return Err(ChzzkApiError("access_token is required".to_string()));
        }

        let endpoint = format!("{}/open/v1/streams/key", self.api_base);
        let authorization = format!("Bearer {}", access_token);
        let masked_authorization = format!("Bearer {}", mask_secret(access_token));

        debug(format!(
            "CHZZK API request: method=GET endpoint={} headers={{\"Authorization\":\"{}\",\"Content-Type\":\"application/json\"}}",
            endpoint, masked_authorization
        ));

        let root = self
            .agent
            .get(&endpoint)
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .call()
            .map_err(|error| to_api_error(error, "CHZZK stream-key request failed"))?
            .into_json::<Value>()
            .map_err(|error| ChzzkApiError(format!("CHZZK stream-key parse failed: {error}")))?;

        debug(format!(
            "CHZZK API response: method=GET endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        let payload = extract_payload(&root);
        let stream_key = extract_string(payload, "streamKey").ok_or_else(|| {
            ChzzkApiError(format!(
                "CHZZK stream-key response missing streamKey: {}",
                masked_json_string(&root)
            ))
        })?;

        Ok(ChzzkStreamKey { stream_key })
    }

    pub(crate) fn update_live_settings(
        &self,
        access_token: &str,
        update: &ChzzkLiveSettingUpdate,
    ) -> Result<(), ChzzkApiError> {
        let access_token = access_token.trim();
        if access_token.is_empty() {
            return Err(ChzzkApiError("access_token is required".to_string()));
        }
        if update.is_empty() {
            return Err(ChzzkApiError(
                "live setting update payload is empty".to_string(),
            ));
        }

        let endpoint = format!("{}/open/v1/lives/setting", self.api_base);
        let authorization = format!("Bearer {}", access_token);
        let masked_authorization = format!("Bearer {}", mask_secret(access_token));

        let mut body = serde_json::Map::new();

        if let Some(title) = update.default_live_title.as_deref() {
            let title = title.trim();
            if title.is_empty() {
                return Err(ChzzkApiError(
                    "default_live_title cannot be empty when provided".to_string(),
                ));
            }
            body.insert(
                "defaultLiveTitle".to_string(),
                Value::String(title.to_string()),
            );
        }

        if let Some(category_type) = update
            .category_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            body.insert(
                "categoryType".to_string(),
                Value::String(category_type.to_string()),
            );
        }

        if let Some(category_id) = &update.category_id {
            body.insert(
                "categoryId".to_string(),
                Value::String(category_id.trim().to_string()),
            );
        }

        if let Some(tags) = &update.tags {
            let tag_values = tags
                .iter()
                .map(|tag| tag.trim())
                .filter(|tag| !tag.is_empty())
                .map(|tag| Value::String(tag.to_string()))
                .collect::<Vec<_>>();
            body.insert("tags".to_string(), Value::Array(tag_values));
        }

        if body.is_empty() {
            return Err(ChzzkApiError(
                "live setting update payload has no valid fields".to_string(),
            ));
        }

        let body = Value::Object(body);

        debug(format!(
            "CHZZK API request: method=PATCH endpoint={} headers={{\"Authorization\":\"{}\",\"Content-Type\":\"application/json\"}} body={}",
            endpoint,
            masked_authorization,
            masked_json_string(&body)
        ));

        let root = self
            .agent
            .request("PATCH", &endpoint)
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|error| to_api_error(error, "CHZZK live-setting patch request failed"))?
            .into_json::<Value>()
            .map_err(|error| {
                ChzzkApiError(format!("CHZZK live-setting patch parse failed: {error}"))
            })?;

        debug(format!(
            "CHZZK API response: method=PATCH endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        Ok(())
    }

    pub(crate) fn search_categories(
        &self,
        client_id: &str,
        client_secret: &str,
        query: &str,
        size: Option<u32>,
    ) -> Result<Vec<ChzzkCategory>, ChzzkApiError> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        let query = query.trim();

        if client_id.is_empty() {
            return Err(ChzzkApiError("client_id is required".to_string()));
        }
        if client_secret.is_empty() {
            return Err(ChzzkApiError("client_secret is required".to_string()));
        }
        if query.is_empty() {
            return Err(ChzzkApiError("query is required".to_string()));
        }

        let size = size.unwrap_or(20).clamp(1, 50);
        let endpoint = format!(
            "{}/open/v1/categories/search?query={}&size={}",
            self.api_base,
            simple_url_encode(query),
            size
        );

        debug(format!(
            "CHZZK API request: method=GET endpoint={} headers={{\"Client-Id\":\"{}\",\"Client-Secret\":\"{}\",\"Content-Type\":\"application/json\"}}",
            endpoint,
            mask_secret(client_id),
            mask_secret(client_secret)
        ));

        let root = self
            .agent
            .get(&endpoint)
            .set("Client-Id", client_id)
            .set("Client-Secret", client_secret)
            .set("Content-Type", "application/json")
            .call()
            .map_err(|error| to_api_error(error, "CHZZK category search request failed"))?
            .into_json::<Value>()
            .map_err(|error| {
                ChzzkApiError(format!("CHZZK category search parse failed: {error}"))
            })?;

        debug(format!(
            "CHZZK API response: method=GET endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        let payload = extract_payload(&root);
        let category_items = payload.as_array().ok_or_else(|| {
            ChzzkApiError(format!("CHZZK category response missing list: {}", root))
        })?;

        let categories = category_items
            .iter()
            .filter_map(|item| {
                let category_type = extract_string(item, "categoryType")?;
                let category_id = extract_string(item, "categoryId")?;
                let category_value = extract_string(item, "categoryValue")?;
                let poster_image_url = extract_string(item, "posterImageUrl");

                Some(ChzzkCategory {
                    category_type,
                    category_id,
                    category_value,
                    poster_image_url,
                })
            })
            .collect::<Vec<_>>();

        debug(format!(
            "CHZZK category search parsed: query='{}', count={}",
            query,
            categories.len()
        ));

        Ok(categories)
    }

    pub(crate) fn fetch_live_thumbnail_image_url(
        &self,
        channel_id: Option<&str>,
    ) -> Result<Option<String>, ChzzkApiError> {
        let channel_id = channel_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ChzzkApiError("channel_id is required for unofficial live-detail".to_string())
            })?;

        let endpoint = format!(
            "{}/service/v2/channels/{}/live-detail",
            DEFAULT_SERVICE_BASE,
            simple_url_encode(channel_id)
        );

        debug(format!(
            "CHZZK unofficial request: method=GET endpoint={} headers={{\"User-Agent\":\"{}\",\"Accept\":\"application/json\"}}",
            endpoint, DEFAULT_UNOFFICIAL_USER_AGENT
        ));

        let root = self
            .agent
            .get(&endpoint)
            .set("User-Agent", DEFAULT_UNOFFICIAL_USER_AGENT)
            .set("Accept", "application/json")
            .set("Referer", "https://chzzk.naver.com")
            .call()
            .map_err(|error| to_api_error(error, "CHZZK unofficial live-detail request failed"))?
            .into_json::<Value>()
            .map_err(|error| {
                ChzzkApiError(format!(
                    "CHZZK unofficial live-detail parse failed: {error}"
                ))
            })?;

        debug(format!(
            "CHZZK unofficial response: method=GET endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        let payload = extract_payload(&root);
        let thumbnail = extract_live_item_thumbnail_url(payload).or_else(|| {
            payload
                .get("live")
                .and_then(extract_live_item_thumbnail_url)
        });

        debug(format!(
            "CHZZK unofficial live-detail parsed: channel_id={}, has_thumbnail={}",
            channel_id,
            thumbnail.is_some()
        ));

        Ok(thumbnail)
    }

    pub(crate) fn fetch_user_channel_info(
        &self,
        access_token: &str,
    ) -> Result<ChzzkUserChannelInfo, ChzzkApiError> {
        let access_token = access_token.trim();
        if access_token.is_empty() {
            return Err(ChzzkApiError("access_token is required".to_string()));
        }

        let endpoint = format!("{}/open/v1/users/me", self.api_base);
        let authorization = format!("Bearer {}", access_token);
        let masked_authorization = format!("Bearer {}", mask_secret(access_token));

        debug(format!(
            "CHZZK API request: method=GET endpoint={} headers={{\"Authorization\":\"{}\",\"Content-Type\":\"application/json\"}}",
            endpoint, masked_authorization
        ));

        let root = self
            .agent
            .get(&endpoint)
            .set("Authorization", &authorization)
            .set("Content-Type", "application/json")
            .call()
            .map_err(|error| to_api_error(error, "CHZZK user request failed"))?
            .into_json::<Value>()
            .map_err(|error| ChzzkApiError(format!("CHZZK user parse failed: {error}")))?;

        debug(format!(
            "CHZZK API response: method=GET endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        let payload = extract_payload(&root);
        let channel_root = payload.get("channel").unwrap_or(payload);

        let channel_id = extract_string(channel_root, "channelId")
            .or_else(|| extract_string(payload, "channelId"));
        let channel_name = extract_string(channel_root, "channelName")
            .or_else(|| extract_string(payload, "channelName"));

        debug(format!(
            "CHZZK user parsed: has_channel_id={}, has_channel_name={}",
            channel_id.is_some(),
            channel_name.is_some()
        ));

        Ok(ChzzkUserChannelInfo {
            channel_id,
            channel_name,
        })
    }

    pub(crate) fn revoke_token(
        &self,
        client_id: &str,
        client_secret: &str,
        token: &str,
    ) -> Result<(), ChzzkApiError> {
        let client_id = client_id.trim();
        let client_secret = client_secret.trim();
        let token_val = token.trim();

        if client_id.is_empty() {
            return Err(ChzzkApiError("client_id is required".to_string()));
        }
        if client_secret.is_empty() {
            return Err(ChzzkApiError("client_secret is required".to_string()));
        }
        if token_val.is_empty() {
            return Err(ChzzkApiError("token is required".to_string()));
        }

        let endpoint = format!("{}/auth/v1/token/revoke", self.api_base);
        info(format!("CHZZK token revoke request: {}", endpoint));

        let mut body = serde_json::Map::new();
        body.insert("clientId".to_string(), Value::String(client_id.to_string()));
        body.insert(
            "clientSecret".to_string(),
            Value::String(client_secret.to_string()),
        );
        body.insert("token".to_string(), Value::String(token_val.to_string()));

        let _root = self.post_json(&endpoint, Value::Object(body))?;
        info("CHZZK token revoked");
        Ok(())
    }

    fn post_json(&self, endpoint: &str, body: Value) -> Result<Value, ChzzkApiError> {
        debug(format!(
            "CHZZK API request: method=POST endpoint={} headers={{\"Content-Type\":\"application/json\"}} body={}",
            endpoint,
            masked_json_string(&body)
        ));

        let root = self
            .agent
            .post(endpoint)
            .set("Content-Type", "application/json")
            .send_json(body)
            .map_err(|error| to_api_error(error, "CHZZK POST request failed"))?
            .into_json::<Value>()
            .map_err(|error| ChzzkApiError(format!("CHZZK response parse failed: {error}")))?;

        debug(format!(
            "CHZZK API response: method=POST endpoint={} body={}",
            endpoint,
            masked_json_string(&root)
        ));

        validate_api_code(&root)?;
        Ok(root)
    }
}

fn to_api_error(error: ureq::Error, context: &str) -> ChzzkApiError {
    let message = match error {
        ureq::Error::Status(status, response) => {
            let body = response.into_string().unwrap_or_default();
            let detail = extract_error_detail(&body);
            format!("{} ({}): {}", context, status, detail)
        }
        other => format!("{}: {}", context, other),
    };
    log_error(&message);
    ChzzkApiError(message)
}

fn extract_error_detail(raw_body: &str) -> String {
    match serde_json::from_str::<Value>(raw_body) {
        Ok(value) => value
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or(raw_body)
            .to_string(),
        Err(_) => raw_body.to_string(),
    }
}

fn mask_secret(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 6 {
        return MASKED.to_string();
    }

    let prefix: String = chars.iter().take(3).collect();
    let suffix: String = chars
        .iter()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("{prefix}{MASKED}{suffix}")
}

fn mask_json_value(key: Option<&str>, value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut masked = serde_json::Map::with_capacity(map.len());
            for (child_key, child_value) in map {
                if is_sensitive_key(child_key) {
                    if child_value.is_null() {
                        masked.insert(child_key.clone(), Value::Null);
                    } else {
                        masked.insert(child_key.clone(), Value::String(MASKED.to_string()));
                    }
                } else {
                    masked.insert(
                        child_key.clone(),
                        mask_json_value(Some(child_key), child_value),
                    );
                }
            }
            Value::Object(masked)
        }
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| mask_json_value(key, item))
                .collect(),
        ),
        Value::String(text) => {
            if key.is_some_and(is_sensitive_key) {
                Value::String(mask_secret(text))
            } else {
                Value::String(text.clone())
            }
        }
        other => other.clone(),
    }
}

fn masked_json_string(value: &Value) -> String {
    let masked = mask_json_value(None, value);
    serde_json::to_string(&masked).unwrap_or_else(|_| "<json-encode-failed>".to_string())
}

fn validate_api_code(root: &Value) -> Result<(), ChzzkApiError> {
    let code = root.get("code").and_then(Value::as_i64).unwrap_or(200);
    if code == 200 {
        return Ok(());
    }

    let message = root
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("unknown error");
    Err(ChzzkApiError(format!(
        "CHZZK API error: code={}, message={}",
        code, message
    )))
}

fn extract_string(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
}

fn extract_string_array(value: &Value, key: &str) -> Option<Vec<String>> {
    let items = value.get(key)?.as_array()?;
    let values = items
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(|text| text.to_string())
        .collect::<Vec<_>>();

    Some(values)
}

fn extract_payload<'a>(root: &'a Value) -> &'a Value {
    let content = root.get("content").unwrap_or(root);
    content.get("data").unwrap_or(content)
}

fn extract_access_token(root: &Value) -> Result<String, ChzzkApiError> {
    let payload = extract_payload(root);
    extract_string(payload, "accessToken")
        .or_else(|| extract_string(payload, "access_token"))
        .ok_or_else(|| {
            ChzzkApiError(format!(
                "CHZZK token response missing accessToken: {}",
                root
            ))
        })
}

fn extract_live_item_thumbnail_url(item: &Value) -> Option<String> {
    extract_string(item, "liveImageUrl")
        .or_else(|| extract_string(item, "defaultThumbnailImageUrl"))
        .or_else(|| extract_string(item, "liveThumbnailImageURL"))
        .or_else(|| extract_string(item, "liveThumbnailImageUrl"))
        .or_else(|| extract_string(item, "thumbnailImageUrl"))
        .or_else(|| extract_string(item, "thumbnail_image_url"))
        .or_else(|| extract_string(item, "live_thumbnail_image_url"))
        .or_else(|| extract_string(item, "live_image_url"))
        .or_else(|| extract_string(item, "default_thumbnail_image_url"))
        .or_else(|| {
            item.get("liveImage")
                .and_then(|value| extract_string(value, "url"))
        })
}

fn simple_url_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            other => encoded.push_str(&format!("%{:02X}", other)),
        }
    }
    encoded
}

fn is_sensitive_key(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "authorization"
            | "clientid"
            | "client_id"
            | "client-id"
            | "clientsecret"
            | "client_secret"
            | "client-secret"
            | "code"
            | "state"
            | "token"
            | "accesstoken"
            | "access_token"
            | "access-token"
            | "refreshtoken"
            | "refresh_token"
            | "refresh-token"
            | "streamkey"
            | "stream_key"
            | "stream-key"
    )
}
