//! Storage scan integration tests over real temp-dir fixtures
//! (phase 04 §4): deep trees, Unicode names, symlink loops, hardlinks,
//! sparse files, cancellation, and error policies.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use monitor_storage::{scan, ErrorPolicy, ScanOptions};

fn temp_root(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "monitor-storage-scan-{}-{}-{}",
        std::process::id(),
        tag,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn scan_default(root: &Path) -> monitor_storage::ScanTree {
    let options = ScanOptions::new(root);
    let (tree, _) = scan(&options, &AtomicBool::new(false), None).expect("scan ok");
    tree
}

#[test]
fn deep_tree_totals_match_known_layout() {
    let root = temp_root("deep");
    // root/a (100 bytes), root/b/c (300 bytes), root/empty-dir/
    fs::create_dir_all(root.join("b/c")).unwrap();
    fs::write(root.join("a.txt"), vec![0u8; 100]).unwrap();
    fs::write(root.join("b/c/big.bin"), vec![0u8; 300]).unwrap();
    fs::create_dir(root.join("empty-dir")).unwrap();

    let tree = scan_default(&root);
    let root_node = &tree.nodes[0];
    assert_eq!(root_node.logical_size, 400, "files only: 100 + 300");
    assert_eq!(root_node.file_count, 2);
    assert_eq!(root_node.dir_count, 4, "b, c, empty-dir + root itself");
    assert!(!root_node.children_truncated);
    // Allocated >= logical for real files on a real FS (blocks rounding).
    assert!(root_node.allocated_size >= root_node.logical_size);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn unicode_and_spaces_survive_the_walk() {
    let root = temp_root("unicode");
    fs::write(root.join("данные файл.txt"), vec![0u8; 50]).unwrap();
    fs::create_dir(root.join("日本語フォルダ")).unwrap();
    fs::write(root.join("日本語フォルダ/ok.txt"), vec![0u8; 25]).unwrap();

    let tree = scan_default(&root);
    let root_node = &tree.nodes[0];
    assert_eq!(root_node.logical_size, 75);
    let names: Vec<&str> = root_node
        .children
        .iter()
        .map(|idx| tree.nodes[*idx].name.as_str())
        .collect();
    assert!(names.contains(&"данные файл.txt"), "names: {names:?}");
    assert!(names.contains(&"日本語フォルダ"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn symlink_loop_is_recorded_not_followed() {
    let root = temp_root("loop");
    fs::write(root.join("real.txt"), vec![0u8; 10]).unwrap();
    // loop -> parent dir; following it would never terminate.
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
    // link -> real.txt (file symlink)
    std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();

    let tree = scan_default(&root);
    let root_node = &tree.nodes[0];
    // Content counted exactly once: the non-symlink child is real.txt (10).
    let content_bytes: u64 = root_node
        .children
        .iter()
        .map(|idx| &tree.nodes[*idx])
        .filter(|n| !n.is_symlink)
        .map(|n| n.logical_size)
        .sum();
    assert_eq!(content_bytes, 10, "symlink targets not followed");
    // Symlink entries record their own (path-string) sizes as skipped
    // statuses — never descended, so the scan terminates.
    let symlinks: Vec<&monitor_storage::ScanNode> = root_node
        .children
        .iter()
        .map(|idx| &tree.nodes[*idx])
        .filter(|n| n.is_symlink)
        .collect();
    assert_eq!(symlinks.len(), 2, "both symlinks recorded");
    assert!(
        tree.skipped
            .iter()
            .all(|s| s.reason == "SYMLINK_NOT_FOLLOWED"),
        "skipped statuses: {:?}",
        tree.skipped
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn hardlinks_are_counted_per_path_with_uncertainty_documented() {
    let root = temp_root("hardlink");
    fs::write(root.join("orig.bin"), vec![0u8; 500]).unwrap();
    fs::hard_link(root.join("orig.bin"), root.join("alias.bin")).unwrap();

    let tree = scan_default(&root);
    // Both paths report 500 logical bytes; combined total 1000 — this
    // over-counts shared blocks, which scan docs call out as uncertainty
    // (dedup/clones/hardlinks: logical ≠ reclaimable).
    assert_eq!(tree.nodes[0].logical_size, 1000);
    fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(unix)]
fn sparse_file_logical_exceeds_allocated() {
    let root = temp_root("sparse");
    let file = root.join("sparse.bin");
    let f = fs::File::create(&file).unwrap();
    f.set_len(1_000_000).unwrap();
    drop(f);

    let tree = scan_default(&root);
    let file_node = &tree.nodes[tree.nodes[0].children[0]];
    assert_eq!(file_node.logical_size, 1_000_000);
    // On APFS/ext4 a fresh sparse file allocates (almost) nothing.
    assert!(
        file_node.allocated_size < file_node.logical_size,
        "sparse: allocated {} < logical {}",
        file_node.allocated_size,
        file_node.logical_size
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancellation_stops_midwalk_with_partial_tree() {
    let root = temp_root("cancel");
    for i in 0..40 {
        let dir = root.join(format!("dir-{i:03}"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("f.bin"), vec![0u8; 100]).unwrap();
    }
    let cancel = Arc::new(AtomicBool::new(true)); // cancelled before start
    let options = ScanOptions::new(&root);
    let (tree, progress) = scan(&options, &cancel, None).expect("cancelled scan returns");
    assert!(progress.cancelled);
    assert!(!progress.done);
    assert!(
        tree.nodes.len() < 40,
        "partial tree only: {} nodes",
        tree.nodes.len()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn permission_failure_is_counted_scan_continues() {
    let root = temp_root("perm");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(root.join("visible.txt"), vec![0u8; 10]).unwrap();
    fs::write(locked.join("secret.txt"), vec![0u8; 10]).unwrap();

    // chmod 000: unreadable for non-root users. Skip the assertion branch
    // when running as root (CI containers) — the walk itself must still pass.
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o000);
    let _ = fs::set_permissions(&locked, perms.clone());
    let is_root = users_current_uid() == 0;

    let options = ScanOptions::new(&root);
    let (tree, progress) = scan(&options, &AtomicBool::new(false), None).expect("scan ok");
    if !is_root {
        assert!(progress.error_count >= 1, "locked dir must be an error");
        assert!(tree.errors.iter().any(|e| e.reason == "PERMISSION_DENIED"));
    }
    // The visible sibling is still counted — the scan continued.
    assert!(tree.nodes[0].logical_size >= 10);

    // Restore and clean.
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    let _ = fs::set_permissions(&locked, perms);
    fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
fn users_current_uid() -> u32 {
    unsafe { libc::getuid() }
}

#[test]
fn stop_policy_halts_on_first_error() {
    let root = temp_root("stop");
    let locked = root.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(root.join("visible.txt"), vec![0u8; 10]).unwrap();
    let mut perms = fs::metadata(&locked).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o000);
    let _ = fs::set_permissions(&locked, perms.clone());
    let is_root = users_current_uid() == 0;

    let options = ScanOptions {
        error_policy: ErrorPolicy::Stop,
        ..ScanOptions::new(&root)
    };
    let (tree, progress) = scan(&options, &AtomicBool::new(false), None).expect("scan ok");
    if !is_root {
        assert!(progress.error_count >= 1);
        // With Stop the walk ended immediately: the queue was abandoned.
        assert!(progress.done);
    }
    let _ = tree;

    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    let _ = fs::set_permissions(&locked, perms);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_root_is_a_typed_error() {
    let options = ScanOptions::new("/nonexistent/path/for/hostlume-test");
    match scan(&options, &AtomicBool::new(false), None) {
        Err(monitor_storage::ScanStartError::InvalidRoot(_)) => {}
        other => panic!("expected InvalidRoot, got {other:?}"),
    }
}
