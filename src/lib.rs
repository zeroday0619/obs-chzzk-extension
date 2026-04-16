use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};
#[cfg(unix)]
use std::sync::{mpsc, Mutex, OnceLock};
#[cfg(unix)]
use std::thread::{self, JoinHandle};
#[cfg(unix)]
use std::time::Duration;

mod chzzk;
#[cfg(unix)]
mod discord;
mod logging;
#[path = "qt-rs/notification_popup.rs"]
mod notification_popup;
#[cfg(unix)]
mod presence;
#[path = "qt-rs/live_dock.rs"]
mod qt_bridge;
mod settings;

#[cfg(unix)]
use discord::run_presence_worker;
#[cfg(not(unix))]
use logging::{debug, info};
#[cfg(unix)]
use logging::{debug, info, warn};
#[cfg(unix)]
use presence::{build_presence_config, PresenceCommand, PresenceConfig};

const LIBOBS_API_VER: u32 = (32 << 24) | (1 << 16) | 1;

type ObsFrontendEvent = i32;

const OBS_FRONTEND_EVENT_STREAMING_STARTED: ObsFrontendEvent = 1;
const OBS_FRONTEND_EVENT_STREAMING_STOPPING: ObsFrontendEvent = 2;
const OBS_FRONTEND_EVENT_STREAMING_STOPPED: ObsFrontendEvent = 3;
const OBS_FRONTEND_EVENT_EXIT: ObsFrontendEvent = 17;

static OBS_MODULE_POINTER: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
#[cfg(unix)]
static DISCORD_PRESENCE_MANAGER: OnceLock<Mutex<DiscordPresenceManager>> = OnceLock::new();
#[cfg(unix)]
static PRESENCE_START_TOKEN: AtomicU64 = AtomicU64::new(0);

#[cfg(unix)]
const PRESENCE_START_DELAY_SECS: u64 = 20;

fn module_description() -> &'static [u8] {
    b"OBS Chzzk Extension\0"
}

fn module_name() -> &'static [u8] {
    b"obs-chzzk-extension\0"
}

fn module_author() -> &'static [u8] {
    b"zeroday0619\0"
}

#[cfg(unix)]
#[derive(Default)]
struct DiscordPresenceManager {
    worker: Option<PresenceWorker>,
}

#[cfg(unix)]
struct PresenceWorker {
    sender: mpsc::Sender<PresenceCommand>,
    handle: JoinHandle<()>,
}

#[cfg(unix)]
fn manager() -> &'static Mutex<DiscordPresenceManager> {
    DISCORD_PRESENCE_MANAGER.get_or_init(|| Mutex::new(DiscordPresenceManager::default()))
}

fn frontend_event_name(event: ObsFrontendEvent) -> &'static str {
    match event {
        OBS_FRONTEND_EVENT_STREAMING_STARTED => "STREAMING_STARTED",
        OBS_FRONTEND_EVENT_STREAMING_STOPPING => "STREAMING_STOPPING",
        OBS_FRONTEND_EVENT_STREAMING_STOPPED => "STREAMING_STOPPED",
        OBS_FRONTEND_EVENT_EXIT => "EXIT",
        _ => "OTHER",
    }
}

#[cfg(unix)]
fn start_presence_for_current_stream() {
    if !settings::current_settings().discord_presence_enabled {
        info("Discord Rich Presence is disabled in settings; skip scheduling presence start");
        return;
    }

    let token = PRESENCE_START_TOKEN
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    info(format!(
        "presence start scheduled with {}s delay (token={})",
        PRESENCE_START_DELAY_SECS, token
    ));

    thread::spawn(move || {
        thread::sleep(Duration::from_secs(PRESENCE_START_DELAY_SECS));

        if PRESENCE_START_TOKEN.load(Ordering::SeqCst) != token {
            debug(format!(
                "presence start cancelled during delay (stale token={})",
                token
            ));
            return;
        }

        if !settings::current_settings().discord_presence_enabled {
            info("Discord Rich Presence is disabled in settings after delay; skip worker start");
            return;
        }

        let Some(config) = build_presence_config() else {
            warn("presence config build returned None after delay; skip worker start");
            return;
        };

        debug(format!(
            "presence config built after delay: activity='{}', details_len={}, state_len={}, button={}",
            config.activity.name,
            config.activity.details.as_ref().map(|v| v.len()).unwrap_or(0),
            config.activity.state.as_ref().map(|v| v.len()).unwrap_or(0),
            !config.activity.buttons.is_empty()
        ));

        let mut manager = manager().lock().unwrap_or_else(|error| error.into_inner());
        manager.stop_locked();
        manager.start_locked(config);
    });
}

#[cfg(not(unix))]
fn start_presence_for_current_stream() {}

#[cfg(unix)]
pub(crate) fn stop_presence() {
    let token = PRESENCE_START_TOKEN
        .fetch_add(1, Ordering::SeqCst)
        .wrapping_add(1);
    debug(format!(
        "presence stop requested: invalidated pending delayed starts (token={})",
        token
    ));
    let mut manager = manager().lock().unwrap_or_else(|error| error.into_inner());
    manager.stop_locked();
}

#[cfg(not(unix))]
pub(crate) fn stop_presence() {}

#[cfg(unix)]
impl DiscordPresenceManager {
    fn start_locked(&mut self, config: PresenceConfig) {
        let (sender, receiver) = mpsc::channel();
        let application_id = config.application_id.clone();

        info("starting Discord Rich Presence worker thread");

        let handle = thread::spawn(move || run_presence_worker(application_id, config, receiver));

        self.worker = Some(PresenceWorker { sender, handle });
    }

    fn stop_locked(&mut self) {
        if let Some(worker) = self.worker.take() {
            info("stopping Discord Rich Presence worker thread");
            let _ = worker.sender.send(PresenceCommand::Stop);
            let _ = worker.handle.join();
            debug("Discord Rich Presence worker thread joined");
        }
    }
}

unsafe extern "C" fn frontend_event_callback(event: ObsFrontendEvent, _private_data: *mut c_void) {
    debug(format!(
        "received OBS frontend event={} ({})",
        event,
        frontend_event_name(event)
    ));
    match event {
        OBS_FRONTEND_EVENT_STREAMING_STARTED => start_presence_for_current_stream(),
        OBS_FRONTEND_EVENT_STREAMING_STOPPING
        | OBS_FRONTEND_EVENT_STREAMING_STOPPED
        | OBS_FRONTEND_EVENT_EXIT => {
            stop_presence();
        }
        _ => {}
    }
}

#[no_mangle]
pub extern "C" fn obs_module_ver() -> u32 {
    LIBOBS_API_VER
}

#[no_mangle]
pub extern "C" fn obs_module_set_pointer(module: *mut c_void) {
    OBS_MODULE_POINTER.store(module, Ordering::SeqCst);
    debug(format!("OBS module pointer set: {:p}", module));
}

#[no_mangle]
pub extern "C" fn obs_current_module() -> *mut c_void {
    OBS_MODULE_POINTER.load(Ordering::SeqCst)
}

#[no_mangle]
pub extern "C" fn obs_module_name() -> *const c_char {
    module_name().as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn obs_module_description() -> *const c_char {
    module_description().as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn obs_module_author() -> *const c_char {
    module_author().as_ptr().cast()
}

#[no_mangle]
pub extern "C" fn obs_module_load() -> bool {
    info("obs-chzzk-extension: module load");
    settings::initialize_gui_settings();

    unsafe {
        obs_frontend_add_event_callback(Some(frontend_event_callback), core::ptr::null_mut());
    }
    debug("OBS frontend callback registered");

    true
}

#[no_mangle]
pub extern "C" fn obs_module_unload() -> bool {
    info("obs-chzzk-extension: module unload");
    stop_presence();
    settings::shutdown_gui_settings();

    unsafe {
        obs_frontend_remove_event_callback(Some(frontend_event_callback), core::ptr::null_mut());
    }
    debug("OBS frontend callback removed");

    true
}

#[allow(improper_ctypes)]
extern "C" {
    fn obs_frontend_add_event_callback(
        callback: Option<unsafe extern "C" fn(ObsFrontendEvent, *mut c_void)>,
        private_data: *mut c_void,
    );

    fn obs_frontend_remove_event_callback(
        callback: Option<unsafe extern "C" fn(ObsFrontendEvent, *mut c_void)>,
        private_data: *mut c_void,
    );
}
