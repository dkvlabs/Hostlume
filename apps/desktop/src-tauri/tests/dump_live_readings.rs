//! Release-smoke diagnostic (phase 09): dump one warm live sample from every
//! collector, exactly as the scheduler assembles it, so the TypeScript IPC
//! guards can be checked against real readings from this machine. Ignored by
//! default; run with
//! `cargo test -p app --test dump_live_readings -- --ignored --nocapture`.
//! Output: `<temp dir>/hostlume-live-readings.json`.
use monitor_core::{Clock, Collector, SystemClock};

#[test]
#[ignore]
fn dump_live_readings_to_tmp() {
    let mut collectors: Vec<Box<dyn Collector>> = vec![
        Box::new(app_lib::collectors::CpuCollector::new()),
        Box::new(app_lib::collectors::MemoryCollector::new()),
        Box::new(app_lib::collectors::HostCollector::new()),
        Box::new(app_lib::collectors::DisksCollector::new()),
        Box::new(app_lib::collectors::NetworkCollector::new()),
        Box::new(app_lib::collectors::DiskIoCollector::new()),
        Box::new(app_lib::collectors::SensorsCollector::new()),
        Box::new(app_lib::collectors::BatteryCollector::new()),
        Box::new(app_lib::collectors::BluetoothCollector::new()),
        Box::new(app_lib::collectors::GpuCollector::new()),
        Box::new(monitor_platform_macos::PlatformCollector::new()),
        Box::new(monitor_platform_windows::PlatformCollector::new()),
        Box::new(monitor_platform_linux::PlatformCollector::new()),
    ];
    let clock = SystemClock;
    for collector in &mut collectors {
        let _ = collector.sample(&clock);
    }
    std::thread::sleep(std::time::Duration::from_millis(1_500));
    let mut all = Vec::new();
    for collector in &mut collectors {
        match collector.sample(&clock) {
            Ok(readings) => all.extend(readings),
            Err(err) => eprintln!("{} error: {err:?}", collector.id()),
        }
    }
    let json = serde_json::to_string_pretty(&all).expect("serialize");
    let path = std::env::temp_dir().join("hostlume-live-readings.json");
    std::fs::write(&path, &json).expect("write");
    eprintln!(
        "wrote {} readings to {} at {}",
        all.len(),
        path.display(),
        clock.now_utc_iso8601()
    );
}
