use std::env;
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

use crate::presence::{
    build_clear_activity_payload, build_handshake_payload, build_set_activity_payload, PresenceCommand,
    PresenceConfig,
};
use crate::logging::{debug, error as log_error, info};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
enum DiscordIpcOpcode {
    Handshake = 0,
    Frame = 1,
    Close = 2,
    Ping = 3,
    Pong = 4,
}

impl DiscordIpcOpcode {
    fn from_u32(value: u32) -> Option<Self> {
        match value {
            0 => Some(Self::Handshake),
            1 => Some(Self::Frame),
            2 => Some(Self::Close),
            3 => Some(Self::Ping),
            4 => Some(Self::Pong),
            _ => None,
        }
    }
}

enum IpcEvent {
    Ping(Vec<u8>),
    Close,
    RpcError(String),
    Other,
}

struct DiscordIpcClient {
    stream: UnixStream,
}

fn ipc_socket_candidates() -> Vec<PathBuf> {
    let mut prefixes = Vec::new();

    for variable in ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"] {
        if let Ok(value) = env::var(variable) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                prefixes.push(PathBuf::from(trimmed));
            }
        }
    }

    prefixes.push(PathBuf::from("/tmp"));

    let mut candidates = Vec::new();
    for prefix in prefixes {
        for index in 0..10 {
            candidates.push(prefix.join(format!("discord-ipc-{}", index)));
        }
    }

    debug(format!("Discord IPC candidate count={}", candidates.len()));

    candidates
}

fn connect_to_discord() -> io::Result<UnixStream> {
    let mut last_error: Option<io::Error> = None;

    for candidate in ipc_socket_candidates() {
        debug(format!("trying Discord IPC socket: {}", candidate.display()));
        match UnixStream::connect(&candidate) {
            Ok(stream) => {
                info(format!("connected to Discord IPC socket: {}", candidate.display()));
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| io::Error::new(io::ErrorKind::NotFound, "discord ipc socket not found")))
}

fn write_ipc_frame(stream: &mut UnixStream, opcode: DiscordIpcOpcode, payload: &[u8]) -> io::Result<()> {
    let payload_bytes = payload;
    let mut header = [0u8; 8];

    header[0..4].copy_from_slice(&(opcode as u32).to_le_bytes());
    header[4..8].copy_from_slice(&(payload_bytes.len() as u32).to_le_bytes());

    debug(format!(
        "Discord IPC write frame: opcode={}, payload_len={}, payload_preview={}",
        opcode as u32,
        payload_bytes.len(),
        preview_payload(&String::from_utf8_lossy(payload_bytes))
    ));

    stream.write_all(&header)?;
    stream.write_all(payload_bytes)?;
    stream.flush()
}

fn read_ipc_frame(stream: &mut UnixStream) -> io::Result<(DiscordIpcOpcode, Vec<u8>)> {
    let mut header = [0u8; 8];
    stream.read_exact(&mut header)?;

    let opcode_raw = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
    let opcode = DiscordIpcOpcode::from_u32(opcode_raw)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("unknown opcode: {}", opcode_raw)))?;
    let length = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;

    let mut payload = vec![0u8; length];
    if length > 0 {
        stream.read_exact(&mut payload)?;
    }

    let payload_preview = String::from_utf8_lossy(&payload);
    debug(format!(
        "Discord IPC read frame: opcode={}, payload_len={}, payload_preview={}",
        opcode as u32,
        length,
        preview_payload(&payload_preview)
    ));

    Ok((opcode, payload))
}

fn preview_payload(payload: &str) -> String {
    payload.replace('\n', "\\n")
}

fn parse_json_payload(payload: &[u8]) -> Option<Value> {
    serde_json::from_slice(payload).ok()
}

fn extract_rpc_error_message(payload: &[u8]) -> Option<String> {
    let json = parse_json_payload(payload)?;
    let evt = json.get("evt").and_then(Value::as_str);
    if evt != Some("ERROR") {
        return None;
    }

    let message = json
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("discord rpc returned ERROR event");

    Some(message.to_string())
}

impl DiscordIpcClient {
    fn connect() -> io::Result<Self> {
        let stream = connect_to_discord()?;
        stream.set_read_timeout(Some(Duration::from_millis(500)))?;
        stream.set_write_timeout(Some(Duration::from_secs(2)))?;
        Ok(Self { stream })
    }

    fn handshake(&mut self, application_id: &str) -> io::Result<()> {
        let payload = build_handshake_payload(application_id);
        write_ipc_frame(&mut self.stream, DiscordIpcOpcode::Handshake, payload.as_bytes())?;
        debug("Discord Rich Presence handshake request sent");

        let (opcode, payload) = read_ipc_frame(&mut self.stream)?;
        if opcode != DiscordIpcOpcode::Frame {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unexpected handshake opcode: {}", opcode as u32),
            ));
        }

        if let Some(message) = extract_rpc_error_message(&payload) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("handshake error: {}", message),
            ));
        }

        if let Some(json) = parse_json_payload(&payload) {
            let cmd = json.get("cmd").and_then(Value::as_str).unwrap_or_default();
            let evt = json.get("evt").and_then(Value::as_str).unwrap_or_default();
            debug(format!("Discord RPC handshake frame received: cmd='{}', evt='{}'", cmd, evt));
        }

        info("Discord Rich Presence handshake established");
        Ok(())
    }

    fn set_activity(&mut self, payload: &str) -> io::Result<()> {
        write_ipc_frame(&mut self.stream, DiscordIpcOpcode::Frame, payload.as_bytes())
    }

    fn clear_activity(&mut self) {
        let _ = self.set_activity(&build_clear_activity_payload());
    }

    fn next_event(&mut self) -> io::Result<IpcEvent> {
        let (opcode, payload) = read_ipc_frame(&mut self.stream)?;
        match opcode {
            DiscordIpcOpcode::Ping => Ok(IpcEvent::Ping(payload)),
            DiscordIpcOpcode::Close => Ok(IpcEvent::Close),
            DiscordIpcOpcode::Frame => {
                if let Some(message) = extract_rpc_error_message(&payload) {
                    return Ok(IpcEvent::RpcError(message));
                }

                if let Some(json) = parse_json_payload(&payload) {
                    let cmd = json.get("cmd").and_then(Value::as_str).unwrap_or_default();
                    let evt = json.get("evt").and_then(Value::as_str).unwrap_or_default();
                    debug(format!("Discord RPC frame received: cmd='{}', evt='{}'", cmd, evt));
                }

                Ok(IpcEvent::Other)
            }
            _ => Ok(IpcEvent::Other),
        }
    }

    fn send_pong(&mut self, payload: &[u8]) -> io::Result<()> {
        write_ipc_frame(&mut self.stream, DiscordIpcOpcode::Pong, payload)
    }
}

pub(crate) fn run_presence_worker(
    application_id: String,
    initial_config: PresenceConfig,
    receiver: mpsc::Receiver<PresenceCommand>,
) {
    info(format!(
        "presence worker started: app_id={}, activity='{}'",
        application_id, initial_config.activity.name
    ));

    let mut client = match DiscordIpcClient::connect() {
        Ok(client) => client,
        Err(error) => {
            log_error(format!("Discord Rich Presence connection failed: {}", error));
            return;
        }
    };

    if let Err(error) = client.handshake(&application_id) {
        log_error(format!("Discord Rich Presence handshake failed: {}", error));
        return;
    }

    if let Err(error) = client.set_activity(&build_set_activity_payload(&initial_config)) {
        log_error(format!("Discord Rich Presence activity update failed: {}", error));
        return;
    }
    info("Discord Rich Presence activity set");

    loop {
        match receiver.try_recv() {
            Ok(PresenceCommand::Stop) => {
                info("presence worker received stop command");
                client.clear_activity();
                break;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                info("presence worker channel disconnected");
                client.clear_activity();
                break;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }

        match client.next_event() {
            Ok(IpcEvent::Ping(payload)) => {
                debug("Discord IPC ping received");
                if let Err(error) = client.send_pong(&payload) {
                    log_error(format!("Discord Rich Presence pong failed: {}", error));
                    break;
                }
                debug("Discord IPC pong sent");
            }
            Ok(IpcEvent::Close) => {
                warn_close();
                break;
            }
            Ok(IpcEvent::RpcError(message)) => {
                log_error(format!("Discord RPC error event: {}", message));
                break;
            }
            Ok(IpcEvent::Other) => {}
            Err(error) if error.kind() == io::ErrorKind::WouldBlock || error.kind() == io::ErrorKind::TimedOut => {}
            Err(error) => {
                log_error(format!("Discord Rich Presence read failed: {}", error));
                break;
            }
        }
    }

    info("presence worker exited");
}

fn warn_close() {
    info("Discord IPC close opcode received");
}
