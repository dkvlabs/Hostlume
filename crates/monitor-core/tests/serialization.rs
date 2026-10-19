//! Contract serialization tests: golden fixture, round trips, invariant
//! enforcement and the schema version guard (phase 01 §3 test list).

use monitor_core::{
    check_schema_version, Availability, CapabilityReport, ContractViolation, MetricCapability,
    MetricReading, MetricUnit, MetricValue, SchemaError, SystemSnapshot, SCHEMA_VERSION,
};

fn fixture_json() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/snapshot_v1.json"
    ))
    .expect("fixture file")
}

fn sample_reading() -> MetricReading {
    MetricReading {
        metric_id: "cpu.total.usage".into(),
        entity_id: "host".into(),
        value: Some(MetricValue::Number(12.5)),
        unit: MetricUnit::Percent,
        availability: Availability::Available,
        source: "test".into(),
        collected_at_utc: "2026-10-19T00:00:00.000Z".into(),
        monotonic_sequence: 1,
        reason_code: None,
        human_message_key: None,
        precision: Some(1),
    }
}

#[test]
fn golden_fixture_parses_validates_and_reserializes_identically() {
    let raw = fixture_json();
    let snapshot: SystemSnapshot = serde_json::from_str(&raw).expect("fixture parses");
    snapshot.validate().expect("fixture is contract-consistent");
    assert_eq!(snapshot.schema_version, SCHEMA_VERSION);
    assert_eq!(snapshot.metrics.len(), 3);

    let parsed_again: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let reserialized = serde_json::to_value(&snapshot).unwrap();
    assert_eq!(parsed_again, reserialized, "serialization shape drifted");
}

#[test]
fn snapshot_json_roundtrip_preserves_all_fields() {
    let snapshot = SystemSnapshot {
        schema_version: SCHEMA_VERSION,
        host_id: "11111111-2222-4333-8444-555566667777".into(),
        generation: "gen-1".into(),
        sequence: 7,
        collected_at_utc: "2026-10-19T01:02:03.004Z".into(),
        metrics: vec![
            sample_reading(),
            MetricReading {
                availability: Availability::PermissionRequired,
                value: None,
                reason_code: Some("PERMISSION_DENIED".into()),
                human_message_key: Some("states.permissionRequired".into()),
                metric_id: "sensor.temperature".into(),
                entity_id: "sensor:smc".into(),
                unit: MetricUnit::Celsius,
                source: "test".into(),
                collected_at_utc: "2026-10-19T01:02:03.004Z".into(),
                monotonic_sequence: 7,
                precision: None,
            },
        ],
        capabilities_revision: 2,
    };
    let json = serde_json::to_string(&snapshot).unwrap();
    let back: SystemSnapshot = serde_json::from_str(&json).unwrap();
    assert_eq!(back, snapshot);
}

#[test]
fn unavailable_reading_with_value_is_rejected() {
    let mut reading = sample_reading();
    reading.availability = Availability::Unsupported;
    assert_eq!(
        reading.check_invariants(),
        Err(ContractViolation::UnavailableRequiresNull)
    );
}

#[test]
fn available_reading_without_value_is_rejected() {
    let mut reading = sample_reading();
    reading.value = None;
    assert_eq!(
        reading.check_invariants(),
        Err(ContractViolation::AvailableRequiresValue)
    );
}

#[test]
fn non_finite_number_is_rejected() {
    let mut reading = sample_reading();
    reading.value = Some(MetricValue::Number(f64::NAN));
    assert_eq!(
        reading.check_invariants(),
        Err(ContractViolation::NonFiniteNumber)
    );
    reading.value = Some(MetricValue::Number(f64::INFINITY));
    assert_eq!(
        reading.check_invariants(),
        Err(ContractViolation::NonFiniteNumber)
    );
}

#[test]
fn snapshot_validate_reports_offending_reading() {
    let mut snapshot = SystemSnapshot::empty("host-1", "gen-1", 1, "2026-10-19T00:00:00.000Z");
    let mut reading = sample_reading();
    reading.availability = Availability::Stale;
    snapshot.metrics.push(reading);
    match snapshot.validate() {
        Err(SchemaError::InvalidReading {
            metric_id,
            violation,
            ..
        }) => {
            assert_eq!(metric_id, "cpu.total.usage");
            assert_eq!(violation, ContractViolation::UnavailableRequiresNull);
        }
        other => panic!("expected InvalidReading, got {other:?}"),
    }
}

#[test]
fn schema_version_guard() {
    assert!(check_schema_version(1).is_ok());
    assert_eq!(
        check_schema_version(0),
        Err(SchemaError::UnsupportedSchemaVersion {
            found: 0,
            supported: 1
        })
    );
    assert_eq!(
        check_schema_version(2),
        Err(SchemaError::UnsupportedSchemaVersion {
            found: 2,
            supported: 1
        })
    );
    // A payload claiming a future schema must be refused, not parsed loosely.
    let raw = fixture_json().replace("\"schema_version\": 1", "\"schema_version\": 2");
    let parsed: Result<SystemSnapshot, _> = serde_json::from_str(&raw);
    if let Ok(snapshot) = parsed {
        assert!(
            snapshot.validate().is_err(),
            "future schema must fail validation"
        );
    }
}

#[test]
fn capability_report_roundtrip() {
    let mut report = CapabilityReport::new("host-1", "2026-10-19T00:00:00.000Z");
    report
        .capabilities
        .push(MetricCapability::not_implemented_yet(
            "cpu.total.usage",
            "platform-test",
        ));
    let json = serde_json::to_string(&report).unwrap();
    let back: CapabilityReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, report);
    let cap = &back.capabilities[0];
    assert_eq!(cap.state, Availability::Unsupported);
    assert_eq!(
        cap.reason_code.as_deref(),
        Some("COLLECTOR_NOT_IMPLEMENTED")
    );
}
