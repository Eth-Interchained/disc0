//! disc0 — what is using my disk, what grew, who owns it, and what happens
//! if I remove it.
//!
//! v0.1 is READ-ONLY. No file outside the tool's own state directory is ever
//! modified or removed. Cleanup is a later milestone, deliberately gated behind
//! the durability contract.
//!
//! Exit codes (spec):
//!   0  complete, successful operation
//!   1  operational error
//!   2  incomplete scan, or a stale/untrustworthy baseline
//!   3  user cancellation

use disc0::{detect, scan, store};

use anyhow::Result;
use disc0::{human_bytes as human, scope_hash, state_dir};
use serde_json::json;
use std::path::PathBuf;

const SCHEMA_VERSION: u32 = 1;



/// The scope hash pins what was measured, so a later diff can refuse to
/// compare incomparable scans.

struct Args {
    cmd: String,
    root: Option<PathBuf>,
    json: bool,
    cross: bool,
    follow: bool,
    ephemeral: bool,
    limit: usize,
    target: Option<String>,
}

fn parse_args() -> Args {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args {
        cmd: raw.first().cloned().unwrap_or_else(|| "help".into()),
        root: None,
        json: false,
        cross: false,
        follow: false,
        ephemeral: false,
        limit: 15,
        target: None,
    };
    let mut i = 1;
    while i < raw.len() {
        match raw[i].as_str() {
            "--json" => a.json = true,
            "--cross-filesystems" => a.cross = true,
            "--follow-symlinks" => a.follow = true,
            "--ephemeral" => a.ephemeral = true,
            "--limit" => {
                i += 1;
                a.limit = raw.get(i).and_then(|s| s.parse().ok()).unwrap_or(15);
            }
            s if s.starts_with("--") => {}
            s => {
                if a.root.is_none() && a.cmd == "scan" {
                    a.root = Some(PathBuf::from(s));
                } else if a.target.is_none() {
                    a.target = Some(s.to_string());
                }
            }
        }
        i += 1;
    }
    a
}

fn main() {
    let code = match run() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("disc0: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let a = parse_args();
    match a.cmd.as_str() {
        "scan" => cmd_scan(&a),
        "findings" => cmd_findings(&a),
        "explain" => cmd_explain(&a),
        "status" => cmd_status(&a),
        _ => {
            println!(
                "disc0 {} — read-only disk explainer\n\n\
                 USAGE\n\
                 \x20 disc0 scan <path> [--json] [--cross-filesystems] [--ephemeral] [--limit N]\n\
                 \x20 disc0 findings [--json]\n\
                 \x20 disc0 explain <finding-id>\n\
                 \x20 disc0 status [--json]\n\n\
                 v0.1 NEVER modifies or removes anything outside its own state directory.\n\
                 State: {}\n",
                env!("CARGO_PKG_VERSION"),
                state_dir().display()
            );
            Ok(0)
        }
    }
}

fn cmd_scan(a: &Args) -> Result<i32> {
    let root = a
        .root
        .clone()
        .ok_or_else(|| anyhow::anyhow!("scan needs a path: disc0 scan <path>"))?;

    let st = state_dir();
    let opts = scan::ScanOptions {
        cross_filesystems: a.cross,
        follow_symlinks: a.follow,
        // Never measure our own database as if it were the user's data.
        excluded: vec![st.clone()],
        ..Default::default()
    };

    if !a.json {
        eprintln!("scanning {} …", root.display());
    }
    let t0 = std::time::Instant::now();
    let result = scan::scan(&root, &opts)?;
    let scan_ms = t0.elapsed().as_millis();
    let findings = detect::detect(&result);

    let store = if a.ephemeral {
        store::Store::ephemeral()
    } else {
        store::Store::open(&st)?
    };
    let written = store.write_scan(&result, &findings, &scope_hash(&root, a.cross, a.follow))?;

    if a.json {
        let out = json!({
            "schema_version": SCHEMA_VERSION,
            "scan": {
                "scan_id": written.scan_id,
                "root": result.root.to_string_lossy(),
                "status": if result.complete { "complete" } else { "incomplete" },
                "coverage_complete": result.complete,
                "cross_filesystems": result.cross_filesystems,
                "scan_ms": scan_ms,
                "entries": result.entries.len(),
                "files": result.file_count(),
                "allocated_bytes_deduped": result.allocated_deduped().to_string(),
                "logical_bytes": result.logical_total().to_string(),
                "start_hash": written.start_hash,
                "completion_hash": written.completion_hash,
                "observation_count": written.observation_count,
                "persisted": written.persisted,
                "persist_error": written.persist_error,
                "ephemeral": a.ephemeral,
            },
            "coverage_errors": result.coverage.iter().map(|c| json!({
                "kind": c.kind(), "path": c.path().to_string_lossy(), "detail": c.detail()
            })).collect::<Vec<_>>(),
            "findings": findings.iter().enumerate().map(|(i, f)| json!({
                "finding_id": format!("{}_f{}", written.scan_id, i),
                "path": f.path.to_string_lossy(),
                "category": f.category,
                "owner": {
                    "kind": f.owner_kind,
                    "root": f.owner_root.as_ref().map(|p| p.to_string_lossy().to_string()),
                    "evidence": f.evidence.as_str(),
                },
                "evidence_facts": f.evidence_facts,
                "consequence": { "class": f.consequence.as_str(), "summary": f.consequence.summary() },
                "size": {
                    "logical_bytes": f.logical.to_string(),
                    "allocated_bytes": f.allocated.to_string(),
                    "reclaimable_bytes": serde_json::Value::Null,
                    "reclaimable_quality": if f.reclaimable_unknown_reason.is_some() { "unknown" } else { "estimate_withheld" },
                    "reason": f.reclaimable_unknown_reason,
                },
                "external_links": f.external_links,
                "file_count": f.file_count,
                "cleanup": { "eligible": f.cleanup_eligible, "reason": f.cleanup_blocked_reason },
                "rule_version": f.rule_version,
            })).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!();
        println!(
            "  {}   {} in {} files   ({} logical)",
            result.root.display(),
            human(result.allocated_deduped()),
            result.file_count(),
            human(result.logical_total())
        );
        println!("  scanned in {}ms · {} entries", scan_ms, result.entries.len());
        println!();
        if findings.is_empty() {
            println!("  no recognized artifacts found.");
        } else {
            println!(
                "  {:<44} {:>10}  {:<22} {}",
                "FINDING", "ALLOCATED", "OWNER", "CONSEQUENCE"
            );
            for f in findings.iter().take(a.limit) {
                let shown = f
                    .path
                    .strip_prefix(&result.root)
                    .unwrap_or(&f.path)
                    .to_string_lossy()
                    .to_string();
                let shown = if shown.len() > 43 {
                    format!("…{}", &shown[shown.len() - 42..])
                } else {
                    shown
                };
                println!(
                    "  {:<44} {:>10}  {:<22} {}",
                    shown,
                    human(f.allocated),
                    format!("{} ({})", f.owner_kind, f.evidence.as_str()),
                    f.consequence.as_str()
                );
                if let Some(r) = &f.reclaimable_unknown_reason {
                    println!("  {:<44} {:>10}  ↳ reclaimable unknown: {}", "", "", r);
                }
            }
            if findings.len() > a.limit {
                println!("  … {} more (use --limit)", findings.len() - a.limit);
            }
        }
        println!();
        if !result.coverage.is_empty() {
            let mut by: std::collections::BTreeMap<&str, usize> = Default::default();
            for c in &result.coverage {
                *by.entry(c.kind()).or_insert(0) += 1;
            }
            let parts: Vec<String> = by.iter().map(|(k, v)| format!("{v} {k}")).collect();
            println!("  ⚠ partial coverage: {}", parts.join(", "));
            for c in result.coverage.iter().filter(|c| c.detail().is_some()).take(3) {
                println!("    {} — {}", c.path().display(), c.detail().unwrap_or(""));
            }
            println!("    totals above are a LOWER BOUND, not the whole tree.");
        }
        if !written.persisted {
            println!(
                "  ⚠ NOT PERSISTED: {}",
                written.persist_error.as_deref().unwrap_or("unknown error")
            );
            println!("    this scan is not a usable baseline.");
        } else if a.ephemeral {
            println!("  ⓘ --ephemeral: nothing was saved, no baseline for future diffs.");
        } else {
            println!(
                "  saved as {} · {} findings · try: disc0 explain {}_f0",
                written.scan_id, written.finding_count, written.scan_id
            );
        }
        println!();
    }

    // Honest exit code: an incomplete scan is not a clean success.
    if !written.persisted {
        return Ok(1);
    }
    Ok(if result.complete { 0 } else { 2 })
}

fn cmd_findings(a: &Args) -> Result<i32> {
    let store = store::Store::open(&state_dir())?;
    let rows = store.find_findings(None);
    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": SCHEMA_VERSION, "findings": rows
            }))?
        );
    } else if rows.is_empty() {
        println!("no findings stored yet — run: disc0 scan <path>");
    } else {
        for r in rows.iter().take(a.limit) {
            println!(
                "  {:<34} {:>10}  {}",
                r.get("finding_id").and_then(|v| v.as_str()).unwrap_or("?"),
                r.get("size")
                    .and_then(|s| s.get("allocated_bytes"))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(human)
                    .unwrap_or_else(|| "?".into()),
                r.get("path").and_then(|v| v.as_str()).unwrap_or("?")
            );
        }
    }
    Ok(0)
}

fn cmd_explain(a: &Args) -> Result<i32> {
    let id = a
        .target
        .clone()
        .ok_or_else(|| anyhow::anyhow!("explain needs a finding id: disc0 explain <finding-id>"))?;
    let store = store::Store::open(&state_dir())?;
    let rows = store.find_findings(None);
    let row = rows
        .iter()
        .find(|r| r.get("finding_id").and_then(|v| v.as_str()) == Some(id.as_str()));
    let row = match row {
        Some(r) => r,
        None => {
            eprintln!("no such finding: {id}");
            return Ok(1);
        }
    };
    let chain = store.trace_finding(&id, 100);

    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": SCHEMA_VERSION,
                "finding": row,
                "receipt": chain.iter().map(|(c, i)| json!({"collection": c, "id": i})).collect::<Vec<_>>(),
            }))?
        );
        return Ok(0);
    }

    // The five facts, in order: size, owner+evidence, change, consequence, next.
    let size = row
        .get("size")
        .and_then(|s| s.get("allocated_bytes"))
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);
    println!();
    println!("  {}", row.get("path").and_then(|v| v.as_str()).unwrap_or("?"));
    println!("  {} allocated in {} files",
        human(size),
        row.get("file_count").and_then(|v| v.as_u64()).unwrap_or(0));
    println!();
    if let Some(o) = row.get("owner") {
        println!(
            "  OWNER      {} — evidence: {}",
            o.get("kind").and_then(|v| v.as_str()).unwrap_or("?"),
            o.get("evidence").and_then(|v| v.as_str()).unwrap_or("?")
        );
        if let Some(r) = o.get("root").and_then(|v| v.as_str()) {
            println!("             project root {}", r);
        }
    }
    if let Some(facts) = row.get("evidence_facts").and_then(|v| v.as_array()) {
        for f in facts {
            println!("             · {}", f.as_str().unwrap_or(""));
        }
    }
    println!();
    if let Some(c) = row.get("consequence") {
        println!(
            "  IF REMOVED {}",
            c.get("class").and_then(|v| v.as_str()).unwrap_or("?")
        );
        println!("             {}", c.get("summary").and_then(|v| v.as_str()).unwrap_or(""));
    }
    if let Some(reason) = row
        .get("size")
        .and_then(|s| s.get("reason"))
        .and_then(|v| v.as_str())
    {
        println!("             reclaimable bytes UNKNOWN: {}", reason);
    }
    println!();
    if let Some(cl) = row.get("cleanup") {
        println!(
            "  CLEANUP    eligible={} — {}",
            cl.get("eligible").and_then(|v| v.as_bool()).unwrap_or(false),
            cl.get("reason").and_then(|v| v.as_str()).unwrap_or("")
        );
    }
    println!();
    println!("  RECEIPT    {} records in the causal chain", chain.len());
    for (coll, cid) in chain.iter().take(6) {
        println!("             {coll}/{cid}");
    }
    if chain.len() > 6 {
        println!("             … {} more", chain.len() - 6);
    }
    println!();
    Ok(0)
}

fn cmd_status(a: &Args) -> Result<i32> {
    let st = state_dir();
    let store = store::Store::open(&st)?;
    let (ok, bad) = store.verify();
    let baseline = store.latest_verified_baseline();

    if a.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": SCHEMA_VERSION,
                "state_dir": st.to_string_lossy(),
                "head": store.head(),
                "objects_ok": ok,
                "objects_tampered": bad,
                "lost_index_objects": store.is_lost_index(),
                "baseline": baseline.as_ref().map(|b| json!({
                    "scan_id": b.scan_id,
                    "trustworthy": b.trustworthy,
                    "coverage_complete": b.coverage_complete,
                    "claimed_observations": b.claimed_obs,
                    "readable_observations": b.readable_obs,
                    "claimed_findings": b.claimed_findings,
                    "readable_findings": b.readable_findings,
                })),
            }))?
        );
    } else {
        println!();
        println!("  state    {}", st.display());
        println!("  head     {}", if store.head().is_empty() { "(none)".into() } else { store.head() });
        println!("  objects  {} verified, {} tampered", ok, bad.len());
        match &baseline {
            None => match store.is_lost_index() {
                Some(n) => {
                    println!("  baseline ✗ UNREADABLE — {n} objects are intact and verify, but no");
                    println!("           rows can be read. This is the lost-index state, NOT an");
                    println!("           empty database: your scan history is on disk.");
                    println!("           Recover it with:  nedb-cli repair {}", st.display());
                }
                None => println!("  baseline none — run: disc0 scan <path>"),
            },
            Some(b) => {
                println!("  baseline {}", b.scan_id);
                println!(
                    "           observations {}/{} readable · findings {}/{} readable",
                    b.readable_obs, b.claimed_obs, b.readable_findings, b.claimed_findings
                );
                if b.trustworthy {
                    println!("           TRUSTWORTHY (counts match the completion record)");
                } else {
                    println!(
                        "           ✗ NOT TRUSTWORTHY — the completion record claims more records \
                         than are readable."
                    );
                    println!(
                        "             This is the lost-index state: objects are intact but the id \
                         index is missing."
                    );
                    println!("             Recover with nedb-cli repair, then re-check.");
                }
                if !b.coverage_complete {
                    println!("           coverage was INCOMPLETE — totals are a lower bound");
                }
            }
        }
        println!();
    }
    match baseline {
        Some(b) if !b.trustworthy => Ok(2),
        Some(b) if !b.coverage_complete => Ok(2),
        // Objects present but unreadable is a stale/unusable baseline, not a
        // clean slate. Exit 2, per the spec's "stale/rejected" code.
        None if store.is_lost_index().is_some() => Ok(2),
        _ => Ok(0),
    }
}
