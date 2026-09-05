//! Identity, provenance and signature.
//!
//! Two rules, both load-bearing:
//!
//! 1. The banner NEVER touches stdout when machine output is requested. A
//!    JSON document polluted by a wordmark is a parse error, and every
//!    consumer would have to learn to strip it.
//! 2. Colour is opt-out AND auto-off when stdout is not a terminal, so piping
//!    into a file or a pager does not embed escape codes.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_SHA: &str = env!("DISC0_GIT_SHA");
pub const GIT_DATE: &str = env!("DISC0_GIT_DATE");
pub const RUSTC: &str = env!("DISC0_RUSTC");
pub const TARGET: &str = env!("DISC0_TARGET");
pub const PROFILE: &str = env!("DISC0_PROFILE");
pub const ENGINE: &str = env!("DISC0_ENGINE");
pub const LICENSE: &str = "BUSL-1.1";
/// BUSL converts to this on the Change Date — stated so a user of a shipped
/// binary can see the terms they will eventually receive, not just today's.
pub const CHANGE_LICENSE: &str = "GPL-3.0-only";
pub const CHANGE_DATE: &str = "2030-09-05";
pub const REPO: &str = "https://github.com/Eth-Interchained/disc0";

/// 256-colour approximations of the house palette. Chosen over 24-bit truecolour
/// so the banner survives terminals that only speak 256.
mod c {
    pub const BRASS: &str = "\x1b[38;5;179m";
    pub const CREAM: &str = "\x1b[38;5;255m";
    pub const SAGE: &str = "\x1b[38;5;108m";
    pub const TEAL: &str = "\x1b[38;5;30m";
    pub const DIM: &str = "\x1b[38;5;244m";
    pub const RESET: &str = "\x1b[0m";
}

/// Colour is used only when stdout is a TTY and no opt-out is set.
/// Honors NO_COLOR (informal standard) and TERM=dumb.
pub fn color_enabled() -> bool {
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("DISC0_NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
        return false;
    }
    unsafe { libc::isatty(1) == 1 }
}

/// Same policy as [`color_enabled`] but tested against stderr, where progress
/// is written.
pub fn color_enabled_stderr() -> bool {
    if std::env::var_os("NO_COLOR").is_some() || std::env::var_os("DISC0_NO_COLOR").is_some() {
        return false;
    }
    if matches!(std::env::var("TERM").as_deref(), Ok("dumb")) {
        return false;
    }
    unsafe { libc::isatty(2) == 1 }
}

struct Paint(bool);
impl Paint {
    fn w(&self, code: &str, s: &str) -> String {
        if self.0 {
            format!("{code}{s}{}", c::RESET)
        } else {
            s.to_string()
        }
    }
}

/// The wordmark, figlet "standard". Assembled per-glyph rather than
/// hand-drawn: an earlier hand-typed version repeated the `s` glyph where the
/// `c` belonged, so it silently spelled "diss0".
const MARK: [&str; 6] = [
    r"     _  _              ___  ",
    r"  __| |(_) ___   ___  / _ \ ",
    r" / _` || |/ __| / __|| | | |",
    r"| (_| || |\__ \| (__ | |_| |",
    r" \__,_||_||___/ \___| \___/ ",
    r"                            ",
];

/// Full banner: wordmark, tagline, provenance, signature.
pub fn banner() -> String {
    let p = Paint(color_enabled());
    let mut s = String::new();
    s.push('\n');
    for (i, line) in MARK.iter().enumerate() {
        // fade the mark from cream into brass down the glyph body
        let code = if i < 2 { c::CREAM } else if i < 5 { c::BRASS } else { c::DIM };
        s.push_str(&p.w(code, line));
        if i == 2 {
            s.push_str(&p.w(c::SAGE, "   what is using my disk, what grew,"));
        }
        if i == 3 {
            s.push_str(&p.w(c::SAGE, "   who owns it, and what happens if"));
        }
        if i == 4 {
            s.push_str(&p.w(c::SAGE, "   I remove it."));
        }
        s.push('\n');
    }
    s.push_str(&p.w(c::DIM, "  ────────────────────────────────────────────────────────────\n"));
    s.push_str(&format!(
        "  {}  {}   {}\n",
        p.w(c::CREAM, &format!("v{VERSION}")),
        p.w(c::DIM, &format!("{GIT_SHA} · {GIT_DATE}")),
        p.w(c::TEAL, "READ-ONLY"),
    ));
    s.push_str(&format!(
        "  {}\n",
        p.w(c::DIM, &format!("engine nedb-engine {ENGINE} · rustc {RUSTC} · {TARGET}")),
    ));
    s.push_str(&format!(
        "  {}\n",
        p.w(c::DIM, &format!("{LICENSE} → {CHANGE_LICENSE} on {CHANGE_DATE} · Interchained")),
    ));
    s
}

/// One-line signature for the foot of a human report: what produced this
/// output, and from which exact tree. Someone holding a pasted report can
/// reproduce it.
pub fn signature() -> String {
    let p = Paint(color_enabled());
    p.w(
        &c::DIM.to_string(),
        &format!("disc0 v{VERSION} ({GIT_SHA}) · nedb-engine {ENGINE} · read-only"),
    )
}

/// Machine-readable provenance, for `version --json` and embedding in scan
/// records so a stored finding names the exact build that produced it.
pub fn provenance_json() -> serde_json::Value {
    serde_json::json!({
        "name": "disc0",
        "version": VERSION,
        "git_sha": GIT_SHA,
        "git_date": GIT_DATE,
        "rustc": RUSTC,
        "target": TARGET,
        "profile": PROFILE,
        "engine": { "name": "nedb-engine", "version": ENGINE },
        "license": LICENSE,
        "change_license": CHANGE_LICENSE,
        "change_date": CHANGE_DATE,
        "repository": REPO,
        "read_only": true,
    })
}
