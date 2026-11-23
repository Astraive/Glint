//! Battery / power source: Win32 `GetSystemPowerStatus` (no COM needed),
//! WinRT `Windows.System.Power.PowerManager` (status + change events) and
//! `Windows.Devices.Power.Battery.AggregateBattery` (`BatteryReport`,
//! `ReportUpdated`). A machine without a battery maps to `Unavailable`.

use super::{CapabilityRow, Recorder, perr, result_json, timed};
use crate::model::{self, Availability, ProviderError};
use serde_json::{Value, json};
use std::sync::Arc;
use windows::Devices::Power::Battery;
use windows::Foundation::{EventHandler, TypedEventHandler};
use windows::System::Power::PowerManager;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::core::IInspectable;

const PM_OBSERVE: &str = "battery.power-manager.observe";
const REPORT_OBSERVE: &str = "battery.report.observe";
pub const OBSERVE_LABELS: &[&str] = &[PM_OBSERVE, REPORT_OBSERVE];

pub fn winrt_battery_status_result() -> Result<i32, ProviderError> {
    PowerManager::BatteryStatus()
        .map(|s| s.0)
        .map_err(|e| perr("PowerManager::BatteryStatus", &e))
}

fn system_power_status() -> Result<SYSTEM_POWER_STATUS, ProviderError> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: valid out-pointer.
    unsafe { GetSystemPowerStatus(&mut status) }.map_err(|e| perr("GetSystemPowerStatus", &e))?;
    Ok(status)
}

fn power_manager() -> Result<Value, ProviderError> {
    let op = |name: &'static str| move |e: windows::core::Error| perr(name, &e);
    let battery = PowerManager::BatteryStatus().map_err(op("PowerManager::BatteryStatus"))?;
    let supply =
        PowerManager::PowerSupplyStatus().map_err(op("PowerManager::PowerSupplyStatus"))?;
    let percent = PowerManager::RemainingChargePercent()
        .map_err(op("PowerManager::RemainingChargePercent"))?;
    let discharge = PowerManager::RemainingDischargeTime()
        .map_err(op("PowerManager::RemainingDischargeTime"))?;
    let saver = PowerManager::EnergySaverStatus().map_err(op("PowerManager::EnergySaverStatus"))?;
    let (status_name, availability) = model::winrt_battery_status(battery.0);
    // RemainingDischargeTime is a TimeSpan in 100 ns units; huge when on AC.
    let discharge_s = discharge.Duration / 10_000_000;
    Ok(json!({
        "battery_status": status_name,
        "battery_status_raw": battery.0,
        "mapped": availability.to_json(),
        "power_supply": match supply.0 { 0 => "not-present", 1 => "inadequate", 2 => "adequate", _ => "unknown" },
        "remaining_charge_percent": percent,
        "remaining_discharge_seconds": (discharge_s < 10 * 24 * 3600).then_some(discharge_s),
        "energy_saver": match saver.0 { 0 => "disabled", 1 => "off", 2 => "on", _ => "unknown" },
    }))
}

fn aggregate_report() -> Result<Value, ProviderError> {
    let battery = Battery::AggregateBattery().map_err(|e| perr("Battery::AggregateBattery", &e))?;
    let report = battery
        .GetReport()
        .map_err(|e| perr("Battery::GetReport", &e))?;
    let status = report
        .Status()
        .map_err(|e| perr("BatteryReport::Status", &e))?;
    // Capacity fields are IReference<i32>: null on machines without a battery.
    let full = report
        .FullChargeCapacityInMilliwattHours()
        .and_then(|r| r.Value())
        .ok();
    let remaining = report
        .RemainingCapacityInMilliwattHours()
        .and_then(|r| r.Value())
        .ok();
    let rate = report.ChargeRateInMilliwatts().and_then(|r| r.Value()).ok();
    let (status_name, availability) = model::winrt_battery_status(status.0);
    Ok(json!({
        "status": status_name,
        "mapped": availability.to_json(),
        "full_charge_mwh": full,
        "remaining_mwh": remaining,
        "charge_rate_mw": rate,
        "percent_from_capacity": match (full, remaining) {
            (Some(f), Some(r)) if f > 0 => Some(super::round(f64::from(r) * 100.0 / f64::from(f))),
            _ => None,
        },
    }))
}

/// Registered WinRT event tokens (`PowerManager` statics + aggregate battery).
struct Subscriptions {
    battery: Option<Battery>,
    tokens: Vec<(&'static str, Result<i64, ProviderError>)>,
}

fn subscribe(label: &'static str, rec: &Arc<Recorder>) -> Subscriptions {
    let handler = |event: &'static str| {
        let rec = Arc::clone(rec);
        EventHandler::<IInspectable>::new(move |_, _| {
            rec.record(label, json!({ "event": event }));
            Ok(())
        })
    };
    let mut tokens = vec![
        (
            "BatteryStatusChanged",
            PowerManager::BatteryStatusChanged(&handler("battery-status"))
                .map_err(|e| perr("PowerManager::BatteryStatusChanged", &e)),
        ),
        (
            "PowerSupplyStatusChanged",
            PowerManager::PowerSupplyStatusChanged(&handler("power-supply"))
                .map_err(|e| perr("PowerManager::PowerSupplyStatusChanged", &e)),
        ),
        (
            "RemainingChargePercentChanged",
            PowerManager::RemainingChargePercentChanged(&handler("charge-percent"))
                .map_err(|e| perr("PowerManager::RemainingChargePercentChanged", &e)),
        ),
        (
            "EnergySaverStatusChanged",
            PowerManager::EnergySaverStatusChanged(&handler("energy-saver"))
                .map_err(|e| perr("PowerManager::EnergySaverStatusChanged", &e)),
        ),
    ];
    let battery = Battery::AggregateBattery().ok();
    if let Some(b) = &battery {
        let rec = Arc::clone(rec);
        let report_label = if label == PM_OBSERVE {
            REPORT_OBSERVE
        } else {
            label
        };
        let typed = TypedEventHandler::<Battery, IInspectable>::new(move |_, _| {
            rec.record(report_label, json!({ "event": "report-updated" }));
            Ok(())
        });
        tokens.push((
            "Battery.ReportUpdated",
            b.ReportUpdated(&typed)
                .map_err(|e| perr("Battery::ReportUpdated", &e)),
        ));
    }
    Subscriptions { battery, tokens }
}

fn unsubscribe(subs: &Subscriptions) -> Value {
    let remove = |name: &str, token: i64| -> Result<(), ProviderError> {
        let r = match name {
            "BatteryStatusChanged" => PowerManager::RemoveBatteryStatusChanged(token),
            "PowerSupplyStatusChanged" => PowerManager::RemovePowerSupplyStatusChanged(token),
            "RemainingChargePercentChanged" => {
                PowerManager::RemoveRemainingChargePercentChanged(token)
            }
            "EnergySaverStatusChanged" => PowerManager::RemoveEnergySaverStatusChanged(token),
            _ => match &subs.battery {
                Some(b) => b.RemoveReportUpdated(token),
                None => Ok(()),
            },
        };
        r.map_err(|e| perr(name, &e))
    };
    let mut out = serde_json::Map::new();
    for (name, token) in &subs.tokens {
        if let Ok(token) = token {
            let first = remove(name, *token);
            let second = remove(name, *token);
            out.insert(
                (*name).to_string(),
                json!({ "remove": result_json(&first), "remove_again": result_json(&second) }),
            );
        }
    }
    Value::Object(out)
}

pub struct Session {
    win32: Result<SYSTEM_POWER_STATUS, ProviderError>,
    win32_ms: f64,
    pm: Result<Value, ProviderError>,
    pm_ms: f64,
    report: Result<Value, ProviderError>,
    report_ms: f64,
    subs: Subscriptions,
}

impl Session {
    pub fn start(rec: &Arc<Recorder>) -> Self {
        let (win32, win32_ms) = timed(system_power_status);
        let (pm, pm_ms) = timed(power_manager);
        let (report, report_ms) = timed(aggregate_report);
        Self {
            win32,
            win32_ms,
            pm,
            pm_ms,
            report,
            report_ms,
            subs: subscribe(PM_OBSERVE, rec),
        }
    }

    pub fn stop(&mut self) -> Value {
        unsubscribe(&self.subs)
    }

    fn mapped(&self) -> Option<model::PowerStatusReading> {
        self.win32.as_ref().ok().map(|s| {
            model::map_system_power_status(
                s.ACLineStatus,
                s.BatteryFlag,
                s.BatteryLifePercent,
                s.SystemStatusFlag,
                s.BatteryLifeTime,
            )
        })
    }

    pub fn report(&self, stop: Value) -> Value {
        let win32 = match (&self.win32, self.mapped()) {
            (Ok(s), Some(m)) => json!({
                "raw": {
                    "ac_line_status": s.ACLineStatus,
                    "battery_flag": s.BatteryFlag,
                    "battery_life_percent": s.BatteryLifePercent,
                    "system_status_flag": s.SystemStatusFlag,
                    "battery_life_time": s.BatteryLifeTime,
                },
                "mapped": {
                    "availability": m.availability.to_json(),
                    "on_ac": m.on_ac,
                    "percent": m.percent,
                    "charging": m.charging,
                    "seconds_remaining": m.seconds_remaining,
                    "battery_saver": m.battery_saver,
                },
            }),
            (Err(e), _) => json!({ "error": e.to_json() }),
            _ => Value::Null,
        };
        json!({
            "api": {
                "snapshot": "Win32 GetSystemPowerStatus",
                "events_and_detail": "WinRT Windows.System.Power.PowerManager + Windows.Devices.Power.Battery.AggregateBattery",
            },
            "get_system_power_status": win32,
            "get_system_power_status_ms": self.win32_ms,
            "power_manager": match &self.pm { Ok(v) => v.clone(), Err(e) => json!({ "error": e.to_json() }) },
            "power_manager_ms": self.pm_ms,
            "aggregate_battery_report": match &self.report { Ok(v) => v.clone(), Err(e) => json!({ "error": e.to_json() }) },
            "aggregate_battery_report_ms": self.report_ms,
            "subscriptions": self.subs.tokens.iter().map(|(n, t)| (n.to_string(), result_json(t))).collect::<serde_json::Map<_, _>>(),
            "stop": stop,
        })
    }

    pub fn capabilities(&self) -> Vec<CapabilityRow> {
        let mapped = self.mapped();
        let read = match (&self.win32, &mapped) {
            (Ok(_), Some(m)) => {
                let winrt = self.pm.as_ref().ok().map(|v| v["battery_status"].clone());
                CapabilityRow::new(
                    "system.battery.read",
                    Ok(m.availability.clone()),
                    format!(
                        "GetSystemPowerStatus percent {:?} charging {:?}; PowerManager says {}",
                        m.percent,
                        m.charging,
                        winrt.unwrap_or(Value::Null)
                    ),
                )
            }
            (Err(e), _) => {
                CapabilityRow::from_error("system.battery.read", e.clone(), "GetSystemPowerStatus")
            }
            _ => CapabilityRow::new(
                "system.battery.read",
                Ok(Availability::unavailable("no reading")),
                "",
            ),
        };
        let all_subscribed = self.subs.tokens.iter().all(|(_, t)| t.is_ok());
        let events = match (&read.status, all_subscribed) {
            (Ok(Availability::Supported), true) => CapabilityRow::new(
                "system.battery.events",
                Ok(Availability::Supported),
                format!("{} WinRT subscriptions registered", self.subs.tokens.len()),
            ),
            (Ok(Availability::Supported), false) => {
                let err = self
                    .subs
                    .tokens
                    .iter()
                    .find_map(|(_, t)| t.as_ref().err().cloned())
                    .unwrap_or_else(|| {
                        ProviderError::new(crate::model::ErrorKind::Failure, "subscribe", "unknown")
                    });
                CapabilityRow::from_error("system.battery.events", err, "WinRT subscription")
            }
            (status, _) => CapabilityRow::new(
                "system.battery.events",
                status.clone(),
                "follows system.battery.read: no battery, no battery events",
            ),
        };
        let source = match &mapped {
            Some(m) if m.on_ac.is_some() => CapabilityRow::new(
                "system.power.source.read",
                Ok(Availability::Supported),
                format!("on AC: {:?} (reported with or without a battery)", m.on_ac),
            ),
            Some(_) => CapabilityRow::new(
                "system.power.source.read",
                Ok(Availability::unavailable("ACLineStatus unknown (255)")),
                "GetSystemPowerStatus",
            ),
            None => CapabilityRow::new(
                "system.power.source.read",
                read.status.clone(),
                "GetSystemPowerStatus failed",
            ),
        };
        let provider = match &read.status {
            Ok(r) => Ok(model::provider_availability(r, None)),
            Err(e) => Err(e.clone()),
        };
        vec![
            CapabilityRow::new(
                "provider.battery",
                provider,
                "read-only: no write capability exists",
            ),
            read,
            events,
            source,
        ]
    }
}

pub fn restart_checks(rec: &Arc<Recorder>) -> Value {
    const LABEL: &str = "battery.restart";
    let first = subscribe(LABEL, rec);
    let first_stop = unsubscribe(&first);
    drop(first);
    let (second, resubscribe_ms) = timed(|| subscribe(LABEL, rec));
    let tokens: Vec<Value> = second
        .tokens
        .iter()
        .map(|(n, t)| json!({ "event": n, "ok": t.is_ok() }))
        .collect();
    let second_stop = unsubscribe(&second);
    let fresh_battery = Battery::AggregateBattery()
        .and_then(|b| b.GetReport())
        .and_then(|r| r.Status())
        .map(|s| model::winrt_battery_status(s.0).0)
        .map_err(|e| perr("AggregateBattery after resubscribe", &e));
    json!({
        "first_unsubscribe": first_stop,
        "resubscribe_ms": resubscribe_ms,
        "resubscribed": tokens,
        "second_unsubscribe": second_stop,
        "fresh_aggregate_status": match fresh_battery { Ok(s) => json!(s), Err(e) => e.to_json() },
        "callbacks_seen": rec.count(LABEL),
    })
}
