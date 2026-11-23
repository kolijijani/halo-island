//! Master volume change notifications (Core Audio).
//!
//! A dedicated MTA thread owns the `IMMDeviceEnumerator` and the
//! `IAudioEndpointVolume` of the default render endpoint (console role).
//! Volume changes arrive on an audio-engine thread through
//! [`VolumeCallback::OnNotify`], which only takes a short lock, drops
//! duplicates and stale endpoints, and posts `AppEvent::Volume`.
//! Default-device changes arrive through [`DeviceClient`], which only wakes
//! the worker; the worker re-binds to the new endpoint (Core Audio forbids
//! (un)registering from inside its own callbacks) and reports its state.
//!
//! Idle cost: the worker sleeps in a blocking `recv`; nothing polls.

use crate::events::{AppEvent, EventSink};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::core::{implement, Result, PCWSTR};
use windows::Win32::Foundation::E_FAIL;
use windows::Win32::Media::Audio::Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl};
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, eConsole, eMultimedia, eRender, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, AUDCLNT_E_DEVICE_INVALIDATED, AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

/// How long `Drop` waits for the worker to unregister before detaching it.
const STOP_TIMEOUT: Duration = Duration::from_millis(800);

/// Master-volume watcher. Dropping it unregisters every callback.
pub struct AudioService {
    tx: Sender<Msg>,
    done: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

impl AudioService {
    pub fn start(sink: EventSink, watch_mic: bool) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        let client_tx = tx.clone();
        let thread = std::thread::Builder::new()
            .name("halo-audio".into())
            .spawn(move || {
                worker(sink, rx, client_tx, watch_mic);
                let _ = done_tx.send(());
            })
            .map_err(|e| windows::core::Error::new(E_FAIL, format!("audio thread: {e}")))?;
        Ok(Self { tx, done, thread: Some(thread) })
    }

    /// Nudge master volume by `delta` (-1..=1); unmutes if raising while muted.
    pub fn adjust_volume(&self, delta: f32) {
        let _ = self.tx.send(Msg::AdjustVolume(delta));
    }

    /// Enable or disable microphone mute notifications (eCapture endpoint).
    pub fn set_mic_watch(&self, on: bool) {
        let _ = self.tx.send(Msg::SetMicWatch(on));
    }
}

impl Drop for AudioService {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Shutdown);
        let Some(thread) = self.thread.take() else { return };
        match self.done.recv_timeout(STOP_TIMEOUT) {
            // Finished (or died): joining is immediate.
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let _ = thread.join();
            }
            Err(RecvTimeoutError::Timeout) => crate::pwarn!("audio: worker did not stop in time; detaching it"),
        }
    }
}

enum Msg {
    /// The default render endpoint (console or multimedia role) changed.
    DefaultChanged,
    /// The default capture (microphone) endpoint changed.
    MicChanged,
    /// Nudge master output volume by `delta` (-1..=1).
    AdjustVolume(f32),
    /// Turn microphone mute notifications on or off.
    SetMicWatch(bool),
    Shutdown,
}

// ---------------------------------------------------------------------------
// Publishing: dedupe + stale-endpoint filter, shared with the COM callbacks.

struct GateState {
    /// Bumped on every re-bind; callbacks of older endpoints are ignored.
    generation: u32,
    /// Last reported (level in 1/10000 steps, muted).
    last: Option<(u32, bool)>,
    /// Last reported microphone mute state.
    last_mic: Option<bool>,
}

struct Gate {
    sink: EventSink,
    state: Mutex<GateState>,
}

impl Gate {
    fn lock(&self) -> MutexGuard<'_, GateState> {
        // A poisoned lock only means another thread panicked mid-update; the
        // plain fields are still meaningful.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Start a new generation (before binding a new endpoint).
    fn next_generation(&self) -> u32 {
        let mut s = self.lock();
        s.generation = s.generation.wrapping_add(1);
        s.generation
    }

    /// Report `level`/`muted` unless it comes from a stale endpoint or
    /// repeats the last report. Cheap: one lock, no allocation on the
    /// duplicate path. Holding the lock while sending keeps reports ordered
    /// with respect to re-binds.
    fn publish(&self, generation: u32, level: f32, muted: bool) {
        let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 0.0 };
        let key = ((level * 10_000.0).round() as u32, muted);
        let mut s = self.lock();
        if s.generation != generation || s.last == Some(key) {
            return;
        }
        s.last = Some(key);
        self.sink.send(AppEvent::Volume { level, muted });
    }

    /// Report microphone mute state unless stale or duplicate.
    fn publish_mic(&self, generation: u32, muted: bool) {
        let mut s = self.lock();
        if s.generation != generation || s.last_mic == Some(muted) {
            return;
        }
        s.last_mic = Some(muted);
        self.sink.send(AppEvent::Mic { muted });
    }

    fn reset_mic(&self) {
        let mut s = self.lock();
        s.generation = s.generation.wrapping_add(1);
        s.last_mic = None;
    }
}

// ---------------------------------------------------------------------------
// COM callbacks.

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    gate: Arc<Gate>,
    generation: u32,
    is_mic: bool,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> Result<()> {
        // SAFETY: Core Audio passes either null or a struct that is valid for
        // the duration of this call; we only read two fields.
        if let Some(data) = unsafe { pnotify.as_ref() } {
            if self.is_mic {
                self.gate.publish_mic(self.generation, data.bMuted.as_bool());
            } else {
                self.gate.publish(self.generation, data.fMasterVolume, data.bMuted.as_bool());
            }
        }
        Ok(())
    }
}

#[implement(IMMNotificationClient)]
struct DeviceClient {
    tx: Sender<Msg>,
}

impl IMMNotificationClient_Impl for DeviceClient_Impl {
    fn OnDeviceStateChanged(&self, _id: &PCWSTR, _state: DEVICE_STATE) -> Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _id: &PCWSTR) -> Result<()> {
        Ok(())
    }

    fn OnDefaultDeviceChanged(&self, flow: EDataFlow, role: ERole, _id: &PCWSTR) -> Result<()> {
        // Never touch Core Audio from here: just wake the worker.
        if flow == eRender && (role == eConsole || role == eMultimedia) {
            let _ = self.tx.send(Msg::DefaultChanged);
        } else if flow == eCapture && (role == eConsole || role == eCommunications) {
            let _ = self.tx.send(Msg::MicChanged);
        }
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Worker thread.

/// COM apartment membership for the current thread.
struct ComApartment {
    initialized: bool,
}

impl ComApartment {
    fn enter_mta() -> Self {
        // S_OK and S_FALSE both need a matching CoUninitialize; a failure
        // (e.g. RPC_E_CHANGED_MODE) must not be balanced.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_err() {
            crate::pwarn!("audio: CoInitializeEx failed ({hr:?})");
        }
        Self { initialized: hr.is_ok() }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { CoUninitialize() };
        }
    }
}

/// The endpoint we listen to. The callback object stays alive (and
/// registered) for exactly as long as this binding.
struct Binding {
    id: String,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Binding {
    fn unbind(self) {
        match unsafe { self.volume.UnregisterControlChangeNotify(&self.callback) } {
            // An unplugged endpoint is already gone, together with its registration.
            Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => {}
            Err(e) => crate::pwarn!("audio: UnregisterControlChangeNotify failed ({e})"),
            Ok(()) => {}
        }
    }
}

fn worker(sink: EventSink, rx: Receiver<Msg>, client_tx: Sender<Msg>, mut watch_mic: bool) {
    // Declared first so it is dropped last, after every COM object below.
    let com = ComApartment::enter_mta();
    if !com.initialized {
        return;
    }

    let enumerator: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
        Ok(e) => e,
        Err(e) => {
            crate::pwarn!("audio: MMDeviceEnumerator unavailable ({e}); volume pop-ups disabled");
            return;
        }
    };

    let client: IMMNotificationClient = DeviceClient { tx: client_tx }.into();
    let client_registered = match unsafe { enumerator.RegisterEndpointNotificationCallback(&client) } {
        Ok(()) => true,
        Err(e) => {
            crate::pwarn!("audio: default-device notifications unavailable ({e})");
            false
        }
    };

    let gate = Arc::new(Gate { sink: sink.clone(), state: Mutex::new(GateState { generation: 0, last: None, last_mic: None }) });
    let mic_gate = Arc::new(Gate { sink, state: Mutex::new(GateState { generation: 0, last: None, last_mic: None }) });
    // The first report doubles as the initial state, which the app shows silently.
    let mut binding = rebind(&enumerator, &gate, None, false);
    let mut mic_binding = if watch_mic { rebind(&enumerator, &mic_gate, None, true) } else { None };

    while let Ok(msg) = rx.recv() {
        match msg {
            Msg::Shutdown => break,
            Msg::DefaultChanged => {
                binding = rebind(&enumerator, &gate, binding, false);
            }
            Msg::MicChanged => {
                if watch_mic {
                    mic_binding = rebind(&enumerator, &mic_gate, mic_binding, true);
                }
            }
            Msg::SetMicWatch(on) => {
                if on != watch_mic {
                    watch_mic = on;
                    if watch_mic {
                        mic_binding = rebind(&enumerator, &mic_gate, mic_binding, true);
                    } else {
                        mic_gate.reset_mic();
                        if let Some(b) = mic_binding.take() {
                            b.unbind();
                        }
                    }
                }
            }
            Msg::AdjustVolume(delta) => {
                if let Some(b) = &binding {
                    if let Ok(cur) = unsafe { b.volume.GetMasterVolumeLevelScalar() } {
                        let next = (cur + delta).clamp(0.0, 1.0);
                        unsafe {
                            if delta > 0.0 {
                                let _ = b.volume.SetMute(false, std::ptr::null());
                            }
                            let _ = b.volume.SetMasterVolumeLevelScalar(next, std::ptr::null());
                        }
                    }
                }
            }
        }
    }

    if let Some(b) = binding {
        b.unbind();
    }
    if let Some(b) = mic_binding {
        b.unbind();
    }
    if client_registered {
        if let Err(e) = unsafe { enumerator.UnregisterEndpointNotificationCallback(&client) } {
            crate::pwarn!("audio: UnregisterEndpointNotificationCallback failed ({e})");
        }
    }
    // `client`, `enumerator`, then `com` drop here, in that order.
}

/// Bind to the current default endpoint (unless it is the one already bound)
/// and report its state.
fn rebind(enumerator: &IMMDeviceEnumerator, gate: &Arc<Gate>, current: Option<Binding>, is_mic: bool) -> Option<Binding> {
    let flow = if is_mic { eCapture } else { eRender };
    let device = match unsafe { enumerator.GetDefaultAudioEndpoint(flow, eConsole) } {
        Ok(d) => d,
        Err(e) => {
            if !is_mic {
                crate::pinfo!("audio: no default output device ({e})");
            }
            gate.next_generation();
            if let Some(b) = current {
                b.unbind();
            }
            return None;
        }
    };
    let id = device_id(&device);
    if let Some(b) = &current {
        // Same endpoint (a per-role repeat of one switch) and still alive.
        if !id.is_empty() && b.id == id && unsafe { b.volume.GetMute() }.is_ok() {
            return current;
        }
    }

    // A new generation silences the old endpoint's late callbacks.
    let generation = gate.next_generation();
    if let Some(b) = current {
        b.unbind();
    }
    match bind(&device, id, gate, generation, is_mic) {
        Ok(b) => {
            if is_mic {
                if let Ok(muted) = unsafe { b.volume.GetMute() } {
                    gate.publish_mic(generation, muted.as_bool());
                }
            } else {
                match unsafe { (b.volume.GetMasterVolumeLevelScalar(), b.volume.GetMute()) } {
                    (Ok(level), Ok(muted)) => gate.publish(generation, level, muted.as_bool()),
                    (Err(e), _) | (_, Err(e)) => crate::pwarn!("audio: reading the volume failed ({e})"),
                }
            }
            Some(b)
        }
        Err(e) => {
            if !is_mic {
                crate::pwarn!("audio: binding the default output device failed ({e})");
            }
            None
        }
    }
}

fn bind(device: &IMMDevice, id: String, gate: &Arc<Gate>, generation: u32, is_mic: bool) -> Result<Binding> {
    let volume: IAudioEndpointVolume = unsafe { device.Activate(CLSCTX_ALL, None)? };
    let callback: IAudioEndpointVolumeCallback = VolumeCallback { gate: Arc::clone(gate), generation, is_mic }.into();
    unsafe { volume.RegisterControlChangeNotify(&callback)? };
    Ok(Binding { id, volume, callback })
}

/// The endpoint id string ("" if it cannot be read).
fn device_id(device: &IMMDevice) -> String {
    match unsafe { device.GetId() } {
        Ok(p) if !p.is_null() => {
            // SAFETY: GetId returns a NUL-terminated, CoTaskMemAlloc'ed string
            // that we own and free exactly once.
            let id = unsafe { p.to_string() }.unwrap_or_default();
            unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
            id
        }
        _ => String::new(),
    }
}
