//! Directory browsing for choosing a scan root.
//!
//! Read-only and boring on purpose: it lists directories, it never follows a
//! symlink, and it never touches file contents. A permission-denied directory
//! is shown as unreadable rather than omitted — a browser that silently hides
//! what it cannot open teaches you to distrust it.

use std::path::{Path, PathBuf};

pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub readable: bool,
    /// Immediate child count, or None when we could not read it.
    pub children: Option<usize>,
}

pub struct Listing {
    pub dirs: Vec<DirEntry>,
    /// Files are counted, not listed — you scan directories, not files.
    pub file_count: usize,
    pub error: Option<String>,
}

pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub fn list(dir: &Path) -> Listing {
    let mut dirs = Vec::new();
    let mut file_count = 0usize;
    let mut error = None;

    match std::fs::read_dir(dir) {
        Err(e) => error = Some(format!("{e}")),
        Ok(rd) => {
            for item in rd.flatten() {
                let path = item.path();
                // symlink_metadata: a symlinked directory is not descended into
                // here, matching the scanner's own no-follow contract.
                let meta = match std::fs::symlink_metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if meta.file_type().is_symlink() {
                    continue;
                }
                if meta.is_dir() {
                    let name = item.file_name().to_string_lossy().to_string();
                    // Hidden directories are real disk usage; showing them is
                    // the honest default. `.git` in particular matters here.
                    let children = std::fs::read_dir(&path).ok().map(|r| r.count());
                    dirs.push(DirEntry {
                        name,
                        path,
                        readable: children.is_some(),
                        children,
                    });
                } else {
                    file_count += 1;
                }
            }
        }
    }

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Listing {
        dirs,
        file_count,
        error,
    }
}
