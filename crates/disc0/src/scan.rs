//! Filesystem scanner: emits observations, records coverage loss honestly.
//!
//! Contract (from the spec, enforced here):
//!   - symlinks are recorded, never followed
//!   - the scan stops at mount boundaries unless explicitly allowed
//!   - permission errors and disappearing files become explicit partial
//!     coverage, never silently-missing bytes
//!   - logical size and allocated size are tracked separately
//!   - the tool's own state directory is excluded from its own scan

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Entry {
    pub path: PathBuf,
    pub dev: u64,
    pub ino: u64,
    pub nlink: u64,
    /// Apparent size in bytes.
    pub logical: u64,
    /// Blocks actually allocated * 512. Differs from logical for sparse,
    /// compressed, and small files; this is the number that maps to disk usage.
    pub allocated: u64,
    pub is_dir: bool,
    pub is_symlink: bool,
}

#[derive(Debug, Clone)]
pub enum Coverage {
    PermissionDenied(PathBuf),
    Vanished(PathBuf),
    MountBoundary(PathBuf),
    ReadError(PathBuf, String),
    Excluded(PathBuf),
}

impl Coverage {
    pub fn kind(&self) -> &'static str {
        match self {
            Coverage::PermissionDenied(_) => "permission_denied",
            Coverage::Vanished(_) => "vanished_during_scan",
            Coverage::MountBoundary(_) => "mount_boundary",
            Coverage::ReadError(..) => "read_error",
            Coverage::Excluded(_) => "excluded",
        }
    }
    /// The error detail, when there is one. A tool about honesty must not
    /// record a reason and then hide it.
    pub fn detail(&self) -> Option<&str> {
        match self {
            Coverage::ReadError(_, m) => Some(m.as_str()),
            _ => None,
        }
    }

    pub fn path(&self) -> &Path {
        match self {
            Coverage::PermissionDenied(p)
            | Coverage::Vanished(p)
            | Coverage::MountBoundary(p)
            | Coverage::ReadError(p, _)
            | Coverage::Excluded(p) => p,
        }
    }
}

pub struct ScanResult {
    pub entries: Vec<Entry>,
    pub coverage: Vec<Coverage>,
    pub root: PathBuf,
    pub root_dev: u64,
    pub cross_filesystems: bool,
    /// True only when every directory under the root was fully enumerated.
    pub complete: bool,
    /// The caller stopped this scan. Never a usable baseline.
    pub cancelled: bool,
}

impl ScanResult {
    /// Allocated bytes, counting each inode ONCE. A hard-linked file occupies
    /// its blocks a single time no matter how many paths point at it.
    pub fn allocated_deduped(&self) -> u64 {
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut total = 0u64;
        for e in &self.entries {
            if e.is_dir || e.is_symlink {
                continue;
            }
            if seen.insert((e.dev, e.ino)) {
                total += e.allocated;
            }
        }
        total
    }

    pub fn logical_total(&self) -> u64 {
        let mut seen: HashSet<(u64, u64)> = HashSet::new();
        let mut total = 0u64;
        for e in &self.entries {
            if e.is_dir || e.is_symlink {
                continue;
            }
            if seen.insert((e.dev, e.ino)) {
                total += e.logical;
            }
        }
        total
    }

    pub fn file_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_dir && !e.is_symlink).count()
    }
}

/// Live progress, emitted during the walk so a long scan is never silent.
/// Deliberately cheap: counters and a borrowed path, no allocation per entry.
pub struct Progress<'a> {
    pub entries: usize,
    pub files: usize,
    pub dirs: usize,
    pub coverage_errors: usize,
    pub bytes_seen: u64,
    pub current: &'a Path,
}

pub struct ScanOptions {
    pub cross_filesystems: bool,
    pub follow_symlinks: bool,
    /// Absolute paths to skip entirely (our own state dir lives here).
    pub excluded: Vec<PathBuf>,
    /// Stop after this many entries, so an accidental `/` scan is bounded.
    pub max_entries: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            cross_filesystems: false,
            follow_symlinks: false,
            excluded: Vec::new(),
            max_entries: 5_000_000,
        }
    }
}

/// Scan with no progress reporting and no cancellation.
pub fn scan(root: &Path, opts: &ScanOptions) -> anyhow::Result<ScanResult> {
    scan_with(root, opts, &mut |_| true)
}

/// Scan, invoking `on_progress` as the walk proceeds. The callback is
/// THROTTLED by the caller's own logic if needed — we call it once per
/// directory completed, not once per entry, so a million-file tree does not
/// spend its time formatting status lines.
/// The callback returns `false` to CANCEL. Cancellation is checked once per
/// completed directory: fine-grained enough that a user-visible Stop feels
/// immediate, coarse enough to cost nothing on the hot path. A cancelled scan
/// sets `cancelled` and `complete = false`, so it can never be mistaken for a
/// finished one or accepted as a baseline.
pub fn scan_with(
    root: &Path,
    opts: &ScanOptions,
    on_progress: &mut dyn FnMut(&Progress) -> bool,
) -> anyhow::Result<ScanResult> {
    let root = root
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("cannot open root {}: {}", root.display(), e))?;
    let root_meta = fs::symlink_metadata(&root)?;
    let root_dev = root_meta.dev();

    let mut entries: Vec<Entry> = Vec::new();
    let mut coverage: Vec<Coverage> = Vec::new();
    let mut complete = true;
    let mut cancelled = false;
    let mut n_files = 0usize;
    let mut bytes_seen = 0u64;
    let mut stack: Vec<PathBuf> = vec![root.clone()];

    // The root itself is an entry.
    entries.push(entry_from(&root, &root_meta));

    while let Some(dir) = stack.pop() {
        if entries.len() >= opts.max_entries {
            complete = false;
            coverage.push(Coverage::ReadError(
                dir.clone(),
                format!("entry cap {} reached", opts.max_entries),
            ));
            break;
        }

        let rd = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                complete = false;
                coverage.push(match e.kind() {
                    std::io::ErrorKind::PermissionDenied => Coverage::PermissionDenied(dir.clone()),
                    std::io::ErrorKind::NotFound => Coverage::Vanished(dir.clone()),
                    _ => Coverage::ReadError(dir.clone(), e.to_string()),
                });
                continue;
            }
        };

        for item in rd {
            let item = match item {
                Ok(i) => i,
                Err(e) => {
                    complete = false;
                    coverage.push(Coverage::ReadError(dir.clone(), e.to_string()));
                    continue;
                }
            };
            let path = item.path();

            if opts.excluded.iter().any(|x| path == *x || path.starts_with(x)) {
                coverage.push(Coverage::Excluded(path));
                continue;
            }

            // symlink_metadata: never follow. A symlink is recorded as itself.
            let meta = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    complete = false;
                    coverage.push(match e.kind() {
                        std::io::ErrorKind::PermissionDenied => {
                            Coverage::PermissionDenied(path.clone())
                        }
                        std::io::ErrorKind::NotFound => Coverage::Vanished(path.clone()),
                        _ => Coverage::ReadError(path.clone(), e.to_string()),
                    });
                    continue;
                }
            };

            let is_symlink = meta.file_type().is_symlink();
            let e = entry_from(&path, &meta);

            if e.is_dir && !is_symlink {
                if e.dev != root_dev && !opts.cross_filesystems {
                    // A different device under the root is a separate volume;
                    // its bytes are NOT ours to attribute.
                    coverage.push(Coverage::MountBoundary(path.clone()));
                    complete = false;
                    entries.push(e);
                    continue;
                }
                stack.push(path.clone());
            }
            if is_symlink && opts.follow_symlinks {
                // Deliberately unimplemented: following symlinks needs cycle
                // detection and changes what "reclaimable" means. Recorded so
                // the flag can never silently do nothing.
                coverage.push(Coverage::ReadError(
                    path.clone(),
                    "follow_symlinks requested but not implemented in v0.1".into(),
                ));
                complete = false;
            }
            if !e.is_dir && !e.is_symlink {
                n_files += 1;
                bytes_seen += e.allocated;
            }
            entries.push(e);
        }

        // One report per directory completed: frequent enough to look alive on
        // a slow disk, rare enough to cost nothing on a fast one.
        //
        // Counters are RUNNING, not recomputed. The first version of this
        // re-scanned `entries` on every directory to total files and bytes —
        // O(n^2), roughly 200M operations on a 39k-entry tree, turning a
        // progress indicator into the slowest part of the scan.
        let keep_going = on_progress(&Progress {
            entries: entries.len(),
            files: n_files,
            dirs: entries.len() - n_files,
            coverage_errors: coverage.len(),
            bytes_seen: bytes_seen,
            current: &dir,
        });
        if !keep_going {
            cancelled = true;
            complete = false;
            break;
        }
    }

    Ok(ScanResult {
        entries,
        coverage,
        root,
        root_dev,
        cross_filesystems: opts.cross_filesystems,
        complete,
        cancelled,
    })
}

fn entry_from(path: &Path, meta: &fs::Metadata) -> Entry {
    Entry {
        path: path.to_path_buf(),
        dev: meta.dev(),
        ino: meta.ino(),
        nlink: meta.nlink(),
        logical: meta.len(),
        // st_blocks is in 512-byte units by POSIX definition, regardless of
        // the filesystem's own block size.
        allocated: meta.blocks().saturating_mul(512),
        is_dir: meta.file_type().is_dir(),
        is_symlink: meta.file_type().is_symlink(),
    }
}
