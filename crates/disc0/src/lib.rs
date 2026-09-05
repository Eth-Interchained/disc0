//! disc0 — read-only disk explainer.
//!
//! This crate is the ENGINE. Both the CLI and the GUI link it directly and
//! call these functions in-process: there is no subprocess, no JSON round
//! trip, and no serialize/deserialize tax between the scanner and the UI.
//! `scan()` hands back real structs.

pub mod brand;
pub mod detect;
pub mod scan;
pub mod store;

pub use brand::{banner, provenance_json, signature};
pub use detect::{Consequence, Evidence, Finding};
pub use scan::{scan, Coverage, Entry, ScanOptions, ScanResult};
pub use store::{BaselineCheck, Store, WrittenScan};

/// Format bytes for humans. Shared so the CLI and GUI never disagree about
/// what "1.9 GiB" means.
pub fn human_bytes(bytes: u64) -> String {
    const U: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".into();
    }
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, U[0])
    } else {
        format!("{:.1} {}", v, U[i])
    }
}

/// Default state directory, honoring DISC0_STATE then XDG_STATE_HOME.
pub fn state_dir() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(d) = std::env::var("DISC0_STATE") {
        return PathBuf::from(d);
    }
    let base = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
            PathBuf::from(home).join(".local").join("state")
        });
    base.join("disc0")
}

/// Pin what was measured, so a later diff can refuse incomparable scans.
pub fn scope_hash(root: &std::path::Path, cross: bool, follow: bool) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    root.hash(&mut h);
    cross.hash(&mut h);
    follow.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// One call the UI can make on a worker thread: scan, detect, persist.
pub struct ScanOutcome {
    pub result: ScanResult,
    pub findings: Vec<Finding>,
    pub written: WrittenScan,
    pub scan_ms: u128,
}

pub fn scan_and_record(
    root: &std::path::Path,
    opts: &ScanOptions,
    store: &Store,
) -> anyhow::Result<ScanOutcome> {
    let t0 = std::time::Instant::now();
    let result = scan::scan(root, opts)?;
    let scan_ms = t0.elapsed().as_millis();
    let findings = detect::detect(&result);
    let written = store.write_scan(
        &result,
        &findings,
        &scope_hash(root, opts.cross_filesystems, opts.follow_symlinks),
    )?;
    Ok(ScanOutcome { result, findings, written, scan_ms })
}
