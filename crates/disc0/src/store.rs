//! NEDB persistence + the completion protocol.
//!
//! The receipt chain is the product: every finding cites the observations that
//! justified it, via `caused_by` hashes, so `TRACE` reconstructs exactly what
//! the tool knew when it said something.
//!
//! COMPLETION PROTOCOL (rewritten after the M0 crash matrix):
//! A completion record is NOT sufficient on its own. Killing the engine before
//! its durability boundary leaves every object on disk and verifying, while the
//! id index — the thing `list()` reads — is empty. In that state
//! `tip_collection("scans")` still resolves and still says `status=complete`.
//! So a tip-only completion check certifies an EMPTY database as an
//! authoritative baseline.
//!
//! The completion record therefore carries RECORD COUNTS, and a baseline is
//! only accepted when the counts match what is actually readable. That is the
//! check that catches it.

use anyhow::{Context, Result};

/// Run `f` with fd 1 temporarily pointed at fd 2.
///
/// The engine announces its startup regime on stdout (`[nedbd] warm start …`).
/// That is fine for a server and fatal for us: `--json` must emit nothing on
/// stdout but the document, or every machine consumer has to strip banner lines
/// before parsing. Rather than parse around it, we move the engine's chatter to
/// stderr, where progress belongs, for exactly the duration of the open.
fn with_stdout_on_stderr<T>(f: impl FnOnce() -> T) -> T {
    unsafe {
        let saved = libc::dup(1);
        if saved < 0 {
            return f();
        }
        libc::fflush(std::ptr::null_mut());
        libc::dup2(2, 1);
        let out = f();
        libc::fflush(std::ptr::null_mut());
        libc::dup2(saved, 1);
        libc::close(saved);
        out
    }
}
use nedb_engine::Db;
use serde_json::json;
use std::path::Path;

use crate::detect::Finding;
use crate::scan::ScanResult;

pub struct Store {
    db: Db,
}

pub struct WrittenScan {
    pub scan_id: String,
    pub start_hash: String,
    pub completion_hash: Option<String>,
    pub observation_count: usize,
    pub finding_count: usize,
    pub persisted: bool,
    pub persist_error: Option<String>,
}

impl Store {
    pub fn open(state_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(state_dir)
            .with_context(|| format!("cannot create state dir {}", state_dir.display()))?;
        // A second process gets a loud, explicit refusal from the engine's
        // advisory dir lock (released even on SIGKILL). We surface it as a busy
        // error rather than pretending to scan.
        let db = with_stdout_on_stderr(|| Db::open(state_dir, None))
            .with_context(|| "another disc0 process holds this state directory")?;
        Ok(Self { db })
    }

    /// In-memory mode for `--ephemeral`: no baseline is saved, and the caller
    /// is told so explicitly.
    pub fn ephemeral() -> Self {
        Self { db: Db::in_memory() }
    }

    pub fn head(&self) -> String {
        self.db.head()
    }

    pub fn verify(&self) -> (usize, Vec<String>) {
        self.db.verify()
    }

    /// Write a scan: start record -> observations -> findings -> completion.
    /// Every stage cites its cause, and the durability boundary is checked.
    pub fn write_scan(
        &self,
        scan: &ScanResult,
        findings: &[Finding],
        scope_hash: &str,
    ) -> Result<WrittenScan> {
        let scan_id = format!(
            "scan_{}_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
            std::process::id()
        );

        let start = self.db.put(
            "scans",
            &scan_id,
            json!({
                "scan_id": &scan_id,
                "root": scan.root.to_string_lossy(),
                "root_dev": scan.root_dev,
                "cross_filesystems": scan.cross_filesystems,
                "scope_hash": scope_hash,
                "status": "running",
                // Full build provenance, not just a version string: a receipt
                // that cannot name the binary that produced it is a weaker
                // receipt. A finding from an old build stays identifiable.
                "tool": crate::brand::provenance_json(),
            }),
            vec![],
            None,
            None,
        )?;

        // Observations are PACKED, not one document per file.
        //
        // Measured on a 39,400-entry tree with one document per observation:
        // 323 MB of state (loose objects) / 185 MB (v3 segments) to describe
        // 293 MB of node_modules, and ~4.7 s of it write-bound. The floor is
        // structural — every document costs an object plus an id-index leaf,
        // and each rounds up to a filesystem block — so no substrate choice
        // fixes it. A disk-space tool that accumulates metadata on that scale
        // fails its own purpose.
        //
        // Packing trades receipt GRANULARITY for viability: a finding now cites
        // the observation PAGES holding its evidence rather than each individual
        // file record. The evidence is still verbatim and still causally linked
        // — a page is content-addressed and tamper-evident like any node — but
        // TRACE lands on a page, and the exact file is found inside it.
        const OBS_PER_PAGE: usize = 1000;
        let mut obs_page_hashes: Vec<String> = Vec::new();
        for (page_idx, chunk) in scan.entries.chunks(OBS_PER_PAGE).enumerate() {
            let rows: Vec<serde_json::Value> = chunk
                .iter()
                .map(|e| {
                    json!({
                        "path": e.path.to_string_lossy(),
                        "dev": e.dev,
                        "ino": e.ino,
                        "nlink": e.nlink,
                        // Byte counters as decimal STRINGS: exact integers
                        // across JavaScript consumers.
                        "logical_bytes": e.logical.to_string(),
                        "allocated_bytes": e.allocated.to_string(),
                        "is_dir": e.is_dir,
                        "is_symlink": e.is_symlink,
                    })
                })
                .collect();
            let node = self.db.put(
                "observation_pages",
                &format!("{}_p{}", scan_id, page_idx),
                json!({
                    "scan_id": &scan_id,
                    "page": page_idx,
                    "count": rows.len(),
                    "first_index": page_idx * OBS_PER_PAGE,
                    "observations": rows,
                }),
                vec![start.hash.clone()],
                None,
                None,
            )?;
            obs_page_hashes.push(node.hash);
        }

        // Coverage gaps are first-class records, not a log line.
        for (i, c) in scan.coverage.iter().enumerate() {
            self.db.put(
                "coverage_errors",
                &format!("{}_c{}", scan_id, i),
                json!({
                    "scan_id": &scan_id,
                    "kind": c.kind(),
                    "path": c.path().to_string_lossy(),
                }),
                vec![start.hash.clone()],
                None,
                None,
            )?;
        }

        // Findings cite the observations inside their subtree — the evidence.
        let mut finding_hashes = Vec::with_capacity(findings.len());
        for (i, f) in findings.iter().enumerate() {
            // Cite the pages containing this subtree's observations.
            let mut pages: Vec<usize> = scan
                .entries
                .iter()
                .enumerate()
                .filter(|(_, e)| e.path.starts_with(&f.path))
                .map(|(idx, _)| idx / OBS_PER_PAGE)
                .collect();
            pages.sort_unstable();
            pages.dedup();
            let evidence: Vec<String> = pages
                .iter()
                .filter_map(|p| obs_page_hashes.get(*p).cloned())
                .take(64) // bound edge fan-out; the rest stays reachable in the log
                .collect();
            let causes = if evidence.is_empty() {
                vec![start.hash.clone()]
            } else {
                evidence
            };
            let node = self.db.put(
                "findings",
                &format!("{}_f{}", scan_id, i),
                json!({
                    "scan_id": &scan_id,
                    "path": f.path.to_string_lossy(),
                    "category": f.category,
                    "owner": {
                        "kind": f.owner_kind,
                        "root": f.owner_root.as_ref().map(|p| p.to_string_lossy().to_string()),
                        "evidence": f.evidence.as_str(),
                    },
                    "evidence_facts": f.evidence_facts,
                    "consequence": {
                        "class": f.consequence.as_str(),
                        "summary": f.consequence.summary(),
                    },
                    "size": {
                        "logical_bytes": f.logical.to_string(),
                        "allocated_bytes": f.allocated.to_string(),
                        "reclaimable_bytes": serde_json::Value::Null,
                        "reclaimable_quality": if f.reclaimable_unknown_reason.is_some() { "unknown" } else { "estimate_withheld" },
                        "reason": f.reclaimable_unknown_reason,
                    },
                    "external_links": f.external_links,
                    "file_count": f.file_count,
                    "cleanup": {
                        "eligible": f.cleanup_eligible,
                        "reason": f.cleanup_blocked_reason,
                    },
                    "rule_version": f.rule_version,
                }),
                causes,
                None,
                None,
            )?;
            finding_hashes.push(node.hash);
        }

        // Completion record: carries COUNTS, because counts are what catch a
        // lost index. Cites the findings and the scan start.
        let mut causes = finding_hashes.clone();
        causes.push(start.hash.clone());
        let completion = self.db.put(
            "scans",
            &format!("{}_complete", scan_id),
            json!({
                "scan_id": &scan_id,
                "status": if scan.complete { "complete" } else { "incomplete" },
                "coverage_complete": scan.complete,
                "observation_count": scan.entries.len(),
                "observation_page_count": obs_page_hashes.len(),
                "finding_count": findings.len(),
                "coverage_error_count": scan.coverage.len(),
                "allocated_bytes_deduped": scan.allocated_deduped().to_string(),
            }),
            causes,
            None,
            None,
        )?;

        // THE DURABILITY BOUNDARY. try_flush_all reports failure; flush_all
        // would not, and a scan that claims to be a baseline while its index
        // never reached disk is exactly the failure this tool exists to avoid.
        let (persisted, persist_error) = match self.db.try_flush_all() {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };

        Ok(WrittenScan {
            scan_id,
            start_hash: start.hash,
            completion_hash: Some(completion.hash),
            observation_count: scan.entries.len(),
            finding_count: findings.len(),
            persisted,
            persist_error,
        })
    }

    /// Accept a baseline ONLY when its completion record's counts match what is
    /// actually readable. See the module comment: the tip alone is not enough.
    pub fn latest_verified_baseline(&self) -> Option<BaselineCheck> {
        let scans = self.db.list("scans");
        let completion = scans
            .into_iter()
            .filter(|n| n.id.ends_with("_complete"))
            .max_by_key(|n| n.seq);
        let completion = match completion {
            Some(c) => c,
            None => return None,
        };

        let claimed_obs = completion
            .data
            .get("observation_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let claimed_findings = completion
            .data
            .get("finding_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let scan_id = completion
            .data
            .get("scan_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Sum the counts recorded in each readable page. This is the check that
        // catches a lost index: the completion record claims a total, and only
        // pages that are actually readable can contribute to it.
        let readable_obs: usize = self
            .db
            .list("observation_pages")
            .into_iter()
            .filter(|n| n.data.get("scan_id").and_then(|v| v.as_str()) == Some(&scan_id))
            .map(|n| {
                n.data
                    .get("observations")
                    .and_then(|v| v.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0)
            })
            .sum();
        let readable_findings = self
            .db
            .list("findings")
            .into_iter()
            .filter(|n| n.data.get("scan_id").and_then(|v| v.as_str()) == Some(&scan_id))
            .count();

        let trustworthy = readable_obs == claimed_obs && readable_findings == claimed_findings;
        Some(BaselineCheck {
            scan_id,
            claimed_obs,
            readable_obs,
            claimed_findings,
            readable_findings,
            trustworthy,
            coverage_complete: completion
                .data
                .get("coverage_complete")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
        })
    }

    /// Objects exist but NOTHING is readable: the lost-index state.
    ///
    /// This distinction is load-bearing and is exactly the trap the M0 crash
    /// matrix exposed. `list()` returning nothing looks identical to a fresh
    /// database, while `verify()` happily reports every object healthy. A tool
    /// that reads that as "no baseline yet" tells the user to scan again and
    /// silently abandons history that is sitting on disk, intact.
    pub fn is_lost_index(&self) -> Option<usize> {
        let (objects_ok, _) = self.db.verify();
        let readable = self.db.list("scans").len()
            + self.db.list("findings").len()
            + self.db.list("observation_pages").len();
        if objects_ok > 0 && readable == 0 {
            Some(objects_ok)
        } else {
            None
        }
    }

    /// Walk the causal chain backward from a finding: the receipt.
    pub fn trace_finding(&self, finding_id: &str, limit: usize) -> Vec<(String, String)> {
        let f = match self.db.get("findings", finding_id) {
            Some(f) => f,
            None => return Vec::new(),
        };
        self.db
            .trace(&f.hash, false, limit)
            .into_iter()
            .map(|n| (n.coll, n.id))
            .collect()
    }

    pub fn find_findings(&self, scan_id: Option<&str>) -> Vec<serde_json::Value> {
        self.db
            .list("findings")
            .into_iter()
            .filter(|n| match scan_id {
                Some(s) => n.data.get("scan_id").and_then(|v| v.as_str()) == Some(s),
                None => true,
            })
            .map(|n| {
                let mut v = n.data.clone();
                if let Some(o) = v.as_object_mut() {
                    o.insert("finding_id".into(), json!(n.id));
                    o.insert("node_hash".into(), json!(n.hash));
                }
                v
            })
            .collect()
    }
}

#[derive(Debug)]
pub struct BaselineCheck {
    pub scan_id: String,
    pub claimed_obs: usize,
    pub readable_obs: usize,
    pub claimed_findings: usize,
    pub readable_findings: usize,
    pub trustworthy: bool,
    pub coverage_complete: bool,
}
