use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::chzzk::oauth_server::{OAuthCallbackData, OAuthCallbackServer};
use crate::chzzk::ChzzkClient;
use crate::logging::{debug, info};

use super::constants::{
    OAUTH_CALLBACK_POLL_INTERVAL_MS, OAUTH_CALLBACK_PORT, OAUTH_CALLBACK_WAIT_TIMEOUT_SECS,
};
use super::model::PluginSettings;

fn oauth_redirect_uri() -> String {
    format!("http://127.0.0.1:{}/callback", OAUTH_CALLBACK_PORT)
}

pub(crate) fn request_authorization_token(settings: &PluginSettings) -> Result<String, String> {
    if settings.chzzk_client_id.trim().is_empty() || settings.chzzk_client_secret.trim().is_empty()
    {
        return Err("obs-chzzk-extension: Client ID or Client Secret not configured".to_string());
    }

    debug(format!(
        "Starting OAuth callback server on port {}",
        OAUTH_CALLBACK_PORT
    ));
    let oauth_server = OAuthCallbackServer::new();

    let state = format!(
        "obs-chzzk-extension-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    );

    oauth_server
        .start(OAUTH_CALLBACK_PORT, &state)
        .map_err(|error| format!("Failed to start OAuth callback server: {}", error))?;

    debug("OAuth callback server started");

    let redirect_uri = oauth_redirect_uri();
    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    let authorize_uri = client
        .generate_authorization_uri(&settings.chzzk_client_id, &redirect_uri, Some(&state))
        .map_err(|error| format!("Failed to generate authorization URI: {}", error.0))?;

    info("Opening CHZZK authorization page in browser");

    open_url(&authorize_uri).map_err(|error| format!("Failed to open browser: {}", error))?;

    let callback_data = wait_for_oauth_callback(
        &oauth_server,
        Duration::from_secs(OAUTH_CALLBACK_WAIT_TIMEOUT_SECS),
    )
    .ok_or_else(|| "OAuth callback server did not receive authorization code".to_string())?;

    if callback_data.state != state {
        return Err("OAuth state mismatch - CSRF detected".to_string());
    }

    debug("OAuth callback received with valid state");
    client
        .exchange_authorization_code(
            &settings.chzzk_client_id,
            &settings.chzzk_client_secret,
            &callback_data.code,
            &state,
        )
        .map_err(|error| format!("Failed to exchange authorization code: {}", error))
}

pub(crate) fn revoke_token(settings: &PluginSettings) -> Result<(), String> {
    if settings.chzzk_client_id.trim().is_empty() || settings.chzzk_client_secret.trim().is_empty()
    {
        return Err("obs-chzzk-extension: Client ID or Client Secret not configured".to_string());
    }

    if settings.chzzk_authorization_token.trim().is_empty() {
        return Err("obs-chzzk-extension: no authorization token to revoke".to_string());
    }

    let client = ChzzkClient::new(&settings.chzzk_api_base_url);
    client
        .revoke_token(
            &settings.chzzk_client_id,
            &settings.chzzk_client_secret,
            &settings.chzzk_authorization_token,
        )
        .map_err(|error| format!("Failed to revoke CHZZK token: {}", error))
}

fn wait_for_oauth_callback(
    server: &OAuthCallbackServer,
    timeout: Duration,
) -> Option<OAuthCallbackData> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        if let Some(result) = server.get_result() {
            return Some(result);
        }
        thread::sleep(Duration::from_millis(OAUTH_CALLBACK_POLL_INTERVAL_MS));
    }
    None
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(url)
            .spawn()
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        Err("Unsupported OS for browser opening".to_string())
    }
}
