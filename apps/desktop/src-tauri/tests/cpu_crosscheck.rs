//! Native cross-check (phase 02 §3): under a controlled CPU load the
//! collector must report a sensible total-usage rise. This needs real CPU
//! time to elapse between sysinfo refreshes, so it is `#[ignore]`d for
//! normal runs and executed explicitly on native hardware:
//!
//!   cargo test -p app --test cpu_crosscheck -- --ignored --nocapture
//!
//! It is a direction/range check, not a calibration against Activity
//! Monitor; the matched-window comparison lives in docs/progress.

use std::thread;
use std::time::Duration;

use app_lib::collectors::CpuCollector;
use monitor_core::{Collector, MetricValue, SystemClock};

fn total_usage(collector: &mut CpuCollector) -> Result<Option<f64>, String> {
    let readings = collector
        .sample(&SystemClock)
        .map_err(|err| err.to_string())?;
    Ok(readings
        .into_iter()
        .find(|r| r.metric_id == monitor_core::families::CPU_TOTAL_USAGE)
        .and_then(|r| match r.value {
            Some(MetricValue::Number(n)) => Some(n),
            _ => None,
        }))
}

#[test]
#[ignore = "requires real elapsed CPU time and a quiet machine; run explicitly on native hardware"]
fn total_usage_rises_under_controlled_load() {
    let mut collector = CpuCollector::new();
    // Warm-up sample: must be explicitly unavailable.
    let warm = total_usage(&mut collector).expect("warm-up sample");
    assert_eq!(warm, None, "first sample must be a WARMUP (no value)");

    // Idle baseline: take two samples and keep the quieter one so a build
    // or indexing burst just before the test cannot mask the rise.
    thread::sleep(Duration::from_millis(700));
    let idle_a = total_usage(&mut collector)
        .expect("idle sample")
        .expect("idle sample must have a value");
    thread::sleep(Duration::from_millis(700));
    let idle_b = total_usage(&mut collector)
        .expect("idle sample")
        .expect("idle sample must have a value");
    let idle = idle_a.min(idle_b);
    assert!((0.0..=100.0).contains(&idle));

    // Controlled load: two spinning threads ≈ 2 cores of work.
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let stop = Arc::clone(&stop);
            thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    std::hint::spin_loop();
                }
            })
        })
        .collect();
    thread::sleep(Duration::from_millis(1_200));
    let loaded = total_usage(&mut collector).expect("loaded sample");
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    for handle in handles {
        handle.join().expect("load thread joins");
    }

    let loaded = loaded.expect("loaded sample must have a value");
    assert!((0.0..=100.0).contains(&loaded));
    assert!(
        loaded > idle + 5.0,
        "usage must rise under load (idle {idle:.1}%, loaded {loaded:.1}%)"
    );
}

use std::sync::Arc;
