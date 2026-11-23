//! Windows probe. `run` initializes COM (MTA) on the main thread, starts the
//! three providers with change subscriptions, observes for N seconds, tears
//! the subscriptions down, runs restart / stale-subscription / apartment
//! checks and prints one JSON report.

mod audio;
mod network;
mod power;

use crate::model::{self, Availability, ErrorKind, Options, ProviderError};
use serde_json::{Map, Value, json};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use windows::Win32::System::Com::{
    APTTYPE, APTTYPE_MAINSTA, APTTYPE_MTA, APTTYPE_NA, APTTYPE_STA, APTTYPEQUALIFIER,
    APTTYPEQUALIFIER_IMPLICIT_MTA, APTTYPEQUALIFIER_NONE, COINIT_APARTMENTTHREADED,
    COINIT_MULTITHREADED, CoGetApartmentType, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage,
};

/// One callback invocation observed by any provider.
struct Event {
    t_ms: f64,
    label: String,
    thread_id: u32,
    apartment: String,
    detail: Value,
}

/// Thread-safe log of callback invocations, shared by every callback object.
pub struct Recorder {
    start: Instant,
    events: Mutex<Vec<Event>>,
}

impl Recorder {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            events: Mutex::new(Vec::new()),
        }
    }

    pub fn now_ms(&self) -> f64 {
        ms(self.start.elapsed())
    }

    /// Called from OS callback threads: must not block for long.
    pub fn record(&self, label: &str, detail: Value) {
        let event = Event {
            t_ms: self.now_ms(),
            label: label.to_string(),
            thread_id: thread_id(),
            apartment: apartment(),
            detail,
        };
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    pub fn count(&self, label: &str) -> usize {
        self.count_after(label, f64::MIN)
    }

    pub fn count_after(&self, label: &str, t_ms: f64) -> usize {
        self.events
            .lock()
            .map(|events| {
                events
                    .iter()
                    .filter(|e| e.label == label && e.t_ms > t_ms)
                    .count()
            })
            .unwrap_or(0)
    }

    /// Thread IDs of the callbacks for `label`, deduplicated.
    pub fn threads(&self, label: &str) -> Vec<Value> {
        let mut seen: Vec<(u32, String)> = Vec::new();
        if let Ok(events) = self.events.lock() {
            for e in events.iter().filter(|e| e.label == label) {
                if !seen.iter().any(|(id, _)| *id == e.thread_id) {
                    seen.push((e.thread_id, e.apartment.clone()));
                }
            }
        }
        seen.into_iter()
            .map(|(id, apt)| json!({ "thread_id": id, "apartment": apt }))
            .collect()
    }

    /// Waits until an event with `label` newer than `since_ms` matches `pred`,
    /// returning its timestamp, or `None` after `timeout`.
    pub fn wait_for(
        &self,
        label: &str,
        since_ms: f64,
        timeout: Duration,
        pred: impl Fn(&Value) -> bool,
    ) -> Option<f64> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(events) = self.events.lock()
                && let Some(e) = events
                    .iter()
                    .find(|e| e.label == label && e.t_ms > since_ms && pred(&e.detail))
            {
                return Some(e.t_ms);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn to_json(&self) -> Value {
        let events = match self.events.lock() {
            Ok(events) => events,
            Err(_) => return json!([]),
        };
        Value::Array(
            events
                .iter()
                .map(|e| {
                    json!({
                        "t_ms": round(e.t_ms),
                        "label": e.label,
                        "thread_id": e.thread_id,
                        "apartment": e.apartment,
                        "detail": e.detail,
                    })
                })
                .collect(),
        )
    }
}

pub fn ms(d: Duration) -> f64 {
    round(d.as_secs_f64() * 1000.0)
}

pub fn round(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Runs `f` and returns its result with the elapsed milliseconds.
pub fn timed<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let start = Instant::now();
    let value = f();
    (value, ms(start.elapsed()))
}

pub fn thread_id() -> u32 {
    // SAFETY: no preconditions.
    unsafe { GetCurrentThreadId() }
}

/// The calling thread's COM apartment, as `CoGetApartmentType` reports it.
pub fn apartment() -> String {
    let mut kind = APTTYPE::default();
    let mut qualifier = APTTYPEQUALIFIER::default();
    // SAFETY: both out-pointers are valid for the call.
    match unsafe { CoGetApartmentType(&mut kind, &mut qualifier) } {
        Ok(()) => {
            let name = match kind {
                APTTYPE_MTA => "MTA",
                APTTYPE_STA => "STA",
                APTTYPE_MAINSTA => "main STA",
                APTTYPE_NA => "NA",
                _ => "other",
            };
            match qualifier {
                APTTYPEQUALIFIER_NONE => name.to_string(),
                APTTYPEQUALIFIER_IMPLICIT_MTA => format!("{name} (implicit)"),
                other => format!("{name} (qualifier {})", other.0),
            }
        }
        Err(e) => format!("none ({})", model::hresult_name(Some(e.code().0 as u32))),
    }
}

/// Converts a `windows` error into the spike's typed error.
pub fn perr(operation: &str, error: &windows_core::Error) -> ProviderError {
    ProviderError::from_hresult(operation, error.code().0 as u32, error.message())
}

pub fn result_json<T>(result: &Result<T, ProviderError>) -> Value {
    match result {
        Ok(_) => json!({ "ok": true }),
        Err(e) => json!({ "ok": false, "error": e.to_json() }),
    }
}

/// Owns one `CoInitializeEx` on the current thread.
pub struct Com;

impl Com {
    pub fn init(multithreaded: bool) -> Result<Self, ProviderError> {
        let mode = if multithreaded {
            COINIT_MULTITHREADED
        } else {
            COINIT_APARTMENTTHREADED
        };
        // SAFETY: balanced by CoUninitialize in Drop on the same thread.
        let hr = unsafe { CoInitializeEx(None, mode) };
        if hr.is_err() {
            return Err(ProviderError::from_hresult(
                "CoInitializeEx",
                hr.0 as u32,
                hr.message(),
            ));
        }
        Ok(Self)
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        // SAFETY: matches the successful CoInitializeEx in `init`.
        unsafe { CoUninitialize() };
    }
}

/// Pumps the calling thread's message queue for `duration` (STA delivery).
pub fn pump_messages(duration: Duration) -> u32 {
    let deadline = Instant::now() + duration;
    let mut dispatched = 0;
    let mut msg = MSG::default();
    while Instant::now() < deadline {
        // SAFETY: msg is a valid out-pointer; messages belong to this thread.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: msg was filled by PeekMessageW.
            unsafe {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            dispatched += 1;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    dispatched
}

/// One row of the capability table: either a status or a typed error.
pub struct CapabilityRow {
    pub id: &'static str,
    pub status: Result<Availability, ProviderError>,
    pub evidence: String,
}

impl CapabilityRow {
    pub fn new(
        id: &'static str,
        status: Result<Availability, ProviderError>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            id,
            status,
            evidence: evidence.into(),
        }
    }

    /// A typed error that maps to a status becomes that status; timeouts and
    /// failures stay errors.
    pub fn from_error(id: &'static str, error: ProviderError, evidence: impl Into<String>) -> Self {
        let status = error.availability().ok_or(error);
        Self::new(id, status, evidence)
    }

    fn to_json(&self) -> Value {
        let mut row = Map::new();
        row.insert("id".into(), json!(self.id));
        match &self.status {
            Ok(a) => {
                row.insert("availability".into(), a.to_json());
            }
            Err(e) => {
                row.insert("error".into(), e.to_json());
            }
        }
        row.insert("evidence".into(), json!(self.evidence));
        Value::Object(row)
    }

    fn is_hard_failure(&self) -> bool {
        matches!(&self.status, Err(e) if matches!(e.kind, ErrorKind::Failure | ErrorKind::Timeout))
    }
}

/// COM apartment behaviour on a thread that never calls `CoInitializeEx`,
/// measured before the process has an MTA (so nothing is implicit yet).
fn before_any_com_init() -> Value {
    std::thread::spawn(|| {
        let apartment_before = apartment();
        let audio = audio::create_enumerator_result();
        let nlm = network::create_nlm_result();
        // WinRT activation through windows-rs: its factory cache retries with
        // CoIncrementMTAUsage when RoGetActivationFactory reports
        // CO_E_NOTINITIALIZED, which creates the process MTA.
        let (winrt, winrt_ms) = timed(power::winrt_battery_status_result);
        json!({
            "thread_id": thread_id(),
            "apartment_before": apartment_before,
            "core_audio_cocreateinstance": result_json(&audio),
            "nlm_cocreateinstance": result_json(&nlm),
            "winrt_power_manager_battery_status": result_json(&winrt),
            "winrt_ms": winrt_ms,
            "apartment_after_winrt": apartment(),
        })
    })
    .join()
    .unwrap_or_else(|_| json!({ "panicked": true }))
}

/// A thread that never calls `CoInitializeEx` once the process has an MTA.
fn implicit_mta_thread() -> Value {
    std::thread::spawn(|| {
        let apartment = apartment();
        let audio = audio::read_default_volume_result();
        let nlm = network::create_nlm_result();
        json!({
            "thread_id": thread_id(),
            "apartment": apartment,
            "core_audio_read_volume": result_json(&audio),
            "nlm_cocreateinstance": result_json(&nlm),
        })
    })
    .join()
    .unwrap_or_else(|_| json!({ "panicked": true }))
}

pub fn run(options: &Options) -> ExitCode {
    let started = Instant::now();
    let rec = Arc::new(Recorder::new());
    audio::set_nudge(options.nudge);

    let pre_init = before_any_com_init();

    let (com, com_ms) = timed(|| Com::init(true));
    let _com = match com {
        Ok(com) => com,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };
    let main_thread = json!({
        "thread_id": thread_id(),
        "coinit": "COINIT_MULTITHREADED",
        "coinit_ms": com_ms,
        "apartment": apartment(),
    });

    // Snapshots + long-lived subscriptions for every provider.
    let mut audio = audio::Session::start(&rec);
    let mut network = network::Session::start(&rec);
    let mut power = power::Session::start(&rec);

    // Exercise the audio write path with identical values (no audible change).
    audio.exercise_write(&rec);

    let observe_from = rec.now_ms();
    std::thread::sleep(Duration::from_secs(options.smoke_seconds));
    let observe_to = rec.now_ms();

    // Tear down, then poke the audio endpoint again to provoke any stale
    // callback that survived unregistration.
    let unsubscribed_at = rec.now_ms();
    let audio_stop = audio.stop(&rec);
    let network_stop = network.stop();
    let power_stop = power.stop();
    let poke = audio::same_value_write_default(&rec);
    std::thread::sleep(Duration::from_millis(500));
    let stale_after_unsubscribe: Map<String, Value> = audio::OBSERVE_LABELS
        .iter()
        .chain(network::OBSERVE_LABELS)
        .chain(power::OBSERVE_LABELS)
        .map(|label| {
            (
                label.to_string(),
                json!(rec.count_after(label, unsubscribed_at)),
            )
        })
        .collect();

    let restart = json!({
        "audio": audio::restart_checks(&rec),
        "network": network::restart_checks(&rec),
        "power": power::restart_checks(&rec),
    });
    let apartments = json!({
        "before_any_com_init": pre_init,
        "main_thread": main_thread,
        "implicit_mta_thread": implicit_mta_thread(),
        "audio_on_sta_thread": audio::sta_probe(&rec),
        "nlm_on_sta_thread": network::sta_probe(),
    });

    let mut rows = Vec::new();
    rows.extend(audio.capabilities(&rec));
    rows.extend(network.capabilities());
    rows.extend(power.capabilities());
    let failed = rows.iter().any(CapabilityRow::is_hard_failure);

    let observed: Map<String, Value> = audio::OBSERVE_LABELS
        .iter()
        .chain(network::OBSERVE_LABELS)
        .chain(power::OBSERVE_LABELS)
        .map(|label| {
            (
                label.to_string(),
                json!({ "count": rec.count(label), "threads": rec.threads(label) }),
            )
        })
        .collect();

    let report = json!({
        "spike": "SPK-010 windows-system-providers",
        "crate_version": env!("CARGO_PKG_VERSION"),
        "windows_crate": "0.62.2",
        "smoke_seconds": options.smoke_seconds,
        "audio_write_mode": if options.nudge { "nudge-0.01 (restored immediately)" } else { "same-value" },
        "capabilities": rows.iter().map(CapabilityRow::to_json).collect::<Vec<_>>(),
        "providers": {
            "audio": audio.report(audio_stop),
            "network": network.report(network_stop),
            "battery": power.report(power_stop),
        },
        "observation": {
            "window_ms": [round(observe_from), round(observe_to)],
            "callbacks": observed,
            "post_unsubscribe_poke": poke,
            "callbacks_after_unsubscribe": stale_after_unsubscribe,
        },
        "restart_and_stale": restart,
        "apartments": apartments,
        "events": rec.to_json(),
        "total_ms": ms(started.elapsed()),
        "result": if failed { "FAIL" } else { "PASS" },
    });
    match serde_json::to_string_pretty(&report) {
        Ok(text) => println!("{text}"),
        Err(e) => {
            eprintln!("error: cannot serialize report: {e}");
            return ExitCode::from(1);
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}
