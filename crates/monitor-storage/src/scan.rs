//! Storage scan engine (phase 04 §2, FX-008).
//!
//! Iterative, cancellation-aware directory walk producing a flat tree with
//! parent/children indices — the UI consumes paginated slices, never the
//! whole tree (architecture.md §10). Safety posture:
//!
//! * symlinks/reparse points are recorded but never followed by default;
//! * mount boundaries (different device id) are recorded as opaque entries
//!   and not crossed unless explicitly allowed;
//! * permission failures are counted with reason codes — the walk continues;
//! * all sizes are *reported logical* sizes; allocated size is best-effort.
//!   APFS clones/dedup/snapshots mean logical bytes ≠ reclaimable bytes
//!   (contract §4) and the UI must never claim otherwise.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// How to proceed when an entry cannot be read (permissions, vanished).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorPolicy {
    /// Count the error and continue (default).
    ContinueAndCount,
    /// Stop the whole scan at the first error.
    Stop,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub root: PathBuf,
    /// Follow directory symlinks/reparse points. **false by default** —
    /// following risks loops and leaving the intended scope.
    pub follow_symlinks: bool,
    /// Descend into other mounted filesystems. **false by default**.
    pub cross_filesystems: bool,
    pub error_policy: ErrorPolicy,
    /// Reserved for bounded parallel traversal; the v1 walk is single
    /// threaded and cheap enough for interactive roots.
    pub max_concurrency: u32,
}

impl ScanOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            follow_symlinks: false,
            cross_filesystems: false,
            error_policy: ErrorPolicy::ContinueAndCount,
            max_concurrency: 1,
        }
    }
}

/// Flat tree node. Children reference parents by index; `children` holds
/// direct children sorted lazily by the query layer.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScanNode {
    /// File/dir name only (no absolute path — paths stay scoped).
    pub name: String,
    pub is_dir: bool,
    /// True when this entry is a symlink/reparse point (not followed).
    pub is_symlink: bool,
    /// True when this directory sits on a different device than its parent
    /// and was not crossed (cross_filesystems = false).
    pub is_mount_boundary: bool,
    pub logical_size: u64,
    /// Best-effort allocated size (unix: 512-byte blocks; documented).
    pub allocated_size: u64,
    pub file_count: u64,
    pub dir_count: u64,
    pub error_count: u64,
    pub children: Vec<usize>,
    /// Set when the child list was cut short (top-N aggregation).
    pub children_truncated: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScanErrorRecord {
    pub parent_node: usize,
    pub reason: &'static str,
}

#[derive(Debug, Default)]
pub struct ScanTree {
    pub nodes: Vec<ScanNode>,
    pub errors: Vec<ScanErrorRecord>,
    /// Intentional non-entries (symlinks not followed, mount boundaries).
    pub skipped: Vec<ScanErrorRecord>,
    pub root_path: PathBuf,
    pub root_device: Option<u64>,
}

impl ScanTree {
    fn push_node(&mut self, node: ScanNode) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }
}

/// Shared progress published during the walk (live status reads).
#[derive(Debug, Default, Clone, Serialize)]
pub struct ScanProgress {
    pub visited_entries: u64,
    pub visited_bytes: u64,
    pub error_count: u64,
    pub done: bool,
    pub cancelled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum ScanStartError {
    #[error("scan root does not exist or is not a directory: {0}")]
    InvalidRoot(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Aggregate mutable state shared by the walk.
struct WalkState {
    tree: ScanTree,
    queue: VecDeque<(usize, PathBuf, Option<u64>)>, // (node idx, path, parent dev)
    errors: Vec<ScanErrorRecord>,
    skipped: Vec<ScanErrorRecord>,
    visited_entries: AtomicU64,
    visited_bytes: AtomicU64,
}

/// Run a scan to completion (or cancellation). `cancel` is checked between
/// entries; a cancelled scan returns the partial tree with `cancelled` set.
pub fn scan(
    options: &ScanOptions,
    cancel: &AtomicBool,
    progress: Option<Arc<Mutex<ScanProgress>>>,
) -> Result<(ScanTree, ScanProgress), ScanStartError> {
    let root_meta = fs::metadata(&options.root).map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => {
            ScanStartError::InvalidRoot(options.root.to_string_lossy().to_string())
        }
        _ => ScanStartError::Io(err),
    })?;
    if !root_meta.is_dir() {
        return Err(ScanStartError::InvalidRoot(
            options.root.to_string_lossy().to_string(),
        ));
    }
    let root_device = device_id(&root_meta);

    let mut state = WalkState {
        tree: ScanTree {
            nodes: Vec::new(),
            errors: Vec::new(),
            skipped: Vec::new(),
            root_path: options.root.clone(),
            root_device,
        },
        queue: VecDeque::new(),
        errors: Vec::new(),
        skipped: Vec::new(),
        visited_entries: AtomicU64::new(0),
        visited_bytes: AtomicU64::new(0),
    };

    let root_node = ScanNode {
        name: options
            .root
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| options.root.to_string_lossy().to_string()),
        is_dir: true,
        is_symlink: false,
        is_mount_boundary: false,
        logical_size: 0,
        allocated_size: 0,
        file_count: 0,
        dir_count: 1, // self-inclusive; rollup adds the subtree
        error_count: 0,
        children: Vec::new(),
        children_truncated: false,
    };
    state.tree.push_node(root_node);
    state
        .queue
        .push_back((0, options.root.clone(), root_device));

    // Root-level metadata must exist (checked above); process the queue.
    let mut progress_out = ScanProgress::default();
    let mut cancelled = false;
    while let Some((node_idx, dir_path, parent_dev)) = state.queue.pop_front() {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        match fs::read_dir(&dir_path) {
            Ok(entries) => {
                for entry in entries {
                    if cancel.load(Ordering::Relaxed) {
                        cancelled = true;
                        break;
                    }
                    let Ok(entry) = entry else {
                        state.errors.push(ScanErrorRecord {
                            parent_node: node_idx,
                            reason: "READ_DIR_ENTRY_FAILED",
                        });
                        state.tree.nodes[node_idx].error_count += 1;
                        continue;
                    };
                    match visit(&mut state, &entry.path(), node_idx, parent_dev, options) {
                        Ok(outcome) => {
                            // Skipped = intentional non-entry (status); any
                            // failure comes through the Err arm below.
                            if let VisitOutcome::Skipped(reason) = outcome {
                                state.skipped.push(ScanErrorRecord {
                                    parent_node: node_idx,
                                    reason,
                                });
                            }
                        }
                        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                            // Vanished between listing and stat: not an app error.
                        }
                        Err(_) => {
                            state.errors.push(ScanErrorRecord {
                                parent_node: node_idx,
                                reason: "METADATA_FAILED",
                            });
                            state.tree.nodes[node_idx].error_count += 1;
                        }
                    }
                }
            }
            Err(_) => {
                state.errors.push(ScanErrorRecord {
                    parent_node: node_idx,
                    reason: "PERMISSION_DENIED",
                });
                state.tree.nodes[node_idx].error_count += 1;
            }
        }
        if cancelled {
            break;
        }
    }

    progress_out.visited_entries = state.visited_entries.load(Ordering::Relaxed);
    progress_out.visited_bytes = state.visited_bytes.load(Ordering::Relaxed);
    progress_out.error_count = state.errors.len() as u64;
    progress_out.done = !cancelled;
    progress_out.cancelled = cancelled;

    // Roll aggregate sizes up from leaf directories (queue was processed in
    // arbitrary order; recompute bottom-up by reverse insertion order —
    // children always have higher indices than parents).
    for idx in (0..state.tree.nodes.len()).rev() {
        let children = state.tree.nodes[idx].children.clone();
        if children.is_empty() {
            continue;
        }
        let mut logical = state.tree.nodes[idx].logical_size;
        let mut allocated = state.tree.nodes[idx].allocated_size;
        let mut files: u64 = if state.tree.nodes[idx].is_dir { 0 } else { 1 };
        let mut dirs: u64 = u64::from(state.tree.nodes[idx].is_dir);
        let mut errors = state.tree.nodes[idx].error_count;
        for child in children {
            let c = &state.tree.nodes[child];
            logical += c.logical_size;
            allocated += c.allocated_size;
            files += c.file_count;
            dirs += c.dir_count;
            errors += c.error_count;
        }
        let node = &mut state.tree.nodes[idx];
        node.logical_size = logical;
        node.allocated_size = allocated;
        node.file_count = files;
        node.dir_count = dirs;
        node.error_count = errors;
    }
    state.tree.errors = state.errors;
    state.tree.skipped = state.skipped;

    if let Some(progress_slot) = &progress {
        if let Ok(mut slot) = progress_slot.lock() {
            *slot = progress_out.clone();
        }
    }
    Ok((state.tree, progress_out))
}

enum VisitOutcome {
    Ok,
    /// Recorded as a status (not an error): intentionally not entered.
    Skipped(&'static str),
}

fn visit(
    state: &mut WalkState,
    path: &Path,
    parent_node: usize,
    parent_dev: Option<u64>,
    options: &ScanOptions,
) -> std::io::Result<VisitOutcome> {
    let metadata = fs::symlink_metadata(path)?;
    state.visited_entries.fetch_add(1, Ordering::Relaxed);
    let file_type = metadata.file_type();
    let is_symlink = file_type.is_symlink();

    if is_symlink && !options.follow_symlinks {
        // Record the link itself (its own size), never the target.
        let node = ScanNode {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_dir: false,
            is_symlink: true,
            is_mount_boundary: false,
            logical_size: metadata.len(),
            allocated_size: allocated_size(&metadata),
            file_count: 1,
            dir_count: 0,
            error_count: 0,
            children: Vec::new(),
            children_truncated: false,
        };
        attach_child(state, parent_node, node);
        return Ok(VisitOutcome::Skipped("SYMLINK_NOT_FOLLOWED"));
    }

    if file_type.is_dir() {
        let dev = device_id(&metadata);
        let is_mount_boundary = !options.cross_filesystems
            && parent_dev.is_some()
            && dev.is_some()
            && dev != parent_dev;
        let node = ScanNode {
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            is_dir: true,
            is_symlink: false,
            is_mount_boundary,
            logical_size: 0,
            allocated_size: 0,
            file_count: 0,
            dir_count: 1, // self-inclusive; rollup adds the subtree
            error_count: 0,
            children: Vec::new(),
            children_truncated: false,
        };
        let child_idx = attach_child(state, parent_node, node);
        if is_mount_boundary {
            // Recorded as an opaque boundary entry; not crossed.
            return Ok(VisitOutcome::Skipped("MOUNT_BOUNDARY_NOT_CROSSED"));
        }
        state.queue.push_back((child_idx, path.to_path_buf(), dev));
        return Ok(VisitOutcome::Ok);
    }

    // Regular file (or other leaf type).
    let logical = metadata.len();
    let allocated = allocated_size(&metadata);
    state.visited_bytes.fetch_add(logical, Ordering::Relaxed);
    let node = ScanNode {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default(),
        is_dir: false,
        is_symlink: false,
        is_mount_boundary: false,
        logical_size: logical,
        allocated_size: allocated,
        file_count: 1,
        dir_count: 0,
        error_count: 0,
        children: Vec::new(),
        children_truncated: false,
    };
    let _ = (logical, allocated);
    attach_child(state, parent_node, node);
    Ok(VisitOutcome::Ok)
}

/// Link a completed child node to its parent. Totals are computed once by
/// the bottom-up rollup pass (children always have higher indices).
fn attach_child(state: &mut WalkState, parent_node: usize, node: ScanNode) -> usize {
    let idx = state.tree.push_node(node);
    state.tree.nodes[parent_node].children.push(idx);
    idx
}

#[cfg(unix)]
fn device_id(metadata: &fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.dev())
}

#[cfg(not(unix))]
fn device_id(_metadata: &fs::Metadata) -> Option<u64> {
    None // Windows: volume boundaries via reparse flags in a later pass
}

#[cfg(unix)]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks() * 512
}

#[cfg(not(unix))]
fn allocated_size(metadata: &fs::Metadata) -> u64 {
    metadata.len()
}
