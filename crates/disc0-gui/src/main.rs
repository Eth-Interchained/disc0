//! disc0-gui — the native findings window.
//!
//! READ-ONLY, exactly like the CLI. There is no delete control anywhere in
//! this binary, and there will not be one until the plan/validate/authorize
//! chain exists and has been adversarially tested. A GUI makes destructive
//! actions one careless click away, which is precisely why it goes last.

use disc0_gui::app;

use forge_ui::WindowOptions;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut root: Option<std::path::PathBuf> = None;
    let mut ephemeral = false;
    for a in args.by_ref() {
        match a.as_str() {
            "--ephemeral" => ephemeral = true,
            s if s.starts_with("--") => {}
            s => root = Some(std::path::PathBuf::from(s)),
        }
    }
    let root = root.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| ".".into()));

    // The window opens on the browse screen. Nothing is read until the user
    // picks a folder and presses Scan — a disk tool that starts crawling your
    // home directory the moment it launches has already lost your trust.
    let app = app::App::new(root, ephemeral);

    forge_ui::run(
        app,
        WindowOptions {
            title: "disc0".into(),
            width: 1040.0,
            height: 720.0,
            ..Default::default()
        },
    )
    .map_err(|e| anyhow::anyhow!("{e}"))
}
