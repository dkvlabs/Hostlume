//! Unit identifiers and parsing. Canonical names are the snake_case
//! serializations of [`MetricUnit`]; a small alias set covers common
//! human/OS spellings so adapters can be written naturally.

use crate::metric::MetricUnit;

impl MetricUnit {
    pub fn as_canonical_str(&self) -> &'static str {
        match self {
            MetricUnit::Percent => "percent",
            MetricUnit::Bytes => "bytes",
            MetricUnit::BytesPerSecond => "bytes_per_second",
            MetricUnit::BitsPerSecond => "bits_per_second",
            MetricUnit::Celsius => "celsius",
            MetricUnit::Rpm => "rpm",
            MetricUnit::Seconds => "seconds",
            MetricUnit::Count => "count",
            MetricUnit::Watts => "watts",
            MetricUnit::Hertz => "hertz",
            MetricUnit::Boolean => "boolean",
            MetricUnit::Text => "text",
        }
    }
}

/// Parse a unit name into the canonical unit. Accepts canonical snake_case
/// plus a bounded alias list; unknown units are `None` (callers must not
/// invent units).
pub fn parse_unit(raw: &str) -> Option<MetricUnit> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "percent" | "%" => Some(MetricUnit::Percent),
        "bytes" | "byte" | "b" => Some(MetricUnit::Bytes),
        "bytes_per_second" | "bytes/s" | "b/s" | "bps" => Some(MetricUnit::BytesPerSecond),
        "bits_per_second" | "bits/s" | "bit/s" => Some(MetricUnit::BitsPerSecond),
        "celsius" | "°c" | "c" => Some(MetricUnit::Celsius),
        "rpm" => Some(MetricUnit::Rpm),
        "seconds" | "second" | "s" => Some(MetricUnit::Seconds),
        "count" => Some(MetricUnit::Count),
        "watts" | "watt" | "w" => Some(MetricUnit::Watts),
        "hertz" | "hz" => Some(MetricUnit::Hertz),
        "boolean" | "bool" => Some(MetricUnit::Boolean),
        "text" => Some(MetricUnit::Text),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_roundtrip() {
        for unit in [
            MetricUnit::Percent,
            MetricUnit::Bytes,
            MetricUnit::BytesPerSecond,
            MetricUnit::BitsPerSecond,
            MetricUnit::Celsius,
            MetricUnit::Rpm,
            MetricUnit::Seconds,
            MetricUnit::Count,
            MetricUnit::Watts,
            MetricUnit::Hertz,
            MetricUnit::Boolean,
            MetricUnit::Text,
        ] {
            assert_eq!(parse_unit(unit.as_canonical_str()), Some(unit));
        }
    }

    #[test]
    fn aliases_and_rejection() {
        assert_eq!(parse_unit("%"), Some(MetricUnit::Percent));
        assert_eq!(parse_unit("°C"), Some(MetricUnit::Celsius));
        assert_eq!(parse_unit("B/s"), Some(MetricUnit::BytesPerSecond));
        assert_eq!(parse_unit(" Hz "), Some(MetricUnit::Hertz));
        assert_eq!(parse_unit("furlongs"), None);
        assert_eq!(parse_unit(""), None);
    }
}
