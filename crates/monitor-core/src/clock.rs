//! Clock wrappers. UTC wall time labels persisted records; the monotonic
//! reading drives interval math. The two must never be mixed: wall clocks can
//! jump (NTP, DST, manual change), monotonic time does not.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Source of time for collectors and the scheduler. Test code may substitute
/// a fixed implementation to make interval math deterministic.
pub trait Clock: Send + Sync {
    /// UTC wall-clock milliseconds since the Unix epoch (persisted value).
    fn now_epoch_ms(&self) -> i64;
    /// Monotonic nanoseconds from an arbitrary but fixed origin; only ever
    /// used for deltas within one process lifetime.
    fn monotonic_ns(&self) -> u64;
    /// ISO-8601 UTC rendering of [`Clock::now_epoch_ms`], e.g.
    /// `2026-10-19T12:34:56.789Z`.
    fn now_utc_iso8601(&self) -> String {
        format_utc_iso8601(self.now_epoch_ms())
    }
}

/// Process-wide start anchor for the monotonic reading.
static MONOTONIC_ANCHOR: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            // A clock before 1970 is not a supported configuration; saturate
            // instead of panicking so a broken host clock cannot take the
            // app down. 0 here means "unusable timestamp", which downstream
            // freshness checks will reject as stale.
            .unwrap_or(0)
    }

    fn monotonic_ns(&self) -> u64 {
        MONOTONIC_ANCHOR
            .get_or_init(Instant::now)
            .elapsed()
            .as_nanos() as u64
    }
}

/// Format epoch milliseconds as ISO-8601 UTC (`YYYY-MM-DDTHH:MM:SS.mmmZ`).
/// Self-contained civil-calendar conversion (no external date dependency in
/// the contract crate).
pub fn format_utc_iso8601(epoch_ms: i64) -> String {
    let millis = epoch_ms.rem_euclid(1000) as u64;
    let seconds = epoch_ms.div_euclid(1000);
    let days = seconds.div_euclid(86_400);
    let secs_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to (y, m, d).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097); // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // year of era
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year
    let mp = (5 * doy + 2) / 153; // month index March=0..February=11
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso8601_anchors() {
        assert_eq!(format_utc_iso8601(0), "1970-01-01T00:00:00.000Z");
        // 2000-02-29 (leap day): 951782400 s after epoch.
        assert_eq!(
            format_utc_iso8601(951_782_400_000),
            "2000-02-29T00:00:00.000Z"
        );
        assert_eq!(
            format_utc_iso8601(951_782_400_999),
            "2000-02-29T00:00:00.999Z"
        );
    }

    #[test]
    fn iso8601_roundtrip_recent() {
        // 2026-10-19T00:00:00Z == 1792368000 (verified with `date -u -r`).
        assert_eq!(
            format_utc_iso8601(1_792_368_000_000),
            "2026-10-19T00:00:00.000Z"
        );
        assert_eq!(format_utc_iso8601(-1), "1969-12-31T23:59:59.999Z");
    }
}
