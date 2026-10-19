//! Security tests over the shipped Tauri configuration (security-privacy.md
//! §2/§9): strict CSP, least-privilege capabilities, sane window minimums.
//! These run on every platform in CI.

use serde_json::Value;

fn read_json(relative: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {relative}: {err}"));
    serde_json::from_str(&raw).unwrap_or_else(|err| panic!("parse {relative}: {err}"))
}

#[test]
fn csp_is_present_and_strict() {
    let conf = read_json("tauri.conf.json");
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("a CSP must be configured");
    assert!(
        csp.contains("default-src 'self'"),
        "default-src must be 'self': {csp}"
    );
    assert!(
        !csp.contains("unsafe-eval"),
        "unsafe-eval must never be allowed"
    );
    assert!(!csp.contains('*'), "no wildcard origins: {csp}");
    assert!(
        !csp.contains("https:"),
        "no remote https content origins: {csp}"
    );
}

#[test]
fn capabilities_grant_only_core_defaults() {
    let caps = read_json("capabilities/default.json");
    let permissions = caps["permissions"].as_array().expect("permissions array");
    let joined: Vec<&str> = permissions.iter().filter_map(|p| p.as_str()).collect();
    assert_eq!(
        joined,
        vec!["core:default"],
        "only core:default may be granted; found {joined:?}"
    );
    let all = joined.join(" ");
    for forbidden in ["shell", "fs:", "http:", "global-shortcut", "process"] {
        assert!(
            !all.contains(forbidden),
            "dangerous permission {forbidden} present"
        );
    }
}

#[test]
fn window_meets_minimum_logical_size() {
    let conf = read_json("tauri.conf.json");
    let window = &conf["app"]["windows"][0];
    assert_eq!(
        window["minWidth"].as_u64(),
        Some(960),
        "design.md minimum width"
    );
    assert_eq!(
        window["minHeight"].as_u64(),
        Some(650),
        "design.md minimum height"
    );
}

#[test]
fn bundle_declares_original_icons() {
    let conf = read_json("tauri.conf.json");
    let icons = conf["bundle"]["icon"].as_array().expect("icon list");
    assert!(icons.len() >= 4, "icns/ico/png set must be declared");
    assert!(
        icons.iter().any(|i| i.as_str() == Some("icons/icon.icns")),
        "macOS icns required"
    );
    assert!(
        icons.iter().any(|i| i.as_str() == Some("icons/icon.ico")),
        "Windows ico required"
    );
}
