use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::logging::{debug, error as log_error, info};

const MAX_REQUEST_BYTES: usize = 8192;
const MAX_CALLBACK_PARAM_BYTES: usize = 2048;
const READ_TIMEOUT_SECS: u64 = 10;
const ACCEPT_RETRY_INTERVAL_MS: u64 = 25;

#[derive(Clone)]
pub(crate) struct OAuthCallbackData {
    pub(crate) code: String,
    pub(crate) state: String,
}

pub(crate) struct OAuthCallbackServer {
    result: Arc<Mutex<Option<OAuthCallbackData>>>,
    shutdown: Arc<AtomicBool>,
}

impl OAuthCallbackServer {
    pub(crate) fn new() -> Self {
        Self {
            result: Arc::new(Mutex::new(None)),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn start(
        &self,
        port: u16,
        expected_state: &str,
    ) -> Result<thread::JoinHandle<()>, String> {
        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|e| format!("Failed to bind OAuth callback server: {}", e))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("Failed to configure OAuth callback server: {}", e))?;

        info(format!("OAuth callback server listening on port {}", port));

        let result = Arc::clone(&self.result);
        let shutdown = Arc::clone(&self.shutdown);
        let expected_state = expected_state.to_string();

        let handle = thread::spawn(move || {
            loop {
                if shutdown.load(Ordering::Relaxed) {
                    debug("OAuth callback server stopping by request");
                    break;
                }

                match listener.accept() {
                    Ok((stream, _)) => {
                        debug("OAuth callback server received request");
                        handle_client(stream, result, &expected_state);
                        break;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(ACCEPT_RETRY_INTERVAL_MS));
                    }
                    Err(error) => {
                        log_error(format!("OAuth callback accept failed: {}", error));
                        break;
                    }
                }
            }
        });

        Ok(handle)
    }

    pub(crate) fn stop(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }

    pub(crate) fn get_result(&self) -> Option<OAuthCallbackData> {
        self.result.lock().ok().and_then(|r| r.clone())
    }
}

fn handle_client(
    mut stream: TcpStream,
    result: Arc<Mutex<Option<OAuthCallbackData>>>,
    expected_state: &str,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(READ_TIMEOUT_SECS)));

    let mut buffer = [0; MAX_REQUEST_BYTES];
    let n = match stream.read(&mut buffer) {
        Ok(n) => n,
        Err(error) => {
            log_error(format!("OAuth callback read failed: {}", error));
            send_error_response(&mut stream, "Failed to read request");
            return;
        }
    };

    if n == 0 {
        send_error_response(&mut stream, "Empty request");
        return;
    }
    if n >= MAX_REQUEST_BYTES {
        send_error_response(&mut stream, "Request too large");
        return;
    }

    let request = String::from_utf8_lossy(&buffer[..n]);
    let request_line = request.lines().next().unwrap_or("unknown");
    debug(format!("OAuth callback request: {}", request_line));

    let Some((method, target)) = parse_request_line(request_line) else {
        send_error_response(&mut stream, "Malformed request line");
        return;
    };

    if method != "GET" {
        send_error_response(&mut stream, "Unsupported HTTP method");
        return;
    }

    let (path, query) = split_target(target);
    if path != "/callback" {
        send_error_response(&mut stream, "Invalid callback path");
        return;
    }

    let Some(query) = query else {
        send_error_response(&mut stream, "Missing query parameters");
        return;
    };

    let Some(code) = extract_query_param(query, "code") else {
        log_error("OAuth callback missing code parameter");
        send_error_response(&mut stream, "Missing code parameter");
        return;
    };

    let Some(state) = extract_query_param(query, "state") else {
        log_error("OAuth callback missing state parameter");
        send_error_response(&mut stream, "Missing state parameter");
        return;
    };

    if code.len() > MAX_CALLBACK_PARAM_BYTES || state.len() > MAX_CALLBACK_PARAM_BYTES {
        send_error_response(&mut stream, "OAuth callback parameter too large");
        return;
    }

    if !constant_time_eq(&state, expected_state) {
        log_error("OAuth callback state mismatch");
        send_error_response(&mut stream, "State mismatch");
        return;
    }

    if let Ok(mut r) = result.lock() {
        *r = Some(OAuthCallbackData { code, state });
        info("OAuth callback received successfully");
    }

    send_success_response(&mut stream);
}

fn parse_request_line(line: &str) -> Option<(&str, &str)> {
    let mut parts = line.split_whitespace();
    let method = parts.next()?;
    let target = parts.next()?;
    let _version = parts.next()?;
    Some((method, target))
}

fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

fn extract_query_param(query: &str, param: &str) -> Option<String> {
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == param {
            return Some(url_decode(value));
        }
    }
    None
}

fn url_decode(s: &str) -> String {
    let mut bytes = Vec::with_capacity(s.len());
    let input = s.as_bytes();
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'%' if i + 2 < input.len() => {
                let hex = &s[i + 1..i + 3];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    bytes.push(byte);
                    i += 3;
                    continue;
                }
                bytes.push(input[i]);
                i += 1;
            }
            b'+' => {
                bytes.push(b' ');
                i += 1;
            }
            byte => {
                bytes.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&bytes).to_string()
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let max_len = left_bytes.len().max(right_bytes.len());
    let mut diff = (left_bytes.len() ^ right_bytes.len()) as u8;

    for index in 0..max_len {
        let a = *left_bytes.get(index).unwrap_or(&0);
        let b = *right_bytes.get(index).unwrap_or(&0);
        diff |= a ^ b;
    }

    diff == 0
}

fn send_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status,
        body.as_bytes().len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn send_success_response(stream: &mut TcpStream) {
    let html = "<!DOCTYPE html><html><body><h1>Authorization Successful</h1><p>You can close this window and return to OBS.</p></body></html>";
    send_response(stream, "200 OK", html);
}

fn send_error_response(stream: &mut TcpStream, error: &str) {
    let html = format!(
        "<!DOCTYPE html><html><body><h1>Authorization Failed</h1><p>Error: {}</p></body></html>",
        error
    );
    send_response(stream, "400 Bad Request", &html);
}
