//! The screens.
//!
//! Tone: an instrument, not an alarm. This tool points at a person's own files,
//! so the visual language has to earn trust rather than manufacture urgency —
//! no red, no warning triangles, no "THREATS FOUND". Copper appears only where
//! there is genuine uncertainty, and the READ-ONLY promise is visible on every
//! screen including mid-scan.

use disc0::{human_bytes, Finding};
use forge_ui::{Node, Rect};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::app::Live;
use crate::browse::Listing;
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
    pub cancelled: bool,
    pub persisted: bool,
    pub trustworthy: bool,
    pub ephemeral: bool,
}

// ── shared furniture ─────────────────────────────────────────────────────────

/// Wordmark plus a right-hand slot. Present on every screen so the app never
/// loses its identity mid-task.
fn masthead(id: &str, subtitle: String, right: Option<(String, String)>) -> Node {
    Node::custom(id, "disc0", false, move |p, r, _t| {
        p.text("disc0", r.x, r.y, 30.0, CREAM, true);
        let w = p.measure("disc0", 30.0, true);
        p.text(&subtitle, r.x + w + 14.0, r.y + 12.0, 13.0, SAGE, false);
        if let Some((big, small)) = &right {
            let tw = p.measure(big, 30.0, true);
            p.text(big, r.x + r.w - tw, r.y - 2.0, 30.0, BRASS, true);
            let sw = p.measure(small, 10.0, false);
            p.text(small, r.x + r.w - sw, r.y + 30.0, 10.0, SAGE.mix(FIELD, 0.2), false);
        }
    })
    .height(46.0)
}

fn hairline(id: &str) -> Node {
    Node::custom(id, "", false, |p, r, _t| {
        p.rect(Rect::new(r.x, r.y, r.w, 1.0), BRASS.mix(FIELD, 0.45), 0.0);
    })
    .height(1.0)
}

fn stat(id: &str, label: &'static str, value: String) -> Node {
    Node::custom(id, label, false, move |p, r, _t| {
        p.text(&tracked(label), r.x, r.y, 9.0, SAGE.mix(FIELD, 0.3), false);
        p.text(&value, r.x, r.y + 14.0, 17.0, CREAM, true);
    })
    .height(38.0)
}

/// The promise, restated on every screen. Trust is not a one-time banner.
fn promise_bar(id: &str, extra: Option<(String, forge_ui::Color)>) -> Node {
    Node::custom(id, "", false, move |p, r, _t| {
        p.rect(Rect::new(r.x, r.y, r.w, 1.0), BRASS.mix(FIELD, 0.35), 0.0);
        let badge = " READ-ONLY ";
        let bw = p.measure(badge, 9.0, true) + 6.0;
        p.rect(Rect::new(r.x, r.y + 12.0, bw, 15.0), TEAL.mix(FIELD, 0.5), 7.0);
        p.text(badge, r.x + 3.0, r.y + 15.0, 9.0, CREAM, true);
        p.text(
            "disc0 reads. It never moves, changes or removes a file.",
            r.x + bw + 10.0,
            r.y + 15.0,
            10.0,
            SAGE.mix(FIELD, 0.25),
            false,
        );
        if let Some((msg, c)) = &extra {
            let w = p.measure(msg, 10.0, true);
            p.text(msg, r.x + r.w - w, r.y + 15.0, 10.0, *c, true);
        }
    })
    .height(34.0)
}

fn signature(id: &str) -> Node {
    let sig = format!(
        "disc0 v{} · {} · nedb-engine {} · {} → {} on {}",
        disc0::brand::VERSION,
        disc0::brand::GIT_SHA,
        disc0::brand::ENGINE,
        disc0::brand::LICENSE,
        disc0::brand::CHANGE_LICENSE,
        disc0::brand::CHANGE_DATE,
    );
    Node::custom(id, "", false, move |p, r, _t| {
        let w = p.measure(&sig, 9.0, false);
        p.text(&sig, r.x + r.w - w, r.y, 9.0, SAGE.mix(FIELD, 0.55), false);
    })
    .height(14.0)
}

// ── screen 1: browse ─────────────────────────────────────────────────────────

pub fn browse_screen(cwd: &Path, listing: &Listing, ephemeral: bool) -> Node {
    let path = cwd.display().to_string();
    let n_dirs = listing.dirs.len();
    let n_files = listing.file_count;
    let err = listing.error.clone();

    let mut kids = vec![
        masthead("masthead", "choose what to look at".into(), None),
        hairline("rule1"),
        Node::custom("crumb", "", false, move |p, r, _t| {
            p.text(&tracked("scan root"), r.x, r.y, 9.0, SAGE.mix(FIELD, 0.35), true);
            p.text(&path, r.x, r.y + 15.0, 15.0, CREAM, true);
            let meta = if let Some(e) = &err {
                format!("unreadable — {e}")
            } else {
                format!("{n_dirs} folders · {n_files} files here")
            };
            let c = if err.is_some() { COPPER } else { SAGE.mix(FIELD, 0.25) };
            p.text(&meta, r.x, r.y + 36.0, 11.0, c, err.is_some());
        })
        .height(56.0),
        Node::row(
            "nav",
            vec![
                Node::button("scan_start", "Scan this folder").width(190.0),
                Node::button("up", "Parent folder").width(140.0),
                Node::button("go_home", "Home").width(90.0),
                Node::button("go_root", "/").width(60.0),
                // absorb the remaining width so the buttons stay left-aligned
                Node::label("navpad", ""),
            ],
        )
        .gap(10.0),
        Node::custom("dirhdr", "", false, |p, r, _t| {
            p.text(
                &tracked("folders — click to open"),
                r.x + 14.0,
                r.y,
                9.0,
                SAGE.mix(FIELD, 0.4),
                true,
            );
        })
        .height(14.0),
    ];

    // A scrollable list, so a big home directory does not blow the layout.
    let mut rows: Vec<Node> = Vec::new();
    for (i, d) in listing.dirs.iter().enumerate().take(200) {
        let name = d.name.clone();
        let readable = d.readable;
        let children = d.children;
        rows.push(
            Node::custom(format!("dir_{i}"), name.clone(), true, move |p, r, _t| {
                let fg = if readable { CREAM } else { SAGE.mix(FIELD, 0.45) };
                p.rect(Rect::new(r.x, r.y, 2.0, r.h), BRASS.mix(FIELD, 0.6), 0.0);
                p.text(&name, r.x + 14.0, r.y + 6.0, 14.0, fg, false);
                let note = match children {
                    Some(n) => format!("{n} items"),
                    None => "unreadable — will be reported as a coverage gap".into(),
                };
                let c = if readable { SAGE.mix(FIELD, 0.4) } else { COPPER };
                let w = p.measure(&note, 10.0, false);
                p.text(&note, r.x + r.w - 14.0 - w, r.y + 8.0, 10.0, c, false);
            })
            .height(28.0),
        );
    }
    if rows.is_empty() {
        rows.push(
            Node::custom("nodirs", "", false, |p, r, _t| {
                p.text(
                    "no sub-folders here — scanning this folder will still measure its files",
                    r.x + 14.0,
                    r.y + 6.0,
                    12.0,
                    SAGE,
                    false,
                );
            })
            .height(28.0),
        );
    }
    kids.push(Node::scroll("dirs", rows).fill_height());

    let extra = if ephemeral {
        Some((
            "ephemeral — this session saves no history".to_string(),
            BRASS,
        ))
    } else {
        None
    };
    kids.push(promise_bar("promise", extra));
    kids.push(signature("sig"));
    Node::column("root", kids).padding(28.0).gap(12.0)
}

// ── screen 2: scanning ───────────────────────────────────────────────────────

pub fn scanning_screen(cwd: &Path, live: &Live, elapsed: Duration) -> Node {
    let entries = live.entries.load(Ordering::Relaxed);
    let files = live.files.load(Ordering::Relaxed);
    let bytes = live.bytes.load(Ordering::Relaxed);
    let errors = live.errors.load(Ordering::Relaxed);
    let current = live
        .current
        .lock()
        .map(|c| c.clone())
        .unwrap_or_default();
    let secs = elapsed.as_secs_f64().max(0.001);
    let rate = entries as f64 / secs;
    let root = cwd.display().to_string();

    let mut kids = vec![
        masthead(
            "masthead",
            root,
            Some((human_bytes(bytes), "discovered so far".into())),
        ),
        hairline("rule1"),
        // The sweep. Indeterminate on purpose: we do not know the total up
        // front, and a progress bar that invents a percentage is a lie. It
        // moves with the work, and the numbers beside it are the real signal.
        Node::custom("sweep", "", false, move |p, r, _t| {
            let track = Rect::new(r.x, r.y + 10.0, r.w, 6.0);
            p.rect(track, FIELD.mix(SAGE, 0.12), 3.0);
            // position derives from entries seen, so it advances with progress
            // rather than with wall-clock time — it cannot look busy while
            // stalled.
            let span = r.w * 0.22;
            let t = ((entries as f32 / 900.0) % 1.0) * (r.w + span) - span;
            let x = t.max(r.x - r.x);
            let w = span.min(r.w - x).max(0.0);
            if w > 0.0 {
                p.rect(Rect::new(r.x + x, r.y + 10.0, w, 6.0), BRASS, 3.0);
            }
            let label = tracked("reading");
            p.text(&label, r.x, r.y + 24.0, 9.0, SAGE.mix(FIELD, 0.35), true);
            // Measure rather than guess: the tracked label is wider than it
            // looks and a fixed offset ran it straight into the path.
            let lw = p.measure(&label, 9.0, true) + 12.0;
            let short = truncate_middle(&current, 92);
            p.text(&short, r.x + lw, r.y + 24.0, 10.0, SAGE, false);
        })
        .height(44.0),
        Node::row(
            "stats",
            vec![
                stat("s_entries", "entries", commas(entries as u64)),
                stat("s_files", "files", commas(files as u64)),
                stat("s_rate", "per second", commas(rate as u64)),
                stat("s_elapsed", "elapsed", format!("{:.1}s", secs)),
                stat("s_gaps", "coverage gaps", commas(errors as u64)),
            ],
        )
        .gap(26.0),
        Node::row(
            "controls",
            vec![
                Node::button("scan_stop", "Stop scan").width(140.0),
                Node::label("ctlpad", ""),
            ],
        )
        .gap(10.0),
        Node::custom("reassure", "", false, |p, r, _t| {
            p.text(
                "Stopping is safe. A stopped scan is kept and clearly labelled incomplete —",
                r.x,
                r.y,
                11.0,
                SAGE.mix(FIELD, 0.2),
                false,
            );
            p.text(
                "it is never accepted as a baseline for comparison.",
                r.x,
                r.y + 15.0,
                11.0,
                SAGE.mix(FIELD, 0.2),
                false,
            );
        })
        .height(34.0),
    ];
    kids.push(Node::column("spacer", vec![]).fill_height());
    kids.push(promise_bar("promise", None));
    kids.push(signature("sig"));
    Node::column("root", kids).padding(28.0).gap(12.0)
}

// ── screen 3: results ────────────────────────────────────────────────────────

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
        let x = r.x + 16.0;
        p.text(&shown, x, r.y + 7.0, 15.0, CREAM, selected);

        let pill = format!(" {ev} ");
        let pw = p.measure(&pill, 9.0, true) + 8.0;
        let py = r.y + 28.0;
        p.rect(Rect::new(x, py, pw, 14.0), ev_color.mix(FIELD, 0.72), 7.0);
        p.text(&pill, x + 4.0, py + 2.5, 9.0, ev_color, true);
        let meta = format!("{owner} · {} files", commas(files as u64));
        p.text(&meta, x + pw + 8.0, py + 2.0, 10.0, SAGE.mix(FIELD, 0.3), false);

        let bar_x = r.x + r.w * 0.47;
        let bar_w = r.w * 0.30;
        let by = r.y + r.h * 0.5 - 8.0;
        p.rect(Rect::new(bar_x, by, bar_w, 8.0), FIELD.mix(SAGE, 0.14), 4.0);
        let frac = if max == 0 { 0.0 } else { bytes as f32 / max as f32 };
        p.rect(Rect::new(bar_x, by, (bar_w * frac).max(2.0), 8.0), accent, 4.0);

        let right = r.x + r.w - 16.0;
        let sw = p.measure(&pretty, 16.0, true);
        p.text(&pretty, right - sw, r.y + 7.0, 16.0, CREAM, true);
        let cw = p.measure(&consequence, 10.0, false);
        p.text(&consequence, right - cw, py + 2.0, 10.0, accent, false);

        if let Some(reason) = &unknown {
            let short: String = reason.chars().take(78).collect();
            p.text(&short, bar_x, by + 12.0, 9.0, COPPER, true);
        }
    })
    .height(52.0)
}

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
        for fact in facts.iter().take(3) {
            let line: String = fact.chars().take(96).collect();
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
        let short: String = joined.chars().take(118).collect();
        p.text(&short, x + 74.0, y, 9.0, SAGE.mix(FIELD, 0.3), false);
    })
    .height(168.0)
}

pub fn results_screen(
    s: &ScanSummary,
    findings: &[Finding],
    selected: Option<usize>,
    receipt: &[(String, String)],
) -> Node {
    let max = findings.iter().map(|f| f.allocated).max().unwrap_or(1);
    let mut kids = vec![
        masthead(
            "masthead",
            s.root.clone(),
            Some((
                human_bytes(s.allocated),
                format!("allocated · {} logical", human_bytes(s.logical)),
            )),
        ),
        hairline("rule1"),
        Node::row(
            "stats",
            vec![
                stat("s_files", "files", commas(s.files as u64)),
                stat("s_entries", "entries", commas(s.entries as u64)),
                stat("s_scan", "scan", format!("{}ms", s.scan_ms)),
                stat("s_find", "findings", commas(findings.len() as u64)),
            ],
        )
        .gap(28.0),
        Node::row(
            "controls",
            vec![
                Node::button("back_to_browse", "Choose another folder").width(200.0),
                Node::button("scan_start", "Scan again").width(130.0),
                Node::label("ctlpad", ""),
            ],
        )
        .gap(10.0),
        column_header(),
    ];

    if findings.is_empty() {
        kids.push(
            Node::custom("empty", "", false, |p, r, _t| {
                p.text(
                    "No recognized build artifacts here. That is a clean result, not a failure.",
                    r.x + 16.0,
                    r.y + 8.0,
                    13.0,
                    SAGE,
                    false,
                );
            })
            .height(36.0),
        );
    }

    let root = s.root.clone();
    let mut rows: Vec<Node> = Vec::new();
    for (i, f) in findings.iter().enumerate().take(60) {
        rows.push(finding_row(i, f, &root, max, selected == Some(i)));
    }
    let selected_open = selected.is_some();
    if !rows.is_empty() {
        // With a detail panel open the list is capped so the panel sits
        // directly under its row; with nothing selected the list takes the
        // space it deserves.
        let list = Node::scroll("rows", rows);
        kids.push(if selected_open {
            list.height(((findings.len().min(5) as f32) * 52.0).max(52.0))
        } else {
            list.fill_height()
        });
    }
    if let Some(i) = selected {
        if let Some(f) = findings.get(i) {
            kids.push(detail(f, receipt));
        }
        kids.push(Node::column("tail", vec![]).fill_height());
    }

    // Status the user must never have to guess at.
    let extra = if s.cancelled {
        Some((
            format!("STOPPED EARLY · {} gaps · totals are a lower bound", s.coverage_errors),
            BRASS,
        ))
    } else if !s.coverage_complete {
        Some((
            format!("partial coverage · {} gaps · lower bound", s.coverage_errors),
            COPPER,
        ))
    } else if s.ephemeral {
        Some(("ephemeral — nothing saved".to_string(), BRASS))
    } else if !s.persisted {
        Some(("NOT SAVED — not a usable baseline".to_string(), COPPER))
    } else if s.trustworthy {
        Some(("saved · baseline verified".to_string(), SAGE.mix(FIELD, 0.2)))
    } else {
        Some(("baseline unreadable — run nedb-cli repair".to_string(), COPPER))
    };
    kids.push(promise_bar("promise", extra));
    kids.push(signature("sig"));
    Node::column("root", kids).padding(28.0).gap(12.0)
}

// ── screen 4: failure, stated plainly ────────────────────────────────────────

pub fn failed_screen(cwd: &Path, msg: &str) -> Node {
    let path = cwd.display().to_string();
    let msg = msg.to_string();
    Node::column(
        "root",
        vec![
            masthead("masthead", path, None),
            hairline("rule1"),
            Node::custom("err", "", false, move |p, r, _t| {
                p.text(&tracked("the scan could not finish"), r.x, r.y, 9.0, COPPER, true);
                let mut y = r.y + 18.0;
                for line in wrap(&msg, 96).iter().take(5) {
                    p.text(line, r.x, y, 12.0, CREAM, false);
                    y += 17.0;
                }
                p.text(
                    "Nothing was modified. You can choose another folder and try again.",
                    r.x,
                    y + 8.0,
                    11.0,
                    SAGE,
                    false,
                );
            })
            .height(140.0),
            Node::row(
                "controls",
                vec![
                    Node::button("back_to_browse", "Choose another folder").width(200.0),
                    Node::label("errpad", ""),
                ],
            )
            .gap(10.0),
            Node::column("spacer", vec![]).fill_height(),
            promise_bar("promise", None),
            signature("sig"),
        ],
    )
    .padding(28.0)
    .gap(12.0)
}

// ── text helpers ─────────────────────────────────────────────────────────────

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

/// Keep head and tail; the middle of a path is the least informative part.
fn truncate_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let tail = keep * 2 / 3;
    let head = keep - tail;
    let h: String = s.chars().take(head).collect();
    let t: String = s.chars().skip(n - tail).collect();
    format!("{h}…{t}")
}
