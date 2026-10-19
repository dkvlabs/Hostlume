//! Remote projection (security-privacy.md §6/§7): what leaves the machine.
//!
//! Pure functions from the normalized snapshot to route payloads. The
//! privacy rule is structural, not per-request: **only numeric readings
//! are ever projected** on family and snapshot routes, so interface
//! addresses, filesystem names, Bluetooth device names and the raw
//! hostname cannot leak through them. `/system` is the single, explicit
//! identity projection (hostname, OS version, architecture, uptime).
//! Processes reuse the already-sanitized page shape (pid, name, CPU,
//! RSS — no paths, no command lines).

use axum::Json;
use monitor_core::{Availability, MetricReading, MetricValue, SystemSnapshot};
use serde::Serialize;

/// A numeric reading as served remotely: the canonical shape, guaranteed
/// `available` with a finite number by the filter below.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RemoteReading {
    pub metric_id: String,
    pub entity_id: String,
    pub value: f64,
    pub unit: &'static str,
    pub collected_at_utc: String,
    pub sequence: u64,
}

pub fn numeric_readings(snapshot: &SystemSnapshot) -> Vec<RemoteReading> {
    snapshot.metrics.iter().filter_map(remote_reading).collect()
}

fn remote_reading(reading: &MetricReading) -> Option<RemoteReading> {
    // Contract invariant: only `available` readings carry values, and the
    // engine never fabricates — but the filter is defensive anyway.
    if reading.availability != Availability::Available {
        return None;
    }
    match reading.value {
        Some(MetricValue::Number(value)) if value.is_finite() => Some(RemoteReading {
            metric_id: reading.metric_id.clone(),
            entity_id: reading.entity_id.clone(),
            value,
            unit: reading.unit.as_canonical_str(),
            collected_at_utc: reading.collected_at_utc.clone(),
            sequence: reading.monotonic_sequence,
        }),
        _ => None,
    }
}

fn family_readings(snapshot: &SystemSnapshot, family: &str) -> Vec<RemoteReading> {
    numeric_readings(snapshot)
        .into_iter()
        .filter(|reading| reading.metric_id.starts_with(family))
        .collect()
}

/// `(route, family prefix)` for every per-family route. Routes the phase
/// whitelist names but no family exists for (e.g. `gpu` on unsupported
/// hardware) simply serve an empty, honest list.
pub const FAMILY_ROUTES: &[(&str, &str)] = &[
    ("cpu", "cpu."),
    ("memory", "memory."),
    ("disks", "disk."),
    ("network", "net."),
    ("gpu", "gpu."),
    ("sensors", "sensor."),
    ("battery", "battery."),
    ("bluetooth", "bluetooth."),
];

#[derive(Debug, Serialize)]
pub struct FamilyPayload {
    pub collected_at_utc: String,
    pub sequence: u64,
    pub readings: Vec<RemoteReading>,
}

pub fn family_payload(snapshot: &SystemSnapshot, family: &str) -> Json<FamilyPayload> {
    Json(FamilyPayload {
        collected_at_utc: snapshot.collected_at_utc.clone(),
        sequence: snapshot.sequence,
        readings: family_readings(snapshot, family),
    })
}

pub fn snapshot_payload(snapshot: &SystemSnapshot) -> Json<SnapshotPayload> {
    Json(SnapshotPayload {
        schema_version: snapshot.schema_version,
        sequence: snapshot.sequence,
        collected_at_utc: snapshot.collected_at_utc.clone(),
        readings: numeric_readings(snapshot),
    })
}

#[derive(Debug, Serialize)]
pub struct SnapshotPayload {
    pub schema_version: u8,
    pub sequence: u64,
    pub collected_at_utc: String,
    pub readings: Vec<RemoteReading>,
}

/// The single identity projection (design.md §14 "explicit host badge").
/// Everything here is user-visible system description, not hardware
/// identity: no serials, no MACs, no usernames, no paths.
#[derive(Debug, Serialize)]
pub struct SystemPayload {
    pub collected_at_utc: String,
    pub hostname: Option<String>,
    pub os_version: Option<String>,
    pub arch: Option<String>,
    pub uptime_seconds: Option<f64>,
    pub logical_cores: Option<f64>,
}

pub fn system_payload(snapshot: &SystemSnapshot) -> Json<SystemPayload> {
    let text = |metric_id: &str| {
        snapshot.metrics.iter().find_map(|r| {
            if r.metric_id == metric_id {
                match &r.value {
                    Some(MetricValue::Text(text)) => Some(text.clone()),
                    _ => None,
                }
            } else {
                None
            }
        })
    };
    let number = |metric_id: &str| {
        snapshot.metrics.iter().find_map(|r| {
            if r.metric_id == metric_id {
                match r.value {
                    Some(MetricValue::Number(value)) => Some(value),
                    _ => None,
                }
            } else {
                None
            }
        })
    };
    use monitor_core::families;
    Json(SystemPayload {
        collected_at_utc: snapshot.collected_at_utc.clone(),
        hostname: text(families::HOST_HOSTNAME),
        os_version: text(families::HOST_OS_VERSION),
        arch: text(families::HOST_ARCH),
        uptime_seconds: number(families::HOST_UPTIME),
        logical_cores: number(families::HOST_CPU_CORES_LOGICAL),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use monitor_core::MetricUnit;

    fn snapshot() -> SystemSnapshot {
        let mut snapshot = SystemSnapshot::empty("h", "g", 7, "2026-10-19T00:00:00.000Z");
        let mut push = |metric_id: &str, value: MetricValue| {
            snapshot.metrics.push(MetricReading {
                metric_id: metric_id.to_string(),
                entity_id: "host".to_string(),
                value: Some(value),
                unit: MetricUnit::Percent,
                availability: Availability::Available,
                source: "test".into(),
                collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
                monotonic_sequence: 7,
                reason_code: None,
                human_message_key: None,
                precision: None,
            });
        };
        push("cpu.total.usage", MetricValue::Number(42.5));
        push("host.hostname", MetricValue::Text("my-macbook".into()));
        push(
            "net.interface.ipv4",
            MetricValue::Text("192.168.1.10".into()),
        );
        push("host.arch", MetricValue::Text("arm64".into()));
        push("host.uptime", MetricValue::Number(123.0));
        // Unavailable readings must never project.
        snapshot.metrics.push(MetricReading {
            metric_id: "sensor.temperature".into(),
            entity_id: "sensor:0".into(),
            value: None,
            unit: MetricUnit::Celsius,
            availability: Availability::Unsupported,
            source: "test".into(),
            collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
            monotonic_sequence: 7,
            reason_code: Some("NO_SENSOR".into()),
            human_message_key: None,
            precision: None,
        });
        snapshot
    }

    /// Phase 08 §4: "Remote JSON excludes … network hardware addresses by
    /// default" — and every other text metric, everywhere except /system.
    #[test]
    fn numeric_projection_excludes_all_text_metrics() {
        let projected = numeric_readings(&snapshot());
        assert!(projected
            .iter()
            .all(|r| !r.metric_id.starts_with("host.hostname")));
        assert!(projected
            .iter()
            .all(|r| r.metric_id != "net.interface.ipv4"));
        let ids: Vec<&str> = projected.iter().map(|r| r.metric_id.as_str()).collect();
        assert_eq!(ids, vec!["cpu.total.usage", "host.uptime"]);
    }

    #[test]
    fn family_routes_group_by_prefix_and_stay_honest_when_empty() {
        let cpu = family_readings(&snapshot(), "cpu.");
        assert_eq!(cpu.len(), 1);
        assert_eq!(cpu[0].value, 42.5);
        // No sensor family on this fixture: empty list, never fake data.
        assert!(family_readings(&snapshot(), "sensor.").is_empty());
    }

    #[test]
    fn system_projection_is_the_only_identity_surface() {
        let payload = system_payload(&snapshot()).0;
        assert_eq!(payload.hostname.as_deref(), Some("my-macbook"));
        assert_eq!(payload.arch.as_deref(), Some("arm64"));
        assert_eq!(payload.uptime_seconds, Some(123.0));
        // The full snapshot payload carries no hostname.
        let snapshot_all = serde_json::to_string(&snapshot_payload(&snapshot()).0).unwrap();
        assert!(!snapshot_all.contains("my-macbook"));
        assert!(!snapshot_all.contains("192.168.1.10"));
    }
}
