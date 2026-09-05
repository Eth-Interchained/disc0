//! Application state and the scan → view wiring.
//!
//! The scan runs IN-PROCESS through the disc0 library: `disc0::scan_and_record`
//! hands back real `Finding` structs, which the view renders directly. No
//! subprocess, no JSON, nothing to keep in sync.

use anyhow::Result;
use disc0::{detect::Finding, scan::ScanOptions, Store};
use forge_ui::{Action, Application, Node, Theme};
use std::path::PathBuf;

use crate::theme;
use crate::view::{self, ScanSummary};

pub struct App {
    pub summary: ScanSummary,
    pub findings: Vec<Finding>,
    pub selected: Option<usize>,
    pub ephemeral: bool,
    pub receipt: Vec<(String, String)>,
    pub scan_id: String,
    store: Store,
}

impl App {
    /// Scan once, up front, so the window opens with real data.
    ///
    /// Blocking is deliberate for v0.1: measured scans of a 39k-entry tree run
    /// ~210ms, and a progress-reporting async scan is a feature, not a
    /// prerequisite. The CLI already prints live phases for large trees.
    pub fn scan(root: PathBuf, ephemeral: bool) -> Result<Self> {
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
        let out = disc0::scan_and_record(&root, &opts, &store)?;
        let baseline = store.latest_verified_baseline();

        let summary = ScanSummary {
            root: root.display().to_string(),
            allocated: out.result.allocated_deduped(),
            logical: out.result.logical_total(),
            files: out.result.file_count(),
            entries: out.result.entries.len(),
            scan_ms: out.scan_ms,
            coverage_complete: out.result.complete,
            coverage_errors: out.result.coverage.len(),
            persisted: out.written.persisted,
            ephemeral,
            trustworthy: baseline.map(|b| b.trustworthy).unwrap_or(false),
        };

        Ok(Self {
            summary,
            findings: out.findings,
            selected: None,
            ephemeral,
            receipt: Vec::new(),
            scan_id: out.written.scan_id,
            store,
        })
    }

    /// The ONLY way to change selection. Setting `selected` directly leaves the
    /// receipt stale or empty — which is exactly the bug the headless renderer
    /// hit: it assigned `selected` and rendered a finding with no receipt under
    /// it. Selection and its evidence move together or not at all.
    pub fn select(&mut self, idx: Option<usize>) {
        match idx {
            Some(i) if i < self.findings.len() => {
                self.selected = Some(i);
                let id = format!("{}_f{}", self.scan_id, i);
                self.receipt = self.store.trace_finding(&id, 12);
            }
            _ => {
                self.selected = None;
                self.receipt.clear();
            }
        }
    }
}

impl Application for App {
    fn theme(&self) -> Theme {
        theme::theme()
    }

    fn view(&self) -> Node {
        view::screen(&self.summary, &self.findings, self.selected, &self.receipt)
    }

    fn update(&mut self, action: Action) {
        // Row ids are positional and stable for a given scan; identity comes
        // from the id, never from the row's current value.
        if let Some(rest) = action.id.strip_prefix("row_") {
            if let Ok(i) = rest.parse::<usize>() {
                if self.selected == Some(i) {
                    self.select(None); // click again to collapse
                } else {
                    self.select(Some(i));
                }
            }
        }
    }
}
