//! Network: Win32 Network List Manager (`INetworkListManager` connectivity +
//! `INetworkListManagerEvents` through a connection point) and WinRT
//! `Windows.Networking.Connectivity.NetworkInformation` (internet profile,
//! level, cost, interface type, `NetworkStatusChanged`). Both are probed so
//! the report can say which one the product should use.
//!
//! The SSID is never printed: only whether it could be read and its length.

use super::{CapabilityRow, Com, Recorder, perr, result_json, thread_id, timed};
use crate::model::{self, Availability, ProviderError};
use serde_json::{Value, json};
use std::sync::Arc;
use windows::Networking::Connectivity::{NetworkInformation, NetworkStatusChangedEventHandler};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Networking::NetworkListManager::{
    INetworkListManager, INetworkListManagerEvents, INetworkListManagerEvents_Impl,
    NLM_CONNECTIVITY, NetworkListManager,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, CoCreateInstance, IConnectionPoint, IConnectionPointContainer,
};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
};
use windows::core::{Interface, PCWSTR, implement, w};

const NLM_OBSERVE: &str = "network.nlm.observe";
const WINRT_OBSERVE: &str = "network.winrt.observe";
pub const OBSERVE_LABELS: &[&str] = &[NLM_OBSERVE, WINRT_OBSERVE];

#[implement(INetworkListManagerEvents)]
struct NlmSink {
    label: &'static str,
    rec: Arc<Recorder>,
}

impl INetworkListManagerEvents_Impl for NlmSink_Impl {
    fn ConnectivityChanged(&self, connectivity: NLM_CONNECTIVITY) -> windows::core::Result<()> {
        let c = model::decode_nlm_connectivity(connectivity.0);
        self.rec.record(
            self.label,
            json!({ "flags": connectivity.0, "ipv4": c.ipv4, "ipv6": c.ipv6, "internet": c.internet }),
        );
        Ok(())
    }
}

fn nlm_sink(label: &'static str, rec: &Arc<Recorder>) -> INetworkListManagerEvents {
    NlmSink {
        label,
        rec: Arc::clone(rec),
    }
    .into()
}

pub fn create_nlm_result() -> Result<INetworkListManager, ProviderError> {
    // SAFETY: plain COM activation.
    unsafe { CoCreateInstance(&NetworkListManager, None, CLSCTX_ALL) }
        .map_err(|e| perr("CoCreateInstance(NetworkListManager)", &e))
}

fn connection_point(nlm: &INetworkListManager) -> Result<IConnectionPoint, ProviderError> {
    let container: IConnectionPointContainer = nlm
        .cast()
        .map_err(|e| perr("QueryInterface(IConnectionPointContainer)", &e))?;
    // SAFETY: valid container; IID of the events interface.
    unsafe { container.FindConnectionPoint(&INetworkListManagerEvents::IID) }
        .map_err(|e| perr("FindConnectionPoint(INetworkListManagerEvents)", &e))
}

fn advise(cp: &IConnectionPoint, sink: &INetworkListManagerEvents) -> Result<u32, ProviderError> {
    // SAFETY: valid connection point and sink.
    unsafe { cp.Advise(sink) }.map_err(|e| perr("IConnectionPoint::Advise", &e))
}

fn unadvise(cp: &IConnectionPoint, cookie: u32, what: &str) -> Result<(), ProviderError> {
    // SAFETY: valid connection point; cookie may be stale on purpose.
    unsafe { cp.Unadvise(cookie) }.map_err(|e| perr(what, &e))
}

fn nlm_read(nlm: &INetworkListManager) -> Result<Value, ProviderError> {
    // SAFETY: valid interface.
    let flags = unsafe { nlm.GetConnectivity() }.map_err(|e| perr("GetConnectivity", &e))?;
    // SAFETY: valid interface.
    let internet =
        unsafe { nlm.IsConnectedToInternet() }.map_err(|e| perr("IsConnectedToInternet", &e))?;
    let c = model::decode_nlm_connectivity(flags.0);
    Ok(json!({
        "flags": flags.0,
        "ipv4": c.ipv4,
        "ipv6": c.ipv6,
        "internet": c.internet,
        "any_network": c.any_network,
        "is_connected_to_internet": internet.as_bool(),
    }))
}

/// Reads a `REG_SZ`; `None` if absent or unreadable.
fn reg_string(root: HKEY, subkey: PCWSTR, value: PCWSTR) -> Option<String> {
    let mut buf = [0u16; 64];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: buffer and length describe valid writable memory.
    let status = unsafe {
        RegGetValueW(
            root,
            subkey,
            value,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let chars = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..chars]))
}

struct Consent {
    machine: Option<String>,
    user: Option<String>,
    non_packaged: Option<String>,
}

fn location_consent() -> Consent {
    let key = w!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location"
    );
    let np = w!(
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\location\\NonPackaged"
    );
    Consent {
        machine: reg_string(HKEY_LOCAL_MACHINE, key, w!("Value")),
        user: reg_string(HKEY_CURRENT_USER, key, w!("Value")),
        non_packaged: reg_string(HKEY_CURRENT_USER, np, w!("Value")),
    }
}

struct WinrtRead {
    json: Value,
    is_wlan: bool,
    ssid_len: Option<usize>,
}

fn winrt_read() -> Result<WinrtRead, ProviderError> {
    let profiles = NetworkInformation::GetConnectionProfiles()
        .and_then(|v| v.Size())
        .map_err(|e| perr("NetworkInformation::GetConnectionProfiles", &e))?;
    let profile = match NetworkInformation::GetInternetConnectionProfile() {
        Ok(p) => p,
        // A null profile (no internet connection) surfaces as an error with
        // HRESULT 0 in windows-rs: that is "no connection", not a failure.
        Err(e) if e.code().0 == 0 => {
            return Ok(WinrtRead {
                json: json!({ "connection_profiles": profiles, "internet_profile": null, "level": "none" }),
                is_wlan: false,
                ssid_len: None,
            });
        }
        Err(e) => return Err(perr("NetworkInformation::GetInternetConnectionProfile", &e)),
    };
    let level = profile
        .GetNetworkConnectivityLevel()
        .map_err(|e| perr("GetNetworkConnectivityLevel", &e))?;
    let cost = profile
        .GetConnectionCost()
        .and_then(|c| c.NetworkCostType())
        .map(|c| model::winrt_cost_type(c.0))
        .ok();
    let interface = profile
        .NetworkAdapter()
        .and_then(|a| a.IanaInterfaceType())
        .ok();
    let is_wlan = profile.IsWlanConnectionProfile().unwrap_or(false);
    let is_wwan = profile.IsWwanConnectionProfile().unwrap_or(false);
    let ssid = if is_wlan {
        profile
            .WlanConnectionProfileDetails()
            .and_then(|d| d.GetConnectedSsid())
            .map(|s| s.len())
            .map_err(|e| perr("WlanConnectionProfileDetails::GetConnectedSsid", &e))
    } else {
        Ok(0)
    };
    Ok(WinrtRead {
        json: json!({
            "connection_profiles": profiles,
            "internet_profile": true,
            "level": model::winrt_connectivity_level(level.0),
            "cost": cost,
            "iana_interface_type": interface,
            "interface": interface.map(model::iana_interface_type),
            "is_wlan": is_wlan,
            "is_wwan": is_wwan,
            "ssid_readable": matches!(ssid, Ok(n) if n > 0),
            "ssid_length": ssid.as_ref().ok(),
            "ssid_error": ssid.as_ref().err().map(ProviderError::to_json),
        }),
        is_wlan,
        ssid_len: ssid.ok(),
    })
}

fn winrt_handler(label: &'static str, rec: &Arc<Recorder>) -> NetworkStatusChangedEventHandler {
    let rec = Arc::clone(rec);
    NetworkStatusChangedEventHandler::new(move |_| {
        let level = NetworkInformation::GetInternetConnectionProfile()
            .and_then(|p| p.GetNetworkConnectivityLevel())
            .map(|l| model::winrt_connectivity_level(l.0))
            .unwrap_or("none");
        rec.record(label, json!({ "level": level }));
        Ok(())
    })
}

pub struct Session {
    nlm: Result<INetworkListManager, ProviderError>,
    create_ms: f64,
    nlm_read: Result<Value, ProviderError>,
    read_ms: f64,
    cp: Option<IConnectionPoint>,
    cookie: Result<u32, ProviderError>,
    winrt: Result<WinrtRead, ProviderError>,
    winrt_ms: f64,
    winrt_token: Result<i64, ProviderError>,
    consent: Consent,
}

impl Session {
    pub fn start(rec: &Arc<Recorder>) -> Self {
        let (nlm, create_ms) = timed(create_nlm_result);
        let (nlm_read, read_ms) = timed(|| nlm.as_ref().map_err(Clone::clone).and_then(nlm_read));
        let cp = nlm
            .as_ref()
            .map_err(Clone::clone)
            .and_then(connection_point);
        let cookie = cp
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|cp| advise(cp, &nlm_sink(NLM_OBSERVE, rec)));
        let (winrt, winrt_ms) = timed(winrt_read);
        let winrt_token =
            NetworkInformation::NetworkStatusChanged(&winrt_handler(WINRT_OBSERVE, rec))
                .map_err(|e| perr("NetworkInformation::NetworkStatusChanged", &e));
        Self {
            nlm,
            create_ms,
            nlm_read,
            read_ms,
            cp: cp.ok(),
            cookie,
            winrt,
            winrt_ms,
            winrt_token,
            consent: location_consent(),
        }
    }

    pub fn stop(&mut self) -> Value {
        let mut out = json!({});
        if let (Some(cp), Ok(cookie)) = (&self.cp, &self.cookie) {
            out["nlm_unadvise"] = result_json(&unadvise(cp, *cookie, "Unadvise"));
            out["nlm_unadvise_again"] = result_json(&unadvise(cp, *cookie, "Unadvise (again)"));
        }
        if let Ok(token) = &self.winrt_token {
            let first = NetworkInformation::RemoveNetworkStatusChanged(*token)
                .map_err(|e| perr("RemoveNetworkStatusChanged", &e));
            let second = NetworkInformation::RemoveNetworkStatusChanged(*token)
                .map_err(|e| perr("RemoveNetworkStatusChanged (again)", &e));
            out["winrt_remove"] = result_json(&first);
            out["winrt_remove_again"] = result_json(&second);
        }
        out
    }

    pub fn report(&self, stop: Value) -> Value {
        json!({
            "api": {
                "connectivity": "Win32 Network List Manager: INetworkListManager::GetConnectivity + INetworkListManagerEvents (connection point)",
                "details": "WinRT Windows.Networking.Connectivity.NetworkInformation (internet ConnectionProfile, NetworkStatusChanged)",
            },
            "nlm_create": result_json(&self.nlm),
            "nlm_create_ms": self.create_ms,
            "nlm": match &self.nlm_read { Ok(v) => v.clone(), Err(e) => json!({ "error": e.to_json() }) },
            "nlm_read_ms": self.read_ms,
            "nlm_advise": result_json(&self.cookie),
            "winrt": match &self.winrt { Ok(w) => w.json.clone(), Err(e) => json!({ "error": e.to_json() }) },
            "winrt_ms": self.winrt_ms,
            "winrt_subscribe": result_json(&self.winrt_token),
            "location_consent": {
                "machine": self.consent.machine,
                "user": self.consent.user,
                "non_packaged_apps": self.consent.non_packaged,
            },
            "stop": stop,
        })
    }

    pub fn capabilities(&self) -> Vec<CapabilityRow> {
        let read = match &self.nlm_read {
            Ok(v) => CapabilityRow::new(
                "system.network.read",
                Ok(Availability::Supported),
                format!(
                    "NLM ipv4 {} / ipv6 {}, internet {}",
                    v["ipv4"], v["ipv6"], v["internet"]
                ),
            ),
            Err(e) => {
                CapabilityRow::from_error("system.network.read", e.clone(), "NLM GetConnectivity")
            }
        };
        let events = match (&self.cookie, &self.winrt_token) {
            (Ok(_), Ok(_)) => CapabilityRow::new(
                "system.network.events",
                Ok(Availability::Supported),
                "NLM Advise and NetworkStatusChanged registered (no change triggered)",
            ),
            (Err(e), _) | (_, Err(e)) => {
                CapabilityRow::from_error("system.network.events", e.clone(), "subscription")
            }
        };
        let details = match &self.winrt {
            Ok(w) => CapabilityRow::new(
                "system.network.details",
                Ok(Availability::Supported),
                format!(
                    "level {}, interface {}, cost {}",
                    w.json["level"], w.json["interface"], w.json["cost"]
                ),
            ),
            Err(e) => {
                CapabilityRow::from_error("system.network.details", e.clone(), "NetworkInformation")
            }
        };
        let ssid = match &self.winrt {
            Ok(w) => {
                let a = model::ssid_availability(
                    w.is_wlan,
                    w.ssid_len,
                    self.consent.machine.as_deref(),
                    self.consent.user.as_deref(),
                    self.consent.non_packaged.as_deref(),
                );
                CapabilityRow::new(
                    "system.network.ssid",
                    Ok(a),
                    "GetConnectedSsid (value not printed); gated by location consent on Windows 11 24H2+",
                )
            }
            Err(e) => {
                CapabilityRow::from_error("system.network.ssid", e.clone(), "NetworkInformation")
            }
        };
        let toggle = CapabilityRow::new(
            "system.network.toggle",
            Ok(Availability::unavailable(
                "no network/radio toggle path in this provider; Windows.Devices.Radios not evaluated",
            )),
            "not exercised",
        );
        let provider = match &read.status {
            Ok(r) => Ok(model::provider_availability(r, None)),
            Err(e) => Err(e.clone()),
        };
        vec![
            CapabilityRow::new(
                "provider.network",
                provider,
                "read-only: no write capability is offered",
            ),
            read,
            events,
            details,
            ssid,
            toggle,
        ]
    }
}

pub fn restart_checks(rec: &Arc<Recorder>) -> Value {
    const NLM_A: &str = "network.restart.nlm";
    const WINRT_A: &str = "network.restart.winrt";
    let first = create_nlm_result().and_then(|nlm| {
        let cp = connection_point(&nlm)?;
        let cookie = advise(&cp, &nlm_sink(NLM_A, rec))?;
        Ok((nlm, cp, cookie))
    });
    let (first_nlm, first_cp, first_cookie) = match first {
        Ok(v) => v,
        Err(e) => return json!({ "first": e.to_json() }),
    };
    let unadvise_first = unadvise(&first_cp, first_cookie, "Unadvise(first)");
    drop(first_nlm);
    // Unadvise through a connection point whose manager was released.
    let stale_unadvise = unadvise(
        &first_cp,
        first_cookie,
        "Unadvise(stale cookie, released manager)",
    );
    drop(first_cp);
    let (second, recreate_ms) = timed(|| {
        create_nlm_result().and_then(|nlm| {
            let read = nlm_read(&nlm)?;
            let cp = connection_point(&nlm)?;
            let cookie = advise(&cp, &nlm_sink(NLM_A, rec))?;
            Ok((nlm, cp, cookie, read))
        })
    });
    let second_json = match &second {
        Ok((_, cp, cookie, read)) => json!({
            "ok": true,
            "read": read,
            "new_cookie_differs": *cookie != first_cookie,
            "unadvise": result_json(&unadvise(cp, *cookie, "Unadvise(second)")),
        }),
        Err(e) => json!({ "ok": false, "error": e.to_json() }),
    };

    let token1 = NetworkInformation::NetworkStatusChanged(&winrt_handler(WINRT_A, rec))
        .map_err(|e| perr("NetworkStatusChanged(1)", &e));
    let remove1 = token1.as_ref().map_err(Clone::clone).and_then(|t| {
        NetworkInformation::RemoveNetworkStatusChanged(*t).map_err(|e| perr("Remove(1)", &e))
    });
    let token2 = NetworkInformation::NetworkStatusChanged(&winrt_handler(WINRT_A, rec))
        .map_err(|e| perr("NetworkStatusChanged(2)", &e));
    let remove2 = token2.as_ref().map_err(Clone::clone).and_then(|t| {
        NetworkInformation::RemoveNetworkStatusChanged(*t).map_err(|e| perr("Remove(2)", &e))
    });
    let bogus = NetworkInformation::RemoveNetworkStatusChanged(0x0123_4567)
        .map_err(|e| perr("RemoveNetworkStatusChanged(never-issued token)", &e));
    json!({
        "nlm_first_cookie": first_cookie,
        "nlm_unadvise_first": result_json(&unadvise_first),
        "nlm_unadvise_stale_after_release": result_json(&stale_unadvise),
        "nlm_recreate_ms": recreate_ms,
        "nlm_recreated": second_json,
        "winrt_tokens": [token1.as_ref().ok(), token2.as_ref().ok()],
        "winrt_remove_1": result_json(&remove1),
        "winrt_remove_2": result_json(&remove2),
        "winrt_remove_never_issued_token": result_json(&bogus),
        "callbacks_seen": rec.count(NLM_A) + rec.count(WINRT_A),
    })
}

/// NLM from an STA thread: activation, read and Advise. Connectivity events
/// for an STA sink are delivered through the thread's message queue; no
/// event can be triggered without changing the network, so delivery itself
/// is not checked here.
pub fn sta_probe() -> Value {
    std::thread::spawn(|| {
        let com = Com::init(false);
        let apartment = super::apartment();
        let rec = Arc::new(Recorder::new());
        let result = create_nlm_result().and_then(|nlm| {
            let read = nlm_read(&nlm)?;
            let cp = connection_point(&nlm)?;
            let cookie = advise(&cp, &nlm_sink("network.sta", &rec))?;
            let unadvise = unadvise(&cp, cookie, "Unadvise (STA)");
            Ok(json!({ "read": read, "advise": true, "unadvise": result_json(&unadvise) }))
        });
        let winrt = winrt_read().map(|w| w.json);
        drop(com);
        json!({
            "thread_id": thread_id(),
            "apartment": apartment,
            "nlm": match result { Ok(v) => v, Err(e) => json!({ "error": e.to_json() }) },
            "winrt_read": result_json(&winrt),
        })
    })
    .join()
    .unwrap_or_else(|_| json!({ "panicked": true }))
}
