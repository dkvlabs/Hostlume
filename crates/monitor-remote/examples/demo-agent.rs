//! Development demo agent (phase 10 live validation ONLY).
//!
//! Runs the real `monitor-remote` TLS server on loopback and feeds it a
//! slowly-changing **synthetic** snapshot so a console app can be exercised
//! end-to-end (pairing optional, polling, UI) without a second machine.
//! This is a demo harness, clearly labelled as such in its display name and
//! source tag — it is never compiled into the app bundle.
//!
//! Usage:
//!   cargo run -p monitor-remote --example demo-agent -- \
//!     --port 45990 [--device-hash <sha256-hex-of-device-token>] [--label NAME]
//!
//! Prints one JSON line on startup:
//!   {"port":…,"host_id":"…","fingerprint":"…","pairing_code":"…"}
//! With `--device-hash` the harness pre-registers that console credential
//! (skipping the pairing exchange); otherwise pair with the printed code.
#![allow(clippy::print_stdout)]

use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use monitor_core::{
    Availability, Clock, MetricReading, MetricUnit, MetricValue, SystemClock, SystemSnapshot,
};
use monitor_remote::{spawn_with, RemoteConfig, RemoteData, RemoteMode, SpawnOptions, TlsIdentity};

fn demo_snapshot(host_id: &str, tick: u64) -> SystemSnapshot {
    let now = SystemClock.now_utc_iso8601();
    let mut snapshot = SystemSnapshot::empty(host_id, "demo-gen", tick, &now);
    let mut push = |metric_id: &str, entity: &str, unit: MetricUnit, value: f64| {
        snapshot.metrics.push(MetricReading {
            metric_id: metric_id.to_string(),
            entity_id: entity.to_string(),
            value: Some(MetricValue::Number(value)),
            unit,
            availability: Availability::Available,
            source: "demo-harness".to_string(),
            collected_at_utc: now.clone(),
            monotonic_sequence: tick,
            reason_code: None,
            human_message_key: None,
            precision: None,
        });
    };
    let wave = |tick: u64, period: u64, low: f64, high: f64| {
        let phase = f64::from((tick % period) as u32) / f64::from(period as u32);
        low + (high - low) * (0.5 - 0.5 * (std::f64::consts::TAU * phase).cos())
    };
    push(
        "cpu.total.usage",
        "host",
        MetricUnit::Percent,
        wave(tick, 60, 8.0, 64.0),
    );
    for core in 0..4 {
        push(
            "cpu.core.usage",
            &format!("cpu:{core}"),
            MetricUnit::Percent,
            wave(tick + core * 7, 45, 2.0, 90.0),
        );
    }
    push(
        "memory.physical.used",
        "host",
        MetricUnit::Bytes,
        wave(tick, 300, 12.0e9, 20.0e9),
    );
    push("memory.physical.total", "host", MetricUnit::Bytes, 36.0e9);
    push(
        "host.uptime",
        "host",
        MetricUnit::Seconds,
        4_200.0 + tick as f64,
    );
    snapshot
}

fn main() {
    let mut port: u16 = 45_990;
    let mut device_hash: Option<String> = None;
    let mut label = "Demo harness".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().and_then(|v| v.parse().ok()).unwrap_or(port),
            "--device-hash" => device_hash = args.next().filter(|v| v.len() == 64),
            "--label" => label = args.next().unwrap_or(label),
            other => eprintln!("unknown argument {other}"),
        }
    }

    let host_id = uuid::Uuid::new_v4().to_string();
    let identity = Arc::new(TlsIdentity::generate().expect("demo identity"));
    let snapshot: Arc<RwLock<Option<SystemSnapshot>>> = Arc::default();
    let data = RemoteData {
        host_id: host_id.clone(),
        cadence_ms: 2_000,
        snapshot: Arc::clone(&snapshot),
        capabilities: Arc::new(std::sync::Mutex::new(monitor_core::CapabilityReport::new(
            host_id.clone(),
            "demo",
        ))),
        history_db_path: std::env::temp_dir(),
        process_rows: None,
        on_device_paired: None,
    };
    let handle = spawn_with(
        &RemoteConfig {
            mode: RemoteMode::Lan,
            port,
        },
        data,
        monitor_remote::auth::hash_token("demo-owner-token"),
        SpawnOptions {
            tls: Some(Arc::clone(&identity)),
            // Demo scope only: loopback, never a real LAN exposure.
            bind_addr: Some(IpAddr::from(Ipv4Addr::LOCALHOST)),
            devices: device_hash
                .map(|hash| {
                    vec![monitor_remote::DeviceCredential {
                        device_id: "demo-console".to_string(),
                        token_hash: hash,
                    }]
                })
                .unwrap_or_default(),
            display_name: Some(format!("{label} (DEMO)")),
        },
    )
    .expect("demo agent binds");
    let port = handle.port();

    // Prime the pairing window and share the code even in pre-registered mode.
    let pairing = handle.open_pairing_window().expect("pairing window");
    let writer = Arc::clone(&snapshot);
    let snap_host = host_id.clone();
    std::thread::spawn(move || {
        let mut tick: u64 = 0;
        loop {
            *writer.write().expect("demo snapshot lock") = Some(demo_snapshot(&snap_host, tick));
            tick = tick.wrapping_add(1);
            std::thread::sleep(Duration::from_secs(2));
        }
    });
    println!(
        "{}",
        serde_json::json!({
            "port": port,
            "host_id": host_id,
            "fingerprint": identity.fingerprint_hex(),
            "pairing_code": pairing.code,
            "note": "DEMO agent: synthetic values, loopback bind, never a real machine reading",
        })
    );
    // Park forever; Ctrl+C terminates the process (and the server with it).
    loop {
        std::thread::sleep(Duration::from_secs(3_600));
    }
}
