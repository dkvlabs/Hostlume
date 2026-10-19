//! Scan lifecycle registry and guarded storage actions (FX-008).
//!
//! Scans run on their own thread, publish a live `ScanProgress`, and can be
//! cancelled. The tree stays in memory; the UI pulls paginated children.
//! Trash/open actions validate against the scan roots registered in this
//! session — nothing outside a user-scanned root is ever touched.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use monitor_storage::{ScanOptions, ScanProgress, ScanTree};
use serde::Serialize;

#[derive(Debug)]
pub struct ScanHandle {
    pub root: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub tree: Arc<Mutex<ScanTree>>,
    pub progress: Arc<Mutex<ScanProgress>>,
}

#[derive(Debug, Default)]
pub struct ScanRegistry {
    inner: Mutex<HashMap<String, Arc<ScanHandle>>>,
}

impl ScanRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn a scan on a dedicated thread. Returns the scan id.
    pub fn start(&self, scan_id: String, root: PathBuf) {
        let cancel = Arc::new(AtomicBool::new(false));
        let tree_slot: Arc<Mutex<ScanTree>> = Arc::new(Mutex::new(ScanTree {
            nodes: Vec::new(),
            errors: Vec::new(),
            skipped: Vec::new(),
            root_path: root.clone(),
            root_device: None,
        }));
        let progress_slot = Arc::new(Mutex::new(ScanProgress::default()));
        let handle = Arc::new(ScanHandle {
            root: root.clone(),
            cancel: Arc::clone(&cancel),
            tree: Arc::clone(&tree_slot),
            progress: Arc::clone(&progress_slot),
        });
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(scan_id.clone(), handle);

        let registry_cancel = Arc::clone(&cancel);
        let options = ScanOptions::new(root);
        std::thread::Builder::new()
            .name(format!("scan-{scan_id}"))
            .spawn(move || {
                let (tree, progress) = monitor_storage::scan(
                    &options,
                    &registry_cancel,
                    Some(Arc::clone(&progress_slot)),
                )
                .expect("root validated before spawn");
                if let Ok(mut slot) = tree_slot.lock() {
                    *slot = tree;
                }
                if let Ok(mut slot) = progress_slot.lock() {
                    *slot = progress;
                }
            })
            .expect("spawn scan thread");
    }

    pub fn get(&self, scan_id: &str) -> Option<Arc<ScanHandle>> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(scan_id)
            .cloned()
    }

    pub fn cancel(&self, scan_id: &str) -> bool {
        match self.get(scan_id) {
            Some(handle) => {
                handle.cancel.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    /// All registered roots (for Trash scope validation).
    pub fn roots(&self) -> Vec<PathBuf> {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .values()
            .map(|handle| handle.root.clone())
            .collect()
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanStatus {
    #[serde(rename = "scanId")]
    pub scan_id: String,
    pub root: String,
    pub progress: ScanProgress,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanChildren {
    /// Sliced children, size-descending.
    pub children: Vec<ChildEntry>,
    pub total_children: usize,
    /// True when the slice is shorter than the requested page.
    pub end_of_list: bool,
    pub node_index: usize,
    pub errors: u64,
    #[serde(rename = "skippedCount")]
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChildEntry {
    pub index: usize,
    pub name: String,
    pub is_dir: bool,
    #[serde(rename = "isSymlink")]
    pub is_symlink: bool,
    #[serde(rename = "isMountBoundary")]
    pub is_mount_boundary: bool,
    #[serde(rename = "logicalSize")]
    pub logical_size: u64,
    #[serde(rename = "allocatedSize")]
    pub allocated_size: u64,
    #[serde(rename = "errorCount")]
    pub error_count: u64,
}
