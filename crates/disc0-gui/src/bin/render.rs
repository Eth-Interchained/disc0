//! Headless renderer: paint any screen of the REAL view to a PPM.
//!
//! No window, no display server, no GPU. Same view code the window runs, so a
//! screenshot cannot drift from what ships.
//!
//!     disc0-gui-render <path> <browse|scan|results> [out.ppm]

use disc0_gui::app::App;
use disc0_gui::theme;
use forge_ui::{Action, ActionKind, Application, Ui};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = args
        .next()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap());
    let screen = args.next().unwrap_or_else(|| "results".into());
    let out = args.next().unwrap_or_else(|| format!("/tmp/disc0_{screen}.ppm"));

    // Ephemeral: rendering a picture must never write a baseline.
    let mut app = App::new(root, true);

    match screen.as_str() {
        "browse" => {}
        "scan" => {
            app.update(Action { id: "scan_start".into(), kind: ActionKind::Activate });
            // let the worker get far enough to have something to show
            std::thread::sleep(std::time::Duration::from_millis(120));
        }
        _ => {
            app.update(Action { id: "scan_start".into(), kind: ActionKind::Activate });
            for _ in 0..600 {
                app.poll();
                if !matches!(app.phase, disc0_gui::app::Phase::Scanning { .. }) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            app.update(Action { id: "row_0".into(), kind: ActionKind::Activate });
        }
    }

    let mut ui = Ui::new(app.view()).map_err(|e| anyhow::anyhow!(e))?;
    ui.theme = theme::theme();
    ui.resize(1120, 800, 1.0);
    ui.layout();
    ui.paint();
    let first = ui.painter.pixels.clone();
    ui.paint();
    println!(
        "screen={screen} items={} repaint_identical={}",
        ui.items.len(),
        first == ui.painter.pixels
    );
    ui.painter.save_ppm(std::path::Path::new(&out))?;
    println!("wrote {out}");
    Ok(())
}
