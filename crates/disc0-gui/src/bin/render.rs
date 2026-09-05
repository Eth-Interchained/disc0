//! Headless renderer: scan a real tree, render the REAL view, write a PPM.
//!
//! No window, no display server, no GPU. This is what makes the GUI reviewable
//! without a screen, and what a golden-image test would hang off — the paint
//! output is byte-deterministic, so a layout regression is a hash mismatch.
//!
//!     disc0-gui-render <path> [out.ppm]

use disc0_gui::{app::App, theme, view};
use forge_ui::{Application, Ui};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let out = args.next().unwrap_or_else(|| "/tmp/disc0_gui.ppm".into());

    // Ephemeral: rendering a picture must never write a baseline.
    let mut app = App::scan(root, true)?;
    app.select(Some(0));

    let mut ui = Ui::new(app.view()).map_err(|e| anyhow::anyhow!(e))?;
    ui.theme = theme::theme();
    ui.resize(1040, 760, 1.0);
    ui.layout();
    ui.paint();

    let first = ui.painter.pixels.clone();
    ui.paint();
    let deterministic = first == ui.painter.pixels;

    ui.painter.save_ppm(std::path::Path::new(&out))?;
    println!(
        "rendered {} findings · {} items · repaint identical: {}",
        app.findings.len(),
        ui.items.len(),
        if deterministic { "YES" } else { "NO" }
    );
    println!("wrote {out}");
    let _ = view::screen; // keep the shared path referenced explicitly
    Ok(())
}
