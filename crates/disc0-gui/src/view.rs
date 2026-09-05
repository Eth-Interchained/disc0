//! The findings screen.
//!
//! Built from REAL `disc0::Finding` values handed over in-process — no
//! subprocess, no JSON round trip, no hardcoded rows.

use disc0::{human_bytes, Finding};
use forge_ui::{Node, Rect};

use crate::theme::*;

pub struct ScanSummary {
    pub root: String,
    pub allocated: u64,
    pub logical: u64,
    pub files: usize,
    pub entries: usize,
    pub scan_ms: u128,
    pub coverage_complete: bool,
    pub coverage_errors: usize,
    pub persisted: bool,
    pub trustworthy: bool,
    /// True when this run saved nothing — the footer must say so rather than
    /// reporting a "verified baseline" that exists only in memory.
    pub ephemeral: bool,
}

fn stat(id: &str, label: &'static str, value: String) -> Node {
    Node::custom(id, label, false, move |p, r, _t| {
        p.text(&tracked(label), r.x, r.y, 9.0, SAGE.mix(FIELD, 0.3), false);
        p.text(&value, r.x, r.y + 14.0, 17.0, CREAM, true);
    })
    .height(38.0)
}

fn masthead(s: &ScanSummary) -> Node {
    let root = s.root.clone();
    let total = human_bytes(s.allocated);
    let logical = format!("allocated · {} logical", human_bytes(s.logical));
    Node::custom("masthead", "disc0", false, move |p, r, _t| {
        p.text("disc0", r.x, r.y, 30.0, CREAM, true);
        let w = p.measure("disc0", 30.0, true);
        p.text(&root, r.x + w + 14.0, r.y + 12.0, 13.0, SAGE, false);

        let tw = p.measure(&total, 30.0, true);
        p.text(&total, r.x + r.w - tw, r.y - 2.0, 30.0, BRASS, true);
        let lw = p.measure(&logical, 10.0, false);
        p.text(&logical, r.x + r.w - lw, r.y + 30.0, 10.0, SAGE.mix(FIELD, 0.2), false);
    })
    .height(46.0)
}

fn hairline(id: &str) -> Node {
    Node::custom(id, "", false, |p, r, _t| {
        p.rect(Rect::new(r.x, r.y, r.w, 1.0), BRASS.mix(FIELD, 0.45), 0.0);
    })
    .height(1.0)
}

fn column_header() -> Node {
    Node::custom("hdr", "", false, |p, r, _t| {
        let c = SAGE.mix(FIELD, 0.4);
        p.text(&tracked("finding"), r.x + 16.0, r.y, 9.0, c, true);
        p.text(&tracked("magnitude"), r.x + r.w * 0.47, r.y, 9.0, c, true);
        let s = tracked("allocated");
        let w = p.measure(&s, 9.0, true);
        p.text(&s, r.x + r.w - 16.0 - w, r.y, 9.0, c, true);
    })
    .height(14.0)
}

fn finding_row(idx: usize, f: &Finding, root: &str, max: u64, selected: bool) -> Node {
    let accent = accent_for(f.consequence.as_str());
    let ev = f.evidence.as_str().to_string();
    let ev_color = evidence_color(f.evidence.as_str());
    // Show the path relative to the scan root — the shared prefix is noise.
    let shown = f
        .path
        .strip_prefix(root)
        .unwrap_or(&f.path)
        .to_string_lossy()
        .trim_start_matches('/')
        .to_string();
    let owner = f.owner_kind.clone();
    let consequence = f.consequence.as_str().to_string();
    let pretty = human_bytes(f.allocated);
    let bytes = f.allocated;
    let files = f.file_count;
    let unknown = f.reclaimable_unknown_reason.clone();

    Node::custom(format!("row_{idx}"), shown.clone(), true, move |p, r, _t| {
        if selected {
            p.rect(r, PANEL.mix(FIELD, 0.25), 3.0);
            p.rect(Rect::new(r.x, r.y, 3.0, r.h), accent, 0.0);
        } else {
            p.rect(Rect::new(r.x, r.y, 2.0, r.h), accent.mix(FIELD, 0.45), 0.0);
        }

        let pad = 16.0;
        let x = r.x + pad;
        p.text(&shown, x, r.y + 7.0, 15.0, CREAM, selected);

        // provenance line: evidence pill + owner + file count
        let pill = format!(" {ev} ");
        let pw = p.measure(&pill, 9.0, true) + 8.0;
        let py = r.y + 28.0;
        p.rect(Rect::new(x, py, pw, 14.0), ev_color.mix(FIELD, 0.72), 7.0);
        p.text(&pill, x + 4.0, py + 2.5, 9.0, ev_color, true);
        let meta = format!("{owner} · {files} files");
        p.text(&meta, x + pw + 8.0, py + 2.0, 10.0, SAGE.mix(FIELD, 0.3), false);

        // magnitude — linear, because 1.9 GiB really does dwarf 200 KiB
        let bar_x = r.x + r.w * 0.47;
        let bar_w = r.w * 0.30;
        let by = r.y + r.h * 0.5 - 8.0;
        p.rect(Rect::new(bar_x, by, bar_w, 8.0), FIELD.mix(SAGE, 0.14), 4.0);
        let frac = if max == 0 { 0.0 } else { bytes as f32 / max as f32 };
        p.rect(Rect::new(bar_x, by, (bar_w * frac).max(2.0), 8.0), accent, 4.0);

        // numbers share a right edge so magnitudes can be compared
        let right = r.x + r.w - pad;
        let sw = p.measure(&pretty, 16.0, true);
        p.text(&pretty, right - sw, r.y + 7.0, 16.0, CREAM, true);
        let cw = p.measure(&consequence, 10.0, false);
        p.text(&consequence, right - cw, py + 2.0, 10.0, accent, false);

        // an unknowable reclaim is stated, under the bar, never hidden
        if let Some(reason) = &unknown {
            let short: String = reason.chars().take(78).collect();
            p.text(&short, bar_x, by + 12.0, 9.0, COPPER, true);
        }
    })
    .height(52.0)
}

/// The receipt: what this finding was concluded from.
fn detail(f: &Finding, receipt: &[(String, String)]) -> Node {
    let accent = accent_for(f.consequence.as_str());
    let facts = f.evidence_facts.clone();
    let summary = f.consequence.summary().to_string();
    let chain: Vec<String> = receipt.iter().map(|(c, i)| format!("{c}/{i}")).collect();
    let blocked = f
        .cleanup_blocked_reason
        .clone()
        .unwrap_or_else(|| "no owner identified".into());

    Node::custom("detail", "", false, move |p, r, _t| {
        p.rect(r, PANEL.mix(FIELD, 0.45), 3.0);
        let x = r.x + 18.0;
        let mut y = r.y + 14.0;

        p.text(&tracked("why"), x, y, 9.0, accent, true);
        y += 16.0;
        for f in facts.iter().take(3) {
            let line: String = f.chars().take(96).collect();
            p.text(&format!("· {line}"), x, y, 11.0, SAGE, false);
            y += 15.0;
        }
        y += 4.0;
        p.text(&tracked("if removed"), x, y, 9.0, accent, true);
        y += 16.0;
        for chunk in wrap(&summary, 104).iter().take(3) {
            p.text(chunk, x, y, 11.0, CREAM.mix(FIELD, 0.15), false);
            y += 15.0;
        }
        y += 4.0;
        p.text(&tracked("cleanup"), x, y, 9.0, COPPER, true);
        p.text(&blocked, x + 74.0, y, 10.0, COPPER, false);
        y += 18.0;
        p.text(&tracked("receipt"), x, y, 9.0, accent, true);
        let joined = chain.join("   ");
        let short: String = joined.chars().take(120).collect();
        p.text(&short, x + 74.0, y, 9.0, SAGE.mix(FIELD, 0.3), false);
    })
    .height(168.0)
}

/// Naive greedy wrap. Good enough for fixed prose; not a text engine.
fn wrap(s: &str, cols: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        if line.len() + word.len() + 1 > cols && !line.is_empty() {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

fn footer(s: &ScanSummary) -> Node {
    let complete = s.coverage_complete;
    let errors = s.coverage_errors;
    let persisted = s.persisted;
    let trustworthy = s.trustworthy;
    let ephemeral = s.ephemeral;
    let sig = format!(
        "disc0 v{} · {} · nedb-engine {}",
        disc0::brand::VERSION,
        disc0::brand::GIT_SHA,
        disc0::brand::ENGINE
    );
    Node::custom("footer", "", false, move |p, r, _t| {
        p.rect(Rect::new(r.x, r.y, r.w, 1.0), BRASS.mix(FIELD, 0.35), 0.0);
        let badge = " READ-ONLY ";
        let bw = p.measure(badge, 9.0, true) + 6.0;
        p.rect(Rect::new(r.x, r.y + 12.0, bw, 15.0), TEAL.mix(FIELD, 0.5), 7.0);
        p.text(badge, r.x + 3.0, r.y + 15.0, 9.0, CREAM, true);

        // State the two things a user must not have to guess about.
        let mut x = r.x + bw + 10.0;
        let coverage = if complete {
            "coverage complete".to_string()
        } else {
            format!("PARTIAL COVERAGE · {errors} gaps · totals are a lower bound")
        };
        let cc = if complete { SAGE.mix(FIELD, 0.25) } else { COPPER };
        p.text(&coverage, x, r.y + 15.0, 10.0, cc, !complete);
        x += p.measure(&coverage, 10.0, !complete) + 14.0;

        let (baseline, bc) = if ephemeral {
            ("ephemeral — nothing saved, no baseline", BRASS)
        } else if !persisted {
            ("NOT PERSISTED — not a usable baseline", COPPER)
        } else if trustworthy {
            ("baseline verified", SAGE.mix(FIELD, 0.25))
        } else {
            ("baseline UNREADABLE — run nedb-cli repair", COPPER)
        };
        p.text(baseline, x, r.y + 15.0, 10.0, bc, bc == COPPER);

        let sw = p.measure(&sig, 9.0, false);
        p.text(&sig, r.x + r.w - sw, r.y + 16.0, 9.0, SAGE.mix(FIELD, 0.5), false);
    })
    .height(34.0)
}

pub fn screen(
    summary: &ScanSummary,
    findings: &[Finding],
    selected: Option<usize>,
    receipt: &[(String, String)],
) -> Node {
    let max = findings.iter().map(|f| f.allocated).max().unwrap_or(1);
    let mut kids = vec![
        masthead(summary),
        hairline("rule1"),
        Node::row(
            "stats",
            vec![
                stat("s_files", "files", commas(summary.files as u64)),
                stat("s_entries", "entries", commas(summary.entries as u64)),
                stat("s_scan", "scan", format!("{}ms", summary.scan_ms)),
                stat("s_find", "findings", commas(findings.len() as u64)),
            ],
        )
        .gap(30.0),
        column_header(),
    ];

    if findings.is_empty() {
        kids.push(
            Node::custom("empty", "", false, |p, r, _t| {
                p.text(
                    "no recognized artifacts in this tree",
                    r.x + 16.0,
                    r.y + 10.0,
                    13.0,
                    SAGE,
                    false,
                );
            })
            .height(40.0),
        );
    }

    let root = summary.root.clone();
    for (i, f) in findings.iter().enumerate().take(8) {
        kids.push(finding_row(i, f, &root, max, selected == Some(i)));
    }

    if let Some(i) = selected {
        if let Some(f) = findings.get(i) {
            kids.push(detail(f, receipt));
        }
    }

    kids.push(footer(summary));
    Node::column("root", kids).padding(30.0).gap(12.0)
}
