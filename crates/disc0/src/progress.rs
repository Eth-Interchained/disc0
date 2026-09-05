//! Live terminal progress.
//!
//! Everything here writes to STDERR. stdout carries the report (or the JSON
//! document) and must never be interleaved with status frames — a progress
//! spinner in the middle of a JSON object is a parse error.
//!
//! On a non-TTY stderr (CI, piped to a file) the animated line is replaced by
//! occasional plain milestones, so logs stay readable instead of filling with
//! carriage returns and escape codes.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

const SPIN: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];

pub struct Reporter {
    tty: bool,
    color: bool,
    start: Instant,
    last_draw: Instant,
    frame: usize,
    last_line_len: usize,
    quiet: bool,
}

impl Reporter {
    pub fn new(quiet: bool) -> Self {
        let tty = std::io::stderr().is_terminal();
        Self {
            tty,
            color: tty && crate::brand::color_enabled_stderr(),
            start: Instant::now(),
            last_draw: Instant::now() - Duration::from_secs(1),
            frame: 0,
            last_line_len: 0,
            quiet,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    fn dim(&self, s: &str) -> String {
        if self.color { format!("\x1b[38;5;244m{s}\x1b[0m") } else { s.to_string() }
    }
    fn sage(&self, s: &str) -> String {
        if self.color { format!("\x1b[38;5;108m{s}\x1b[0m") } else { s.to_string() }
    }
    fn brass(&self, s: &str) -> String {
        if self.color { format!("\x1b[38;5;179m{s}\x1b[0m") } else { s.to_string() }
    }

    /// A named phase is starting.
    pub fn phase(&mut self, name: &str, detail: &str) {
        if self.quiet {
            return;
        }
        self.clear();
        let _ = writeln!(
            std::io::stderr(),
            "  {} {} {}",
            self.brass("▸"),
            self.sage(name),
            self.dim(detail)
        );
    }

    /// A phase finished, with how long it took.
    pub fn phase_done(&mut self, name: &str, detail: &str, took: Duration) {
        if self.quiet {
            return;
        }
        self.clear();
        let _ = writeln!(
            std::io::stderr(),
            "  {} {} {} {}",
            self.sage("✓"),
            self.sage(name),
            self.dim(detail),
            self.dim(&format!("({})", fmt_dur(took)))
        );
    }

    /// Animated single-line tick. Throttled to ~12 fps; a progress indicator
    /// that costs real time is a bug, not a feature.
    pub fn tick(&mut self, entries: usize, files: usize, bytes: u64, errors: usize, current: &str) {
        if self.quiet {
            return;
        }
        if self.last_draw.elapsed() < Duration::from_millis(80) {
            return;
        }
        self.last_draw = Instant::now();
        let secs = self.start.elapsed().as_secs_f64().max(0.001);
        let rate = entries as f64 / secs;

        if !self.tty {
            // Non-TTY: milestone lines only, every 25k entries.
            if entries % 25_000 == 0 && entries > 0 {
                let _ = writeln!(
                    std::io::stderr(),
                    "  scanning… {entries} entries, {files} files, {} ({:.0}/s)",
                    crate::human_bytes(bytes),
                    rate
                );
            }
            return;
        }

        self.frame = (self.frame + 1) % SPIN.len();
        let err = if errors > 0 {
            format!("  {} {errors}", self.brass("⚠"))
        } else {
            String::new()
        };
        let line = format!(
            "  {} {:>9} entries · {:>8} files · {:>10} · {:>7}/s{}   {}",
            self.brass(SPIN[self.frame]),
            entries,
            files,
            crate::human_bytes(bytes),
            format!("{rate:.0}"),
            err,
            self.dim(&truncate_middle(current, 46)),
        );
        let visible = strip_ansi_len(&line);
        let pad = self.last_line_len.saturating_sub(visible);
        let _ = write!(std::io::stderr(), "\r{line}{}", " ".repeat(pad));
        let _ = std::io::stderr().flush();
        self.last_line_len = visible;
    }

    /// Wipe the animated line so the next output starts on a clean row.
    pub fn clear(&mut self) {
        if self.tty && self.last_line_len > 0 {
            let _ = write!(std::io::stderr(), "\r{}\r", " ".repeat(self.last_line_len));
            let _ = std::io::stderr().flush();
            self.last_line_len = 0;
        }
    }
}

fn fmt_dur(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}m{:02}s", d.as_secs() / 60, d.as_secs() % 60)
    }
}

/// Keep the head and tail of a path — the middle is the least informative part.
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

/// Visible width, ignoring ANSI SGR sequences, so padding math is correct when
/// colour is on.
fn strip_ansi_len(s: &str) -> usize {
    let mut n = 0;
    let mut in_esc = false;
    for ch in s.chars() {
        if in_esc {
            if ch == 'm' {
                in_esc = false;
            }
            continue;
        }
        if ch == '\x1b' {
            in_esc = true;
            continue;
        }
        n += 1;
    }
    n
}
