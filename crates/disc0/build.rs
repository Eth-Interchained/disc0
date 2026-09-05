//! Capture build provenance at compile time.
//!
//! Deliberately NO wall-clock timestamp: a build time makes two builds of the
//! same commit produce different binaries, which breaks reproducible builds
//! and makes "is this the binary I think it is?" unanswerable. The commit is
//! the identity; its date is the date.
use std::process::Command;

fn run(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../Cargo.lock");

    let sha = run("git", &["rev-parse", "--short=9", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let dirty = run("git", &["status", "--porcelain"])
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let sha = if dirty { format!("{sha}+dirty") } else { sha };
    let date = run("git", &["log", "-1", "--format=%cs"]).unwrap_or_else(|| "unknown".into());
    let rustc = run("rustc", &["--version"])
        .map(|s| s.split_whitespace().nth(1).unwrap_or("?").to_string())
        .unwrap_or_else(|| "unknown".into());

    // Engine version straight out of the lockfile — the actual resolved
    // dependency, not a string we hope matches the manifest.
    let engine = std::fs::read_to_string("../../Cargo.lock")
        .ok()
        .and_then(|lock| {
            let i = lock.find("name = \"nedb-engine\"")?;
            let rest = &lock[i..];
            let j = rest.find("version = \"")? + "version = \"".len();
            let k = rest[j..].find('"')?;
            Some(rest[j..j + k].to_string())
        })
        .unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=DISC0_GIT_SHA={sha}");
    println!("cargo:rustc-env=DISC0_GIT_DATE={date}");
    println!("cargo:rustc-env=DISC0_RUSTC={rustc}");
    println!("cargo:rustc-env=DISC0_TARGET={}", std::env::var("TARGET").unwrap_or_default());
    println!("cargo:rustc-env=DISC0_PROFILE={}", std::env::var("PROFILE").unwrap_or_default());
    println!("cargo:rustc-env=DISC0_ENGINE={engine}");
}
