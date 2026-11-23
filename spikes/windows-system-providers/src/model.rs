//! Platform-independent logic: the spike-local `Availability` enum, the typed
//! provider error, and the pure mappings from raw OS values (HRESULTs,
//! `SYSTEM_POWER_STATUS` bytes, NLM connectivity flags, WinRT enum values) to
//! them. Everything here is unit-tested on every host.
//!
//! The product version of these types belongs to CAP-* / `glint-platform`;
//! this copy only exists so the spike can report statuses precisely.

use serde_json::{Value, json};
use std::fmt;

pub const USAGE: &str = "\
usage: spike-windows-system-providers [--smoke <seconds>] [--help]

  --smoke <seconds>  probe audio, network and battery providers, keep change
                     subscriptions open for <seconds> (1-120), run the restart
                     and stale-subscription checks, print one JSON report and
                     exit (0 = report produced, 1 = a provider failed)
  --nudge            opt-in: exercise volume callbacks by lowering (or, near
                     zero, raising) the master level by 0.01 scalar and
                     restoring it immediately after the callback arrives
  --help             print this text

By default system volume is never changed: the write path is exercised by
writing back the value that was just read (Windows sends no change callback
for that). In every mode the value is re-read and restored if it differs.";

/// Parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub help: bool,
    pub smoke_seconds: u64,
    pub nudge: bool,
}

pub fn parse_args(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        help: false,
        smoke_seconds: 5,
        nudge: false,
    };
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => options.help = true,
            "--nudge" => options.nudge = true,
            "--smoke" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--smoke needs a number of seconds".to_string())?;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("--smoke: not a number: {value}"))?;
                if !(1..=120).contains(&seconds) {
                    return Err(format!("--smoke: {seconds} is outside 1-120"));
                }
                options.smoke_seconds = seconds;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(options)
}

/// Spike-local copy of the proposed capability availability
/// (docs/plan/02-architecture/capabilities-permissions-and-errors.md).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub enum Availability {
    Supported,
    ReadOnly,
    PermissionRequired {
        reason: String,
    },
    // Kept to mirror the proposed enum; no provider probed here is experimental.
    #[allow(dead_code)]
    Experimental {
        reason: String,
    },
    Unavailable {
        reason: String,
    },
}

impl Availability {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn permission_required(reason: impl Into<String>) -> Self {
        Self::PermissionRequired {
            reason: reason.into(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Supported => "supported",
            Self::ReadOnly => "read-only",
            Self::PermissionRequired { .. } => "permission-required",
            Self::Experimental { .. } => "experimental",
            Self::Unavailable { .. } => "unavailable",
        }
    }

    pub fn to_json(&self) -> Value {
        match self {
            Self::Supported | Self::ReadOnly => json!({ "status": self.label() }),
            Self::PermissionRequired { reason }
            | Self::Experimental { reason }
            | Self::Unavailable { reason } => json!({ "status": self.label(), "reason": reason }),
        }
    }
}

/// The five error kinds the ticket requires to stay distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    Unsupported,
    PermissionDenied,
    Unavailable,
    Timeout,
    Failure,
}

impl ErrorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unsupported => "unsupported",
            Self::PermissionDenied => "permission-denied",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Failure => "failure",
        }
    }
}

/// Typed provider error: a kind, the operation that failed and, when the OS
/// gave one, the raw HRESULT.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderError {
    pub kind: ErrorKind,
    pub operation: String,
    pub hresult: Option<u32>,
    pub message: String,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl ProviderError {
    pub fn from_hresult(
        operation: impl Into<String>,
        hresult: u32,
        message: impl Into<String>,
    ) -> Self {
        Self {
            kind: classify_hresult(hresult),
            operation: operation.into(),
            hresult: Some(hresult),
            message: message.into(),
        }
    }

    pub fn new(kind: ErrorKind, operation: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind,
            operation: operation.into(),
            hresult: None,
            message: message.into(),
        }
    }

    /// What a capability query should report when this error stops it.
    /// Timeouts and plain failures are *not* availability states: the caller
    /// must surface them as errors, never as a status.
    pub fn availability(&self) -> Option<Availability> {
        match self.kind {
            ErrorKind::Unsupported => Some(Availability::unavailable(format!(
                "unsupported: {} ({})",
                self.operation,
                hresult_name(self.hresult)
            ))),
            ErrorKind::PermissionDenied => Some(Availability::permission_required(format!(
                "{} denied ({})",
                self.operation,
                hresult_name(self.hresult)
            ))),
            ErrorKind::Unavailable => Some(Availability::unavailable(format!(
                "{} ({})",
                self.operation,
                hresult_name(self.hresult)
            ))),
            ErrorKind::Timeout | ErrorKind::Failure => None,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "kind": self.kind.label(),
            "operation": self.operation,
            "hresult": self.hresult.map(|h| format!("0x{h:08X}")),
            "hresult_name": self.hresult.map(|h| hresult_name(Some(h))),
            "message": self.message,
        })
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: {}: {}",
            self.kind.label(),
            self.operation,
            self.message
        )?;
        if let Some(h) = self.hresult {
            write!(f, " (0x{h:08X})")?;
        }
        Ok(())
    }
}

// HRESULTs the providers can return. Kept here (not from the `windows` crate)
// so the mapping is testable on Linux/macOS CI.
pub const S_OK: u32 = 0;
pub const E_NOTIMPL: u32 = 0x8000_4001;
pub const E_NOINTERFACE: u32 = 0x8000_4002;
pub const E_ACCESSDENIED: u32 = 0x8007_0005;
pub const E_INVALIDARG: u32 = 0x8007_0057;
/// `HRESULT_FROM_WIN32(ERROR_NOT_FOUND)`: Core Audio's "no such endpoint".
pub const E_NOTFOUND: u32 = 0x8007_0490;
/// `HRESULT_FROM_WIN32(ERROR_DEVICE_NOT_CONNECTED)`.
pub const E_DEVICE_NOT_CONNECTED: u32 = 0x8007_048F;
/// `HRESULT_FROM_WIN32(ERROR_TIMEOUT)`.
pub const E_WIN32_TIMEOUT: u32 = 0x8007_05B4;
pub const REGDB_E_CLASSNOTREG: u32 = 0x8004_0154;
pub const CLASS_E_CLASSNOTAVAILABLE: u32 = 0x8004_0111;
pub const CONNECT_E_NOCONNECTION: u32 = 0x8004_0200;
/// What NLM's connection point actually returns for an unknown cookie.
pub const OLE_E_NOCONNECTION: u32 = 0x8004_0004;
/// `HRESULT_FROM_WIN32(ERROR_PATH_NOT_FOUND)`.
pub const E_PATH_NOT_FOUND: u32 = 0x8007_0003;
pub const CO_E_NOTINITIALIZED: u32 = 0x8004_01F0;
pub const RPC_E_CHANGED_MODE: u32 = 0x8001_0106;
pub const RPC_E_WRONG_THREAD: u32 = 0x8001_010E;
pub const RPC_E_TIMEOUT: u32 = 0x8001_011F;
pub const AUDCLNT_E_DEVICE_INVALIDATED: u32 = 0x8889_0004;
pub const AUDCLNT_E_SERVICE_NOT_RUNNING: u32 = 0x8889_0010;

/// Maps an HRESULT to one of the five error kinds. Anything not known to
/// mean "absent", "denied", "unsupported" or "timed out" is a failure; it is
/// never silently treated as success or as unavailable.
pub fn classify_hresult(hresult: u32) -> ErrorKind {
    match hresult {
        E_ACCESSDENIED => ErrorKind::PermissionDenied,
        E_NOTFOUND
        | E_DEVICE_NOT_CONNECTED
        | AUDCLNT_E_DEVICE_INVALIDATED
        | AUDCLNT_E_SERVICE_NOT_RUNNING => ErrorKind::Unavailable,
        E_NOTIMPL | E_NOINTERFACE | REGDB_E_CLASSNOTREG | CLASS_E_CLASSNOTAVAILABLE => {
            ErrorKind::Unsupported
        }
        E_WIN32_TIMEOUT | RPC_E_TIMEOUT => ErrorKind::Timeout,
        _ => ErrorKind::Failure,
    }
}

pub fn hresult_name(hresult: Option<u32>) -> &'static str {
    match hresult {
        None => "none",
        Some(S_OK) => "S_OK",
        Some(E_NOTIMPL) => "E_NOTIMPL",
        Some(E_NOINTERFACE) => "E_NOINTERFACE",
        Some(E_ACCESSDENIED) => "E_ACCESSDENIED",
        Some(E_INVALIDARG) => "E_INVALIDARG",
        Some(E_NOTFOUND) => "E_NOTFOUND",
        Some(E_DEVICE_NOT_CONNECTED) => "ERROR_DEVICE_NOT_CONNECTED",
        Some(E_WIN32_TIMEOUT) => "ERROR_TIMEOUT",
        Some(REGDB_E_CLASSNOTREG) => "REGDB_E_CLASSNOTREG",
        Some(CLASS_E_CLASSNOTAVAILABLE) => "CLASS_E_CLASSNOTAVAILABLE",
        Some(CONNECT_E_NOCONNECTION) => "CONNECT_E_NOCONNECTION",
        Some(OLE_E_NOCONNECTION) => "OLE_E_NOCONNECTION",
        Some(E_PATH_NOT_FOUND) => "ERROR_PATH_NOT_FOUND",
        Some(CO_E_NOTINITIALIZED) => "CO_E_NOTINITIALIZED",
        Some(RPC_E_CHANGED_MODE) => "RPC_E_CHANGED_MODE",
        Some(RPC_E_WRONG_THREAD) => "RPC_E_WRONG_THREAD",
        Some(RPC_E_TIMEOUT) => "RPC_E_TIMEOUT",
        Some(AUDCLNT_E_DEVICE_INVALIDATED) => "AUDCLNT_E_DEVICE_INVALIDATED",
        Some(AUDCLNT_E_SERVICE_NOT_RUNNING) => "AUDCLNT_E_SERVICE_NOT_RUNNING",
        Some(_) => "other",
    }
}

/// Context-specific mapping for activating an interface on an audio endpoint
/// whose `DEVICE_STATE` is known. On an endpoint that is not ACTIVE
/// (disabled, not present, unplugged) any activation error means "device
/// gone", whatever HRESULT the driver stack picked (observed:
/// `AUDCLNT_E_DEVICE_INVALIDATED` and `ERROR_PATH_NOT_FOUND`). On an ACTIVE
/// endpoint the generic classification applies.
pub fn classify_endpoint_activation(hresult: u32, device_state: u32) -> ErrorKind {
    const DEVICE_STATE_ACTIVE: u32 = 1;
    match classify_hresult(hresult) {
        ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
        _ if device_state != DEVICE_STATE_ACTIVE => ErrorKind::Unavailable,
        kind => kind,
    }
}

/// Battery reading derived from `GetSystemPowerStatus`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PowerStatusReading {
    pub availability: Availability,
    /// `Some(true)` on AC, `Some(false)` on battery, `None` unknown.
    pub on_ac: Option<bool>,
    pub percent: Option<u8>,
    pub charging: Option<bool>,
    pub seconds_remaining: Option<u32>,
    pub battery_saver: bool,
}

const BATTERY_FLAG_CHARGING: u8 = 8;
const BATTERY_FLAG_NO_BATTERY: u8 = 128;
const BATTERY_FLAG_UNKNOWN: u8 = 255;
const UNKNOWN_BYTE: u8 = 255;
const UNKNOWN_SECONDS: u32 = u32::MAX;

/// Maps the raw `SYSTEM_POWER_STATUS` fields. A machine with no system battery
/// (flag 128) is `Unavailable`, not an error; flag 255 ("unable to read") is
/// also `Unavailable`, with a different reason, and carries no values.
pub fn map_system_power_status(
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
) -> PowerStatusReading {
    let on_ac = match ac_line_status {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    };
    let battery_saver = system_status_flag == 1;
    let none = |availability| PowerStatusReading {
        availability,
        on_ac,
        percent: None,
        charging: None,
        seconds_remaining: None,
        battery_saver,
    };
    if battery_flag == BATTERY_FLAG_UNKNOWN {
        return none(Availability::unavailable(
            "battery status unknown (GetSystemPowerStatus BatteryFlag=255)",
        ));
    }
    if battery_flag & BATTERY_FLAG_NO_BATTERY != 0 {
        return none(Availability::unavailable(
            "no system battery (GetSystemPowerStatus BatteryFlag=128)",
        ));
    }
    PowerStatusReading {
        availability: Availability::Supported,
        on_ac,
        percent: (battery_life_percent != UNKNOWN_BYTE).then_some(battery_life_percent.min(100)),
        charging: Some(battery_flag & BATTERY_FLAG_CHARGING != 0),
        seconds_remaining: (battery_life_time != UNKNOWN_SECONDS).then_some(battery_life_time),
        battery_saver,
    }
}

/// `Windows.System.Power.BatteryStatus` (also used by `BatteryReport.Status`).
pub fn winrt_battery_status(value: i32) -> (&'static str, Availability) {
    match value {
        0 => (
            "not-present",
            Availability::unavailable("no battery (BatteryStatus.NotPresent)"),
        ),
        1 => ("discharging", Availability::Supported),
        2 => ("idle", Availability::Supported),
        3 => ("charging", Availability::Supported),
        _ => (
            "unknown",
            Availability::unavailable(format!("unknown BatteryStatus value {value}")),
        ),
    }
}

/// Decoded `NLM_CONNECTIVITY` bit set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connectivity {
    pub ipv4: &'static str,
    pub ipv6: &'static str,
    pub internet: bool,
    pub any_network: bool,
}

pub fn decode_nlm_connectivity(flags: i32) -> Connectivity {
    fn level(flags: i32, no_traffic: i32, subnet: i32, local: i32, internet: i32) -> &'static str {
        if flags & internet != 0 {
            "internet"
        } else if flags & local != 0 {
            "local-network"
        } else if flags & subnet != 0 {
            "subnet"
        } else if flags & no_traffic != 0 {
            "no-traffic"
        } else {
            "disconnected"
        }
    }
    let ipv4 = level(flags, 0x1, 0x10, 0x20, 0x40);
    let ipv6 = level(flags, 0x2, 0x100, 0x200, 0x400);
    Connectivity {
        ipv4,
        ipv6,
        internet: flags & (0x40 | 0x400) != 0,
        any_network: flags & (0x10 | 0x20 | 0x40 | 0x100 | 0x200 | 0x400) != 0,
    }
}

/// `Windows.Networking.Connectivity.NetworkConnectivityLevel`.
pub fn winrt_connectivity_level(value: i32) -> &'static str {
    match value {
        0 => "none",
        1 => "local-access",
        2 => "constrained-internet-access",
        3 => "internet-access",
        _ => "unknown",
    }
}

/// `Windows.Networking.Connectivity.NetworkCostType`.
pub fn winrt_cost_type(value: i32) -> &'static str {
    match value {
        0 => "unknown",
        1 => "unrestricted",
        2 => "fixed",
        3 => "variable",
        _ => "other",
    }
}

/// IANA ifType values seen on Windows desktops.
pub fn iana_interface_type(value: u32) -> &'static str {
    match value {
        6 => "ethernet",
        71 => "wifi",
        243 | 244 => "mobile-broadband",
        24 => "loopback",
        131 => "tunnel",
        _ => "other",
    }
}

/// What the SSID sub-capability reports. Since Windows 11 24H2, reading the
/// connected SSID needs the location privacy permission; `consent` is the
/// `CapabilityAccessManager\ConsentStore\location` value ("Allow"/"Deny") for
/// the machine, the user and unpackaged desktop apps, `None` when unreadable.
pub fn ssid_availability(
    is_wlan: bool,
    ssid_len: Option<usize>,
    machine: Option<&str>,
    user: Option<&str>,
    non_packaged: Option<&str>,
) -> Availability {
    if !is_wlan {
        return Availability::unavailable("internet profile is not a WLAN connection");
    }
    if matches!(ssid_len, Some(len) if len > 0) {
        return Availability::Supported;
    }
    let denied = [machine, user, non_packaged]
        .into_iter()
        .flatten()
        .any(|v| v.eq_ignore_ascii_case("Deny"));
    if denied {
        Availability::permission_required("location privacy setting denies SSID access")
    } else {
        Availability::unavailable("WLAN profile returned no SSID with location consent not denied")
    }
}

/// Short, stable, non-reversible tag for a device or endpoint ID so reports
/// can compare IDs across restarts without publishing hardware identifiers.
pub fn fingerprint(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", hash >> 32)
}

/// Overall provider availability from its read and write sub-capabilities:
/// read+write = supported, read only = read-only, otherwise the read status.
pub fn provider_availability(read: &Availability, write: Option<&Availability>) -> Availability {
    match (read, write) {
        (Availability::Supported, Some(Availability::Supported)) => Availability::Supported,
        (Availability::Supported, _) => Availability::ReadOnly,
        (other, _) => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parses_smoke_and_rejects_bad_input() {
        assert_eq!(
            parse_args(&args(&["--smoke", "7"])).unwrap().smoke_seconds,
            7
        );
        assert!(parse_args(&args(&["--help"])).unwrap().help);
        assert!(parse_args(&args(&["--smoke"])).is_err());
        assert!(parse_args(&args(&["--smoke", "0"])).is_err());
        assert!(parse_args(&args(&["--smoke", "121"])).is_err());
        assert!(parse_args(&args(&["--smoke", "x"])).is_err());
        assert!(parse_args(&args(&["--probe"])).is_err());
    }

    #[test]
    fn hresults_map_to_distinct_kinds() {
        assert_eq!(
            classify_hresult(E_ACCESSDENIED),
            ErrorKind::PermissionDenied
        );
        assert_eq!(classify_hresult(E_NOTFOUND), ErrorKind::Unavailable);
        assert_eq!(
            classify_hresult(AUDCLNT_E_DEVICE_INVALIDATED),
            ErrorKind::Unavailable
        );
        assert_eq!(
            classify_hresult(AUDCLNT_E_SERVICE_NOT_RUNNING),
            ErrorKind::Unavailable
        );
        assert_eq!(
            classify_hresult(REGDB_E_CLASSNOTREG),
            ErrorKind::Unsupported
        );
        assert_eq!(classify_hresult(E_NOINTERFACE), ErrorKind::Unsupported);
        assert_eq!(classify_hresult(RPC_E_TIMEOUT), ErrorKind::Timeout);
        assert_eq!(classify_hresult(E_WIN32_TIMEOUT), ErrorKind::Timeout);
        // Programming errors are failures, never "unavailable".
        assert_eq!(classify_hresult(CO_E_NOTINITIALIZED), ErrorKind::Failure);
        assert_eq!(classify_hresult(RPC_E_WRONG_THREAD), ErrorKind::Failure);
        assert_eq!(classify_hresult(E_INVALIDARG), ErrorKind::Failure);
        assert_eq!(classify_hresult(0x8000_FFFF), ErrorKind::Failure);
    }

    #[test]
    fn activation_on_non_active_endpoint_is_unavailable() {
        // NOTPRESENT (4) / UNPLUGGED (8) endpoints: both observed HRESULTs.
        assert_eq!(
            classify_endpoint_activation(AUDCLNT_E_DEVICE_INVALIDATED, 4),
            ErrorKind::Unavailable
        );
        assert_eq!(
            classify_endpoint_activation(E_PATH_NOT_FOUND, 4),
            ErrorKind::Unavailable
        );
        assert_eq!(
            classify_endpoint_activation(E_ACCESSDENIED, 8),
            ErrorKind::PermissionDenied
        );
        // ACTIVE endpoint: generic rules, so an odd HRESULT stays a failure.
        assert_eq!(
            classify_endpoint_activation(E_PATH_NOT_FOUND, 1),
            ErrorKind::Failure
        );
        assert_eq!(
            classify_endpoint_activation(AUDCLNT_E_DEVICE_INVALIDATED, 1),
            ErrorKind::Unavailable
        );
    }

    #[test]
    fn nudge_flag_parses() {
        assert!(
            parse_args(&args(&["--smoke", "3", "--nudge"]))
                .unwrap()
                .nudge
        );
        assert!(!parse_args(&args(&[])).unwrap().nudge);
    }

    #[test]
    fn errors_only_become_statuses_when_they_mean_one() {
        let denied = ProviderError::from_hresult("SetMasterVolumeLevelScalar", E_ACCESSDENIED, "x");
        assert_eq!(
            denied.availability().unwrap().label(),
            "permission-required"
        );
        let absent = ProviderError::from_hresult("GetDefaultAudioEndpoint", E_NOTFOUND, "x");
        assert_eq!(absent.availability().unwrap().label(), "unavailable");
        let unsupported = ProviderError::from_hresult("CoCreateInstance", REGDB_E_CLASSNOTREG, "x");
        assert_eq!(unsupported.availability().unwrap().label(), "unavailable");
        let timeout = ProviderError::new(ErrorKind::Timeout, "callback", "x");
        assert_eq!(timeout.availability(), None);
        let failure = ProviderError::from_hresult("CoCreateInstance", CO_E_NOTINITIALIZED, "x");
        assert_eq!(failure.availability(), None);
        assert_eq!(failure.to_json()["hresult"], "0x800401F0");
        assert_eq!(failure.to_json()["hresult_name"], "CO_E_NOTINITIALIZED");
    }

    #[test]
    fn desktop_without_battery_is_unavailable_not_an_error() {
        let r = map_system_power_status(1, 128, 255, 0, u32::MAX);
        assert_eq!(r.availability.label(), "unavailable");
        assert_eq!(r.on_ac, Some(true));
        assert_eq!(r.percent, None);
        assert_eq!(r.charging, None);
        let unknown = map_system_power_status(255, 255, 255, 0, u32::MAX);
        assert_eq!(unknown.availability.label(), "unavailable");
        assert_eq!(unknown.on_ac, None);
        assert_ne!(r.availability, unknown.availability);
    }

    #[test]
    fn laptop_battery_maps_values() {
        // On AC, high charge, charging, time unknown.
        let r = map_system_power_status(1, 1 | 8, 78, 0, u32::MAX);
        assert_eq!(r.availability, Availability::Supported);
        assert_eq!(
            (r.on_ac, r.percent, r.charging),
            (Some(true), Some(78), Some(true))
        );
        assert_eq!(r.seconds_remaining, None);
        // On battery, low, saver on, 30 minutes left.
        let r = map_system_power_status(0, 2, 20, 1, 1800);
        assert_eq!(
            (r.on_ac, r.charging, r.battery_saver),
            (Some(false), Some(false), true)
        );
        assert_eq!(r.seconds_remaining, Some(1800));
        // Unknown percent stays unknown instead of becoming 255 %.
        assert_eq!(map_system_power_status(1, 1, 255, 0, 0).percent, None);
    }

    #[test]
    fn winrt_battery_status_maps_not_present_to_unavailable() {
        assert_eq!(winrt_battery_status(0).1.label(), "unavailable");
        assert_eq!(
            winrt_battery_status(3),
            ("charging", Availability::Supported)
        );
        assert_eq!(winrt_battery_status(9).1.label(), "unavailable");
    }

    #[test]
    fn nlm_flags_decode() {
        let c = decode_nlm_connectivity(0x40 | 0x2);
        assert_eq!(
            (c.ipv4, c.ipv6, c.internet, c.any_network),
            ("internet", "no-traffic", true, true)
        );
        let c = decode_nlm_connectivity(0);
        assert_eq!(
            (c.ipv4, c.ipv6, c.internet, c.any_network),
            ("disconnected", "disconnected", false, false)
        );
        let c = decode_nlm_connectivity(0x20 | 0x200);
        assert_eq!(
            (c.ipv4, c.ipv6, c.internet),
            ("local-network", "local-network", false)
        );
        assert!(c.any_network);
    }

    #[test]
    fn small_enums_map() {
        assert_eq!(winrt_connectivity_level(3), "internet-access");
        assert_eq!(winrt_connectivity_level(7), "unknown");
        assert_eq!(winrt_cost_type(1), "unrestricted");
        assert_eq!(iana_interface_type(71), "wifi");
        assert_eq!(iana_interface_type(6), "ethernet");
    }

    #[test]
    fn ssid_needs_location_consent() {
        assert_eq!(
            ssid_availability(false, None, None, None, None).label(),
            "unavailable"
        );
        assert_eq!(
            ssid_availability(true, Some(8), Some("Allow"), Some("Allow"), Some("Allow")),
            Availability::Supported
        );
        assert_eq!(
            ssid_availability(true, Some(0), Some("Allow"), Some("Deny"), None).label(),
            "permission-required"
        );
        assert_eq!(
            ssid_availability(true, None, Some("Allow"), Some("Allow"), Some("Allow")).label(),
            "unavailable"
        );
    }

    #[test]
    fn provider_level_combines_read_and_write() {
        let s = Availability::Supported;
        assert_eq!(provider_availability(&s, Some(&s)), Availability::Supported);
        assert_eq!(provider_availability(&s, None), Availability::ReadOnly);
        let denied = Availability::permission_required("x");
        assert_eq!(
            provider_availability(&s, Some(&denied)),
            Availability::ReadOnly
        );
        let gone = Availability::unavailable("no battery");
        assert_eq!(provider_availability(&gone, None), gone);
    }

    #[test]
    fn availability_json_carries_reason() {
        assert_eq!(
            Availability::Supported.to_json(),
            json!({"status": "supported"})
        );
        assert_eq!(
            Availability::unavailable("no battery").to_json(),
            json!({"status": "unavailable", "reason": "no battery"})
        );
    }

    #[test]
    fn fingerprint_is_stable_and_short() {
        assert_eq!(fingerprint("abc"), fingerprint("abc"));
        assert_ne!(fingerprint("abc"), fingerprint("abd"));
        assert_eq!(fingerprint("{0.0.0.00000000}.{x}").len(), 8);
    }
}
