//! Application state: browse → scan → results.
//!
//! The scan runs on a WORKER THREAD with real cancellation, so the window never
//! blocks and Stop actually stops. Progress is published into a shared snapshot
//! the view reads.
//!
//! KNOWN LIMITATION, stated because a silent one is worse: Forge UI runs its
//! event loop on `ControlFlow::Wait` and `Application` has no tick hook, so the
//! window repaints on INPUT, not on a timer. Progress therefore advances when
//! the user interacts rather than continuously. The worker, the snapshot and
//! the view are all built so that a framework tick makes this live with no
//! changes here — see `poll()`.

use anyhow::Result;
use disc0::{detect::Finding, scan::ScanOptions, Store};
use forge_ui::{Action, Application, Node, Theme};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Mutex};

use crate::browse::{self, Listing};
use crate::theme;
use crate::view::{self, ScanSummary};

/// Live counters, written by the worker and read by the view. Atomics rather
/// than a mutex on the hot path: the walker touches these once per directory.
#[derive(Default)]
pub struct Live {
    pub entries: AtomicUsize,
    pub files: AtomicUsize,
    pub bytes: AtomicU64,
    pub errors: AtomicUsize,
    pub current: Mutex<String>,
}

pub enum Phase {
    /// Choosing what to look at. Nothing has been read yet.
    Browsing,
    /// A worker is walking the tree. Stop is available and real.
    Scanning {
        started: std::time::Instant,
        cancel: Arc<AtomicBool>,
        live: Arc<Live>,
        rx: mpsc::Receiver<Result<Done>>,
    },
    /// Results for a completed or cancelled scan.
    Results(Box<Results>),
    Failed(String),
}

pub struct Done {
    pub summary: ScanSummary,
    pub findings: Vec<Finding>,
    pub scan_id: String,
    pub store: Store,
}

pub struct Results {
    pub summary: ScanSummary,
    pub findings: Vec<Finding>,
    pub selected: Option<usize>,
    pub receipt: Vec<(String, String)>,
    pub scan_id: String,
    store: Store,
}

pub struct App {
    pub phase: Phase,
    pub cwd: PathBuf,
    pub listing: Listing,
    pub ephemeral: bool,
}

impl App {
    pub fn new(start: PathBuf, ephemeral: bool) -> Self {
        let cwd = start.canonicalize().unwrap_or(start);
        Self {
            listing: browse::list(&cwd),
            phase: Phase::Browsing,
            cwd,
            ephemeral,
        }
    }

    fn navigate(&mut self, to: PathBuf) {
        if to.is_dir() {
            self.cwd = to.canonicalize().unwrap_or(to);
            self.listing = browse::list(&self.cwd);
        }
    }

    /// Spawn the scan. Returns immediately; the window stays responsive.
    fn start_scan(&mut self) {
        let root = self.cwd.clone();
        let ephemeral = self.ephemeral;
        let cancel = Arc::new(AtomicBool::new(false));
        let live = Arc::new(Live::default());
        let (tx, rx) = mpsc::channel();

        {
            let cancel = Arc::clone(&cancel);
            let live = Arc::clone(&live);
            std::thread::spawn(move || {
                let out = run_scan(&root, ephemeral, &cancel, &live);
                let _ = tx.send(out);
            });
        }

        self.phase = Phase::Scanning {
            started: std::time::Instant::now(),
            cancel,
            live,
            rx,
        };
    }

    fn stop_scan(&mut self) {
        if let Phase::Scanning { cancel, .. } = &self.phase {
            // The worker sees this at its next directory boundary and unwinds
            // cleanly, so the partial result is still labelled honestly.
            cancel.store(true, Ordering::SeqCst);
        }
    }

    /// Collect the worker's result if it has finished. Cheap and idempotent —
    /// safe to call from `view()` as well as `update()`, which is what keeps
    /// this correct under a Wait-driven event loop.
    pub fn poll(&mut self) {
        let finished = match &self.phase {
            Phase::Scanning { rx, .. } => match rx.try_recv() {
                Ok(res) => Some(res),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err(anyhow::anyhow!("scan worker died unexpectedly")))
                }
            },
            _ => None,
        };
        if let Some(res) = finished {
            self.phase = match res {
                Ok(d) => Phase::Results(Box::new(Results {
                    summary: d.summary,
                    findings: d.findings,
                    selected: None,
                    receipt: Vec::new(),
                    scan_id: d.scan_id,
                    store: d.store,
                })),
                Err(e) => Phase::Failed(format!("{e:#}")),
            };
        }
    }

    fn select(&mut self, idx: Option<usize>) {
        if let Phase::Results(r) = &mut self.phase {
            match idx {
                Some(i) if i < r.findings.len() => {
                    r.selected = Some(i);
                    let id = format!("{}_f{}", r.scan_id, i);
                    r.receipt = r.store.trace_finding(&id, 12);
                }
                _ => {
                    r.selected = None;
                    r.receipt.clear();
                }
            }
        }
    }
}

fn run_scan(root: &Path, ephemeral: bool, cancel: &AtomicBool, live: &Live) -> Result<Done> {
    let state = disc0::state_dir();
    let opts = ScanOptions {
        excluded: vec![state.clone()],
        ..Default::default()
    };
    let store = if ephemeral {
        Store::ephemeral()
    } else {
        Store::open(&state)?
    };

    let t0 = std::time::Instant::now();
    let result = disc0::scan_with(root, &opts, &mut |p| {
        live.entries.store(p.entries, Ordering::Relaxed);
        live.files.store(p.files, Ordering::Relaxed);
        live.bytes.store(p.bytes_seen, Ordering::Relaxed);
        live.errors.store(p.coverage_errors, Ordering::Relaxed);
        if let Ok(mut c) = live.current.lock() {
            *c = p.current.to_string_lossy().to_string();
        }
        !cancel.load(Ordering::SeqCst)
    })?;
    let scan_ms = t0.elapsed().as_millis();
    let findings = disc0::detect::detect(&result);
    let written = store.write_scan(
        &result,
        &findings,
        &disc0::scope_hash(root, opts.cross_filesystems, opts.follow_symlinks),
    )?;
    let baseline = store.latest_verified_baseline();

    Ok(Done {
        summary: ScanSummary {
            root: root.display().to_string(),
            allocated: result.allocated_deduped(),
            logical: result.logical_total(),
            files: result.file_count(),
            entries: result.entries.len(),
            scan_ms,
            coverage_complete: result.complete,
            coverage_errors: result.coverage.len(),
            cancelled: result.cancelled,
            persisted: written.persisted,
            trustworthy: baseline.map(|b| b.trustworthy).unwrap_or(false),
            ephemeral,
        },
        findings,
        scan_id: written.scan_id,
        store,
    })
}

impl Application for App {
    fn theme(&self) -> Theme {
        theme::theme()
    }

    fn view(&self) -> Node {
        match &self.phase {
            Phase::Browsing => view::browse_screen(&self.cwd, &self.listing, self.ephemeral),
            Phase::Scanning { started, live, .. } => {
                view::scanning_screen(&self.cwd, live, started.elapsed())
            }
            Phase::Results(r) => {
                view::results_screen(&r.summary, &r.findings, r.selected, &r.receipt)
            }
            Phase::Failed(msg) => view::failed_screen(&self.cwd, msg),
        }
    }

    fn update(&mut self, action: Action) {
        // Any interaction is also a chance to notice the worker finished. Under
        // ControlFlow::Wait this is what actually advances the scan state.
        self.poll();

        let id = action.id.as_str();
        match id {
            "scan_start" => self.start_scan(),
            "scan_stop" => self.stop_scan(),
            "back_to_browse" => self.phase = Phase::Browsing,
            "up" => {
                if let Some(parent) = self.cwd.parent().map(|p| p.to_path_buf()) {
                    self.navigate(parent);
                }
            }
            "go_home" => {
                if let Some(h) = browse::home() {
                    self.navigate(h);
                }
            }
            "go_root" => self.navigate(PathBuf::from("/")),
            _ => {
                if let Some(rest) = id.strip_prefix("dir_") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if let Some(e) = self.listing.dirs.get(i) {
                            let to = e.path.clone();
                            self.navigate(to);
                        }
                    }
                } else if let Some(rest) = id.strip_prefix("row_") {
                    if let Ok(i) = rest.parse::<usize>() {
                        let already = matches!(&self.phase, Phase::Results(r) if r.selected == Some(i));
                        self.select(if already { None } else { Some(i) });
                    }
                }
            }
        }
    }

    /// Machine-readable state, for the agent protocol. No secrets, no paths
    /// beyond the one the user already chose.
    fn inspect(&self) -> serde_json::Value {
        let phase = match &self.phase {
            Phase::Browsing => "browsing",
            Phase::Scanning { .. } => "scanning",
            Phase::Results(_) => "results",
            Phase::Failed(_) => "failed",
        };
        serde_json::json!({
            "phase": phase,
            "cwd": self.cwd.display().to_string(),
            "ephemeral": self.ephemeral,
            "read_only": true,
            "tool": disc0::brand::provenance_json(),
        })
    }
}
