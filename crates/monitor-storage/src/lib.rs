//! monitor-storage — safe storage scanning and OS-native Trash (FX-008).
//!
//! The scan engine never follows symlinks or crosses mounts by default,
//! counts what it could not read, and hands the UI paginated slices of a
//! flat tree. Trash operations are guarded, scoped, and fail closed.

pub mod scan;
pub mod trash;

pub use scan::{
    scan, ErrorPolicy, ScanErrorRecord, ScanNode, ScanOptions, ScanProgress, ScanStartError,
    ScanTree,
};
pub use trash::{move_to_trash, validate, TrashError, TrashRefusal};
