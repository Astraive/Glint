//! Core Audio: `IMMDeviceEnumerator` -> default render endpoint (eRender,
//! eConsole) -> `IAudioEndpointVolume` (level, mute), with
//! `IAudioEndpointVolumeCallback` for volume changes and
//! `IMMNotificationClient` for device / default-device changes.
//!
//! The write path is only exercised by writing back the value just read
//! (with a spike-owned event-context GUID), then re-reading and restoring.

use super::{CapabilityRow, Com, Recorder, perr, pump_messages, result_json, thread_id, timed};
use crate::model::{self, Availability, ErrorKind, ProviderError};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    AUDIO_VOLUME_NOTIFICATION_DATA, DEVICE_STATE, DEVICE_STATE_DISABLED, DEVICE_STATE_NOTPRESENT,
    DEVICE_STATE_UNPLUGGED, EDataFlow, ERole, IMMDevice, IMMDeviceEnumerator,
    IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator, eCapture,
    eCommunications, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::core::{GUID, PCWSTR, implement};

/// Event context passed to every spike write so callbacks can tell our own
/// writes apart from the user's.
const SPIKE_CONTEXT: GUID = GUID::from_u128(0x5c1e_0010_6c69_6e74_0000_7370_6b30_3130);

const VOLUME_OBSERVE: &str = "audio.volume.observe";
const DEVICE_OBSERVE: &str = "audio.device.observe";
pub const OBSERVE_LABELS: &[&str] = &[VOLUME_OBSERVE, DEVICE_OBSERVE];

const CALLBACK_TIMEOUT: Duration = Duration::from_millis(1500);

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    label: &'static str,
    rec: Arc<Recorder>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        let detail = if data.is_null() {
            Value::Null
        } else {
            // SAFETY: Core Audio passes a valid notification for the call.
            let d = unsafe { &*data };
            json!({
                "level": round3(d.fMasterVolume),
                "muted": d.bMuted.as_bool(),
                "channels": d.nChannels,
                "own_write": d.guidEventContext == SPIKE_CONTEXT,
                "context_is_null": d.guidEventContext == GUID::zeroed(),
            })
        };
        self.rec.record(self.label, detail);
        Ok(())
    }
}

fn volume_callback(label: &'static str, rec: &Arc<Recorder>) -> IAudioEndpointVolumeCallback {
    VolumeCallback {
        label,
        rec: Arc::clone(rec),
    }
    .into()
}

#[implement(IMMNotificationClient)]
struct DeviceCallback {
    label: &'static str,
    rec: Arc<Recorder>,
}

fn id_tag(id: &PCWSTR) -> Value {
    if id.is_null() {
        return Value::Null;
    }
    // SAFETY: Core Audio passes a valid NUL-terminated string or null.
    match unsafe { id.to_string() } {
        Ok(text) => json!(model::fingerprint(&text)),
        Err(_) => json!("invalid-utf16"),
    }
}

impl IMMNotificationClient_Impl for DeviceCallback_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, state: DEVICE_STATE) -> windows::core::Result<()> {
        self.rec.record(
            self.label,
            json!({ "event": "state-changed", "device": id_tag(id), "state": state.0 }),
        );
        Ok(())
    }

    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.rec.record(
            self.label,
            json!({ "event": "added", "device": id_tag(id) }),
        );
        Ok(())
    }

    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.rec.record(
            self.label,
            json!({ "event": "removed", "device": id_tag(id) }),
        );
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        id: &PCWSTR,
    ) -> windows::core::Result<()> {
        // `id` is null when the last device of that flow/role disappeared.
        self.rec.record(
            self.label,
            json!({ "event": "default-changed", "flow": flow.0, "role": role.0, "device": id_tag(id) }),
        );
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        id: &PCWSTR,
        _key: &windows::Win32::Foundation::PROPERTYKEY,
    ) -> windows::core::Result<()> {
        self.rec.record(
            self.label,
            json!({ "event": "property-changed", "device": id_tag(id) }),
        );
        Ok(())
    }
}

fn round3(x: f32) -> f64 {
    (f64::from(x) * 1000.0).round() / 1000.0
}

pub fn create_enumerator_result() -> Result<IMMDeviceEnumerator, ProviderError> {
    // SAFETY: plain COM activation; fails with CO_E_NOTINITIALIZED off-apartment.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| perr("CoCreateInstance(MMDeviceEnumerator)", &e))
}

fn device_id(device: &IMMDevice) -> Result<String, ProviderError> {
    // SAFETY: GetId returns a CoTaskMemAlloc'd string that we free below.
    unsafe {
        let raw = device.GetId().map_err(|e| perr("IMMDevice::GetId", &e))?;
        let text = raw.to_string().unwrap_or_default();
        CoTaskMemFree(Some(raw.0 as *const _));
        Ok(text)
    }
}

/// One opened default render endpoint.
struct Endpoint {
    enumerator: IMMDeviceEnumerator,
    volume: IAudioEndpointVolume,
    id: String,
}

fn open_default() -> Result<Endpoint, ProviderError> {
    let enumerator = create_enumerator_result()?;
    // SAFETY: valid enumerator; E_NOTFOUND when there is no render endpoint.
    let device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|e| perr("GetDefaultAudioEndpoint(eRender, eConsole)", &e))?;
    let id = device_id(&device)?;
    // SAFETY: valid device; activation of the endpoint volume interface.
    let volume = unsafe { device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None) }
        .map_err(|e| perr("IMMDevice::Activate(IAudioEndpointVolume)", &e))?;
    Ok(Endpoint {
        enumerator,
        volume,
        id,
    })
}

fn read(volume: &IAudioEndpointVolume) -> Result<(f32, bool), ProviderError> {
    // SAFETY: valid interface pointer.
    unsafe {
        let level = volume
            .GetMasterVolumeLevelScalar()
            .map_err(|e| perr("GetMasterVolumeLevelScalar", &e))?;
        let muted = volume.GetMute().map_err(|e| perr("GetMute", &e))?;
        Ok((level, muted.as_bool()))
    }
}

pub fn read_default_volume_result() -> Result<(f32, bool), ProviderError> {
    read(&open_default()?.volume)
}

/// Set once from `--nudge` before any probe runs.
static NUDGE: AtomicBool = AtomicBool::new(false);

pub fn set_nudge(enabled: bool) {
    NUDGE.store(enabled, Ordering::SeqCst);
}

/// Exercises the write path with the spike context GUID.
///
/// Default mode writes back the level and mute just read (no audible change;
/// Windows sends no `OnNotify` for it). `--nudge` mode moves the level by
/// 0.01 scalar, waits for `wait_label` (if any) to see the write, and puts
/// the original level back straight away. Both modes re-read afterwards and
/// restore if the value moved.
fn same_value_write(
    volume: &IAudioEndpointVolume,
    rec: &Recorder,
    wait_label: Option<&str>,
) -> Value {
    let (level, muted) = match read(volume) {
        Ok(v) => v,
        Err(e) => return json!({ "read": e.to_json() }),
    };
    let nudge = NUDGE.load(Ordering::SeqCst);
    let target = match (nudge, level >= 0.02) {
        (false, _) => level,
        (true, true) => level - 0.01,
        (true, false) => level + 0.01,
    };
    let since = rec.now_ms();
    let (write, write_ms) = timed(|| {
        // SAFETY: valid interface; our own context GUID.
        unsafe {
            volume
                .SetMasterVolumeLevelScalar(target, &SPIKE_CONTEXT)
                .map_err(|e| perr("SetMasterVolumeLevelScalar", &e))?;
            if !nudge {
                volume
                    .SetMute(muted, &SPIKE_CONTEXT)
                    .map_err(|e| perr("SetMute", &e))?;
            }
            Ok::<(), ProviderError>(())
        }
    });
    let callback = wait_label.map(|label| {
        match rec.wait_for(label, since, CALLBACK_TIMEOUT, |d| d["own_write"] == true) {
            Some(t) => json!({ "observed": true, "latency_ms": super::round(t - since) }),
            None => json!({
                "observed": false,
                "error": ProviderError::new(ErrorKind::Timeout, "IAudioEndpointVolumeCallback::OnNotify",
                    format!("no callback for our write within {} ms", CALLBACK_TIMEOUT.as_millis())).to_json(),
            }),
        }
    });
    let mut nudged_ms = None;
    if nudge {
        // SAFETY: as above; puts the original level back.
        let _ = unsafe { volume.SetMasterVolumeLevelScalar(level, &SPIKE_CONTEXT) };
        nudged_ms = Some(super::round(rec.now_ms() - since));
    }
    let after = read(volume);
    let mut restored = false;
    if let Ok((l2, m2)) = after
        && (l2 != level || m2 != muted)
    {
        // SAFETY: as above; puts the original values back.
        unsafe {
            let _ = volume.SetMasterVolumeLevelScalar(level, &SPIKE_CONTEXT);
            let _ = volume.SetMute(muted, &SPIKE_CONTEXT);
        }
        restored = true;
    }
    json!({
        "mode": if nudge { "nudge-0.01" } else { "same-value" },
        "level": round3(level),
        "muted": muted,
        "target": round3(target),
        "level_off_original_ms": nudged_ms,
        "write": result_json(&write),
        "write_ms": write_ms,
        "callback": callback,
        "unchanged_after": after.as_ref().map(|(l2, m2)| *l2 == level && *m2 == muted).ok(),
        "restored": restored,
    })
}

pub fn same_value_write_default(rec: &Recorder) -> Value {
    match open_default() {
        Ok(ep) => same_value_write(&ep.volume, rec, None),
        Err(e) => json!({ "open": e.to_json() }),
    }
}

/// Main-thread session: snapshot plus long-lived subscriptions.
pub struct Session {
    endpoint: Result<Endpoint, ProviderError>,
    open_ms: f64,
    snapshot: Value,
    read: Result<(f32, bool), ProviderError>,
    volume_cb: Option<IAudioEndpointVolumeCallback>,
    device_cb: Option<IMMNotificationClient>,
    register_volume: Result<(), ProviderError>,
    register_device: Result<(), ProviderError>,
    write: Option<Value>,
}

impl Session {
    pub fn start(rec: &Arc<Recorder>) -> Self {
        let (endpoint, open_ms) = timed(open_default);
        let ep = match &endpoint {
            Ok(ep) => ep,
            Err(e) => {
                return Self {
                    read: Err(e.clone()),
                    register_volume: Err(e.clone()),
                    register_device: Err(e.clone()),
                    endpoint,
                    open_ms,
                    snapshot: Value::Null,
                    volume_cb: None,
                    device_cb: None,
                    write: None,
                };
            }
        };
        let (read_result, read_ms) = timed(|| read(&ep.volume));
        let snapshot = snapshot(ep, read_ms);

        let volume_cb = volume_callback(VOLUME_OBSERVE, rec);
        // SAFETY: valid interfaces; the callback stays referenced by `self`.
        let register_volume = unsafe { ep.volume.RegisterControlChangeNotify(&volume_cb) }
            .map_err(|e| perr("RegisterControlChangeNotify", &e));

        let device_cb: IMMNotificationClient = DeviceCallback {
            label: DEVICE_OBSERVE,
            rec: Arc::clone(rec),
        }
        .into();
        // SAFETY: as above.
        let register_device = unsafe {
            ep.enumerator
                .RegisterEndpointNotificationCallback(&device_cb)
        }
        .map_err(|e| perr("RegisterEndpointNotificationCallback", &e));
        Self {
            endpoint,
            open_ms,
            snapshot,
            read: read_result,
            volume_cb: Some(volume_cb),
            device_cb: Some(device_cb),
            register_volume,
            register_device,
            write: None,
        }
    }

    pub fn exercise_write(&mut self, rec: &Recorder) {
        if let Ok(ep) = &self.endpoint {
            self.write = Some(same_value_write(&ep.volume, rec, Some(VOLUME_OBSERVE)));
        }
    }

    pub fn stop(&mut self, rec: &Recorder) -> Value {
        let Ok(ep) = &self.endpoint else {
            return Value::Null;
        };
        let at = rec.now_ms();
        let mut out = json!({ "at_ms": super::round(at) });
        if let Some(cb) = &self.volume_cb {
            // SAFETY: unregistering the callback registered in `start`.
            let first = unsafe { ep.volume.UnregisterControlChangeNotify(cb) }
                .map_err(|e| perr("UnregisterControlChangeNotify", &e));
            // SAFETY: second unregister of the same callback.
            let second = unsafe { ep.volume.UnregisterControlChangeNotify(cb) }
                .map_err(|e| perr("UnregisterControlChangeNotify (again)", &e));
            out["volume_unregister"] = result_json(&first);
            out["volume_unregister_again"] = result_json(&second);
        }
        if let Some(cb) = &self.device_cb {
            // SAFETY: as above, for the notification client.
            let first = unsafe { ep.enumerator.UnregisterEndpointNotificationCallback(cb) }
                .map_err(|e| perr("UnregisterEndpointNotificationCallback", &e));
            // SAFETY: second unregister.
            let second = unsafe { ep.enumerator.UnregisterEndpointNotificationCallback(cb) }
                .map_err(|e| perr("UnregisterEndpointNotificationCallback (again)", &e));
            out["device_unregister"] = result_json(&first);
            out["device_unregister_again"] = result_json(&second);
        }
        out
    }

    pub fn report(&self, stop: Value) -> Value {
        json!({
            "api": "Core Audio (Win32 COM): IMMDeviceEnumerator, IAudioEndpointVolume, IAudioEndpointVolumeCallback, IMMNotificationClient",
            "open_default_endpoint": result_json(&self.endpoint),
            "open_ms": self.open_ms,
            "snapshot": self.snapshot,
            "register_volume_callback": result_json(&self.register_volume),
            "register_device_callback": result_json(&self.register_device),
            "same_value_write": self.write,
            "stop": stop,
        })
    }

    pub fn capabilities(&self, rec: &Recorder) -> Vec<CapabilityRow> {
        let read_row = match (&self.endpoint, &self.read) {
            (Err(e), _) | (Ok(_), Err(e)) => {
                CapabilityRow::from_error("system.audio.read", e.clone(), "default render endpoint")
            }
            (Ok(_), Ok((level, muted))) => CapabilityRow::new(
                "system.audio.read",
                Ok(Availability::Supported),
                format!("level {:.3}, muted {muted}", level),
            ),
        };
        let write_ok = self.write.as_ref().map(|w| w["write"]["ok"] == true);
        let write_row = match (&read_row.status, &self.write) {
            (Ok(Availability::Supported), Some(w)) if write_ok == Some(true) => CapabilityRow::new(
                "system.audio.write",
                Ok(Availability::Supported),
                format!(
                    "SetMasterVolumeLevelScalar accepted ({} mode); original value back: {}",
                    w["mode"].as_str().unwrap_or("?"),
                    w["unchanged_after"] == true || w["restored"] == true
                ),
            ),
            (Ok(Availability::Supported), Some(w)) => {
                let kind = w["write"]["error"]["kind"].as_str().unwrap_or("failure");
                let status = match kind {
                    "permission-denied" => Ok(Availability::permission_required(
                        "endpoint volume write denied",
                    )),
                    "unavailable" | "unsupported" => {
                        Ok(Availability::unavailable("endpoint volume write rejected"))
                    }
                    _ => Err(ProviderError::new(
                        ErrorKind::Failure,
                        "SetMasterVolumeLevelScalar",
                        w["write"].to_string(),
                    )),
                };
                CapabilityRow::new("system.audio.write", status, "same-value write")
            }
            (status, _) => CapabilityRow::new(
                "system.audio.write",
                status.clone(),
                "follows system.audio.read (no endpoint to write)",
            ),
        };
        let events = match (&read_row.status, &self.register_volume) {
            (Ok(Availability::Supported), Ok(())) => {
                let seen = self
                    .write
                    .as_ref()
                    .map(|w| w["callback"]["observed"] == true)
                    == Some(true);
                CapabilityRow::new(
                    "system.audio.events",
                    Ok(Availability::Supported),
                    format!(
                        "RegisterControlChangeNotify ok; own write observed: {seen}; {} callbacks total",
                        rec.count(VOLUME_OBSERVE)
                    ),
                )
            }
            (Ok(_), Err(e)) => CapabilityRow::from_error(
                "system.audio.events",
                e.clone(),
                "RegisterControlChangeNotify",
            ),
            (status, _) => CapabilityRow::new(
                "system.audio.events",
                status.clone(),
                "follows system.audio.read",
            ),
        };
        let device_events = match &self.register_device {
            Ok(()) => CapabilityRow::new(
                "system.audio.device-events",
                Ok(Availability::Supported),
                format!(
                    "RegisterEndpointNotificationCallback ok; {} callbacks (device changes not triggered)",
                    rec.count(DEVICE_OBSERVE)
                ),
            ),
            Err(e) => CapabilityRow::from_error(
                "system.audio.device-events",
                e.clone(),
                "RegisterEndpointNotificationCallback",
            ),
        };
        let provider = match (&read_row.status, &write_row.status) {
            (Ok(r), Ok(w)) => Ok(model::provider_availability(r, Some(w))),
            (Ok(r), Err(_)) => Ok(model::provider_availability(r, None)),
            (Err(e), _) => Err(e.clone()),
        };
        vec![
            CapabilityRow::new("provider.audio", provider, "read + write combined"),
            read_row,
            write_row,
            events,
            device_events,
        ]
    }
}

fn snapshot(ep: &Endpoint, read_ms: f64) -> Value {
    let mut channels = None;
    let mut range = None;
    let mut steps = None;
    let mut hardware = None;
    // SAFETY: valid interface; out-pointers live for each call.
    unsafe {
        if let Ok(n) = ep.volume.GetChannelCount() {
            channels = Some(n);
        }
        let (mut min, mut max, mut inc) = (0f32, 0f32, 0f32);
        if ep
            .volume
            .GetVolumeRange(&mut min, &mut max, &mut inc)
            .is_ok()
        {
            range = Some(
                json!({ "min_db": round3(min), "max_db": round3(max), "increment_db": round3(inc) }),
            );
        }
        let (mut step, mut count) = (0u32, 0u32);
        if ep.volume.GetVolumeStepInfo(&mut step, &mut count).is_ok() {
            steps = Some(json!({ "step": step, "count": count }));
        }
        if let Ok(mask) = ep.volume.QueryHardwareSupport() {
            hardware = Some(json!({
                "mask": mask,
                "volume": mask & 1 != 0,
                "mute": mask & 2 != 0,
                "meter": mask & 4 != 0,
            }));
        }
    }
    let read = read(&ep.volume);
    // Is the communications default the same device as the console default?
    // SAFETY: valid enumerator.
    let comms = unsafe {
        ep.enumerator
            .GetDefaultAudioEndpoint(eRender, eCommunications)
    }
    .map_err(|e| perr("GetDefaultAudioEndpoint(eRender, eCommunications)", &e))
    .and_then(|d| device_id(&d));
    json!({
        "endpoint": model::fingerprint(&ep.id),
        "level": read.as_ref().map(|(l, _)| round3(*l)).ok(),
        "muted": read.as_ref().map(|(_, m)| *m).ok(),
        "read_ms": read_ms,
        "channels": channels,
        "volume_range": range,
        "volume_steps": steps,
        "hardware_support": hardware,
        "communications_default_same_device": comms.as_ref().map(|id| *id == ep.id).ok(),
    })
}

/// Provider restart and stale-subscription checks on fresh objects.
pub fn restart_checks(rec: &Arc<Recorder>) -> Value {
    let first = match open_default() {
        Ok(ep) => ep,
        Err(e) => return json!({ "open": e.to_json() }),
    };
    const A: &str = "audio.restart.A";
    const B: &str = "audio.restart.B";
    const C: &str = "audio.restart.C-leaked";
    let cb_a = volume_callback(A, rec);
    // SAFETY: valid interfaces throughout; callbacks are kept alive locally.
    let reg_a = unsafe { first.volume.RegisterControlChangeNotify(&cb_a) }
        .map_err(|e| perr("RegisterControlChangeNotify(A)", &e));
    let write_a = same_value_write(&first.volume, rec, Some(A));
    // SAFETY: as above.
    let unreg_a = unsafe { first.volume.UnregisterControlChangeNotify(&cb_a) }
        .map_err(|e| perr("UnregisterControlChangeNotify(A)", &e));
    let after_unreg = rec.now_ms();
    let poke = same_value_write(&first.volume, rec, None);
    std::thread::sleep(Duration::from_millis(500));
    let a_after_unregister = rec.count_after(A, after_unreg);

    // Leaked registration: register C on a separate endpoint object, then drop
    // every reference to that object without unregistering.
    let leak = open_default().and_then(|ep| {
        let cb_c = volume_callback(C, rec);
        // SAFETY: as above.
        unsafe { ep.volume.RegisterControlChangeNotify(&cb_c) }
            .map_err(|e| perr("RegisterControlChangeNotify(C)", &e))
    });

    // "Restart": drop the enumerator, keep the old IAudioEndpointVolume as a
    // stale handle, open everything again.
    let first_id = first.id.clone();
    let stale_volume = first.volume.clone();
    drop(first);
    let (second, reopen_ms) = timed(open_default);
    let second = match second {
        Ok(ep) => ep,
        Err(e) => return json!({ "reopen": e.to_json() }),
    };
    let cb_b = volume_callback(B, rec);
    // SAFETY: as above.
    let reg_b = unsafe { second.volume.RegisterControlChangeNotify(&cb_b) }
        .map_err(|e| perr("RegisterControlChangeNotify(B)", &e));
    let t = rec.now_ms();
    let write_b = same_value_write(&second.volume, rec, Some(B));
    std::thread::sleep(Duration::from_millis(300));
    let stale_read = read(&stale_volume);
    // SAFETY: as above.
    let unreg_b = unsafe { second.volume.UnregisterControlChangeNotify(&cb_b) }
        .map_err(|e| perr("UnregisterControlChangeNotify(B)", &e));

    json!({
        "register_A": result_json(&reg_a),
        "write_with_A": write_a,
        "unregister_A": result_json(&unreg_a),
        "write_after_unregister_A": poke,
        "A_callbacks_after_unregister": a_after_unregister,
        "leaked_registration_C": result_json(&leak),
        "reopen_ms": reopen_ms,
        "same_endpoint_after_reopen": second.id == first_id,
        "register_B": result_json(&reg_b),
        "write_with_B": write_b,
        "A_callbacks_after_restart": rec.count_after(A, t),
        "C_callbacks_after_dropping_its_endpoint": rec.count_after(C, t),
        "stale_pre_restart_handle_read": match &stale_read {
            Ok((l, m)) => json!({ "ok": true, "level": round3(*l), "muted": m }),
            Err(e) => json!({ "ok": false, "error": e.to_json() }),
        },
        "unregister_B": result_json(&unreg_b),
        "non_active_endpoints": non_active_endpoints(&second.enumerator),
        "default_capture_endpoint": default_capture(&second.enumerator),
    })
}

/// Closest available stand-in for "device removed": activate endpoints that
/// are disabled, unplugged or not present and record what the API returns.
fn non_active_endpoints(enumerator: &IMMDeviceEnumerator) -> Value {
    let mask = DEVICE_STATE(
        DEVICE_STATE_DISABLED.0 | DEVICE_STATE_NOTPRESENT.0 | DEVICE_STATE_UNPLUGGED.0,
    );
    // SAFETY: valid enumerator and collection.
    unsafe {
        let collection = match enumerator.EnumAudioEndpoints(eRender, mask) {
            Ok(c) => c,
            Err(e) => return json!({ "enumerate": perr("EnumAudioEndpoints", &e).to_json() }),
        };
        let count = collection.GetCount().unwrap_or(0);
        let mut probes = Vec::new();
        for i in 0..count.min(4) {
            let Ok(device) = collection.Item(i) else {
                continue;
            };
            let state = device.GetState().map(|s| s.0).ok();
            let activate = device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(|e| perr("Activate(IAudioEndpointVolume) on non-active endpoint", &e));
            let read_result = activate.as_ref().map_err(Clone::clone).and_then(read);
            // Generic HRESULT mapping vs the endpoint-state-aware mapping.
            let mapped = |contextual: bool| match &read_result {
                Ok(_) => json!("supported (endpoint object still answers)"),
                Err(e) => {
                    let mut e = e.clone();
                    if contextual && let (Some(h), Some(s)) = (e.hresult, state) {
                        e.kind = model::classify_endpoint_activation(h, s);
                    }
                    e.availability()
                        .map(|a| a.to_json())
                        .unwrap_or_else(|| json!(e.kind.label()))
                }
            };
            probes.push(json!({
                "state": state,
                "activate": result_json(&activate),
                "read": result_json(&read_result),
                "mapped_generic": mapped(false),
                "mapped_with_endpoint_state": mapped(true),
            }));
        }
        json!({ "count": count, "probed": probes })
    }
}

fn default_capture(enumerator: &IMMDeviceEnumerator) -> Value {
    // SAFETY: valid enumerator.
    let result = unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
        .map_err(|e| perr("GetDefaultAudioEndpoint(eCapture, eConsole)", &e));
    match result {
        Ok(_) => json!({ "present": true }),
        Err(e) => {
            json!({ "present": false, "mapped": e.availability().map(|a| a.to_json()), "error": e.to_json() })
        }
    }
}

/// Core Audio from an STA thread: where do callbacks arrive, and does the
/// thread need to pump messages for them?
pub fn sta_probe(rec: &Arc<Recorder>) -> Value {
    let rec = Arc::clone(rec);
    std::thread::spawn(move || {
        const LABEL: &str = "audio.sta";
        let com = Com::init(false);
        let tid = thread_id();
        let apartment = super::apartment();
        let ep = match open_default() {
            Ok(ep) => ep,
            Err(e) => return json!({ "coinit": result_json(&com), "open": e.to_json() }),
        };
        let cb = volume_callback(LABEL, &rec);
        // SAFETY: valid interfaces; callback kept alive locally.
        let reg = unsafe { ep.volume.RegisterControlChangeNotify(&cb) }
            .map_err(|e| perr("RegisterControlChangeNotify (STA)", &e));
        let t = rec.now_ms();
        let write = same_value_write(&ep.volume, &rec, None);
        std::thread::sleep(Duration::from_millis(700));
        let without_pump = rec.count_after(LABEL, t);
        let dispatched = pump_messages(Duration::from_millis(700));
        let with_pump = rec.count_after(LABEL, t);
        // SAFETY: as above.
        let unreg = unsafe { ep.volume.UnregisterControlChangeNotify(&cb) }
            .map_err(|e| perr("UnregisterControlChangeNotify (STA)", &e));
        drop(ep);
        let threads = rec.threads(LABEL);
        let on_sta_thread = threads.iter().any(|t| t["thread_id"] == tid);
        drop(com);
        json!({
            "thread_id": tid,
            "apartment": apartment,
            "register": result_json(&reg),
            "write": write,
            "callbacks_without_pumping": without_pump,
            "messages_dispatched": dispatched,
            "callbacks_after_pumping": with_pump,
            "callback_threads": threads,
            "callback_on_sta_thread": on_sta_thread,
            "unregister": result_json(&unreg),
        })
    })
    .join()
    .unwrap_or_else(|_| json!({ "panicked": true }))
}
