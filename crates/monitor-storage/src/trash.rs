//! Guarded Move-to-Trash (FX-008, phase 04 §2.6).
//!
//! OS-native Trash/Recycle Bin only (trash crate → NSTrashwell / Recycle
//! Bin / FreeDesktop). **Fail closed**: any guard refusal or trash error
//! leaves the file exactly where it was; there is no permanent-delete
//! fallback anywhere in this module (AGENTS.md §6, security-privacy.md §4).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrashRefusal {
    #[error("path does not exist (it may have been moved already): {0}")]
    Missing(String),
    #[error("path is outside the scanned scope of this session: {0}")]
    OutOfScope(String),
    #[error("refusing to trash a scan root itself: {0}")]
    IsScanRoot(String),
    #[error("refusing to trash a protected system location: {0}")]
    ProtectedLocation(String),
    #[error("path contains a component that escapes the scanned scope: {0}")]
    PathEscape(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrashError {
    #[error("OS trash operation failed for {path}: {message}")]
    OsFailed { path: String, message: String },
}

/// Locations that must never be trashed even if a user builds a scan over
/// them. Platform-scoped; deliberately conservative.
fn protected_prefixes() -> Vec<PathBuf> {
    // True system directories only: /var and /private are deliberately NOT
    // here because on macOS they contain user-writable areas (temp dirs,
    // /private/var/folders) where trashing is legitimate.
    let prefixes = vec![
        PathBuf::from("/System"),
        PathBuf::from("/usr"),
        PathBuf::from("/bin"),
        PathBuf::from("/sbin"),
        PathBuf::from("/etc"),
        PathBuf::from("/boot"),
        PathBuf::from("/proc"),
        PathBuf::from("/sys"),
        PathBuf::from("/dev"),
    ];
    prefixes
}

fn is_protected(path: &Path) -> bool {
    protected_prefixes()
        .iter()
        .any(|prefix| path == prefix || path.starts_with(prefix))
}

/// Validate a single trash request against the session's scanned roots.
/// `scan_roots` are the user's scanned roots this session (canonicalized
/// here); the requested path is canonicalized before the scope and
/// protected-location checks so symlinked parents (/var → /private/var on
/// macOS) cannot mask or fake a location.
pub fn validate(path: &Path, scan_roots: &[PathBuf]) -> Result<(), TrashRefusal> {
    let canonical = std::fs::canonicalize(path).map_err(|_| {
        // Delete-between-selection-and-action: honest, non-fatal refusal.
        TrashRefusal::Missing(path.to_string_lossy().to_string())
    })?;
    let display = canonical.to_string_lossy().to_string();
    if is_protected(&canonical) {
        return Err(TrashRefusal::ProtectedLocation(display));
    }
    let canonical_roots: Vec<PathBuf> = scan_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect();
    if canonical_roots.is_empty() {
        return Err(TrashRefusal::OutOfScope(display));
    }
    let in_scope = canonical_roots
        .iter()
        .any(|root| canonical != *root && canonical.starts_with(root));
    if !in_scope {
        return Err(TrashRefusal::OutOfScope(display));
    }
    Ok(())
}

/// Validate then move to OS trash. One path per call so a partial batch
/// leaves a precise per-path result for the UI.
pub fn move_to_trash(
    path: &Path,
    scan_roots: &[PathBuf],
) -> Result<(), Box<dyn std::error::Error>> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|_| TrashRefusal::Missing(path.to_string_lossy().to_string()))?;
    validate(&canonical, scan_roots)?;
    let canonical_path = &canonical;
    match trash::delete(canonical_path) {
        Ok(()) => Ok(()),
        Err(err) => Err(Box::new(TrashError::OsFailed {
            path: canonical_path.to_string_lossy().to_string(),
            message: err.to_string(),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "monitor-storage-test-{}-{}-{}",
            std::process::id(),
            tag,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn validate_rejects_paths_outside_scan_roots() {
        let root = temp_root("scope");
        let inside = root.join("file.txt");
        std::fs::write(&inside, b"x").unwrap();
        let outside = temp_root("outside");
        let outside_file = outside.join("other.txt");
        std::fs::write(&outside_file, b"x").unwrap();

        assert!(validate(&inside, std::slice::from_ref(&root)).is_ok());
        assert!(validate(&outside_file, std::slice::from_ref(&root)).is_err());
        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn validate_rejects_missing_and_roots_and_protected() {
        let root = temp_root("missing");
        let missing = root.join("gone.txt");

        assert!(validate(&missing, std::slice::from_ref(&root)).is_err());
        // The scan root itself must not be trashed.
        assert!(validate(&root, std::slice::from_ref(&root)).is_err());
        // System locations.
        assert!(validate(Path::new("/System/Library"), &[]).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn delete_between_selection_and_action_refuses_cleanly() {
        let root = temp_root("vanished");
        let file = root.join("victim.txt");
        std::fs::write(&file, b"x").unwrap();
        // Selection happens, then the file is deleted by someone else, then
        // the action arrives: must refuse (missing), not crash or invent.
        std::fs::remove_file(&file).unwrap();
        let err = move_to_trash(&file, std::slice::from_ref(&root)).unwrap_err();
        assert!(err
            .downcast_ref::<TrashRefusal>()
            .is_some_and(|r| matches!(r, TrashRefusal::Missing(_))));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn os_trash_moves_file_out_of_origin() {
        let root = temp_root("trashtest");
        let file = root.join("to-trash.txt");
        std::fs::write(&file, b"bye").unwrap();
        move_to_trash(&file, std::slice::from_ref(&root)).expect("trash succeeds on macOS");
        assert!(!file.exists(), "file must be gone from its origin");
        let _ = std::fs::remove_dir_all(root);
        // Note: the item now lives in ~/.Trash — intentionally not verified
        // further (no restore/clear APIs are exercised).
    }
}
