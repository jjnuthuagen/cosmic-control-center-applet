//! Control Center — a COSMIC panel applet for the settings you reach for daily.
//!
//! One panel button opens a grid of tiles (Wi-Fi, Bluetooth, battery power
//! profile, DNS) over sliders for volume and brightness. Each tile is backed by
//! its own module under [`modules`], hides itself when its hardware or daemon is
//! absent, and can be switched off entirely in `config.toml`.

mod app;
mod check;
mod config;
mod i18n;
mod modules;
#[allow(dead_code)] // items are wired into the popup as the grid rework lands.
mod tile_layout;
mod process;
mod settings;
mod single_instance;
mod ui;

fn main() -> cosmic::iced::Result {
    // `RUST_LOG=debug` is the way to see why a module decided it was
    // unavailable — those paths log at debug and are silent by default, because
    // "no battery on this machine" is not something to warn about every second.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    // Flipping tiling without a GUI: scriptable, and safe to bind to a key.
    // It is also how the Wayland write path gets verified, since a hermetic
    // test cannot talk to a compositor.
    if std::env::args().skip(1).any(|arg| arg == "--toggle-tiling") {
        match modules::tiling::toggle_now() {
            Ok(tiled) => {
                println!("{}", if tiled { "tiled" } else { "floating" });
                std::process::exit(0);
            }
            Err(err) => {
                eprintln!("could not toggle tiling: {err}");
                std::process::exit(2);
            }
        }
    }

    // `--settings` runs the same binary as an ordinary window instead of an
    // applet. The panel button spawns this on right-click.
    if std::env::args().skip(1).any(|arg| arg == "--settings") {
        // Right-clicking the panel button spawns this every time, so the second
        // and later invocations bow out after asking the open window to come
        // forward. Without it, ten right-clicks leave ten windows.
        if single_instance::claim() == single_instance::Claim::AlreadyOpen {
            return Ok(());
        }
        return cosmic::app::run::<settings::Settings>(settings::window_settings(), ());
    }

    // `--icons` shows what the active icon theme resolves each state to.
    if std::env::args().skip(1).any(|arg| arg == "--icons") {
        std::process::exit(check::icon_report());
    }

    // `--check` probes every backend and prints what it found, without opening
    // a surface. See `check.rs` for why an applet needs this.
    if std::env::args().skip(1).any(|arg| arg == "--check") {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                eprintln!("could not start a runtime: {err}");
                std::process::exit(2);
            }
        };
        std::process::exit(runtime.block_on(check::run()));
    }

    cosmic::applet::run::<app::App>(())
}
