//! `--check`: probe every backend against the live system and report.
//!
//! An applet is awkward to test. Most of what can go wrong is not in the widget
//! tree — it is a D-Bus name that is not owned, a daemon that is not running, a
//! sysfs path that does not exist on this hardware. Those failures show up in
//! the UI only as a tile that quietly is not there, which is indistinguishable
//! from the module being switched off.
//!
//! So the same read paths the applet uses are exposed as one-shot probes, and
//! this prints what each one found. It is the first thing to run when a tile is
//! missing, and it is what makes "does this work on someone else's machine?" a
//! question with an answer.

use crate::config::Config;
use crate::modules::{battery, bluetooth, brightness, dns, gamemode, network, system, volume};

/// Exit code when at least one enabled module could not be read.
///
/// Non-zero so this is usable in CI and in a bug report template, not just by
/// eye.
const EXIT_DEGRADED: i32 = 1;

struct Report {
    name: &'static str,
    enabled: bool,
    result: Result<String, String>,
}

impl Report {
    fn line(&self) -> String {
        if !self.enabled {
            return format!("  {:<11} disabled in config.toml", self.name);
        }
        match &self.result {
            Ok(detail) => format!("  {:<11} ok       {detail}", self.name),
            Err(reason) => format!("  {:<11} MISSING  {reason}", self.name),
        }
    }

    /// A disabled module is not a failure, and neither is absent hardware that
    /// the module correctly reports. Only an enabled module that cannot be read
    /// counts against the exit code.
    fn is_failure(&self) -> bool {
        self.enabled && self.result.is_err()
    }
}

pub async fn run() -> i32 {
    let config = Config::load();

    println!("Control Center {}", env!("CARGO_PKG_VERSION"));
    match Config::path() {
        Some(path) if path.exists() => println!("config: {}", path.display()),
        Some(path) => println!("config: {} (absent, using defaults)", path.display()),
        None => println!("config: no config directory"),
    }
    println!("\nmodules:");

    let reports = vec![
        Report {
            name: "wifi",
            enabled: config.modules.wifi,
            result: network::probe().await,
        },
        Report {
            name: "bluetooth",
            enabled: config.modules.bluetooth,
            result: bluetooth::probe().await,
        },
        Report {
            name: "battery",
            enabled: config.modules.battery,
            result: battery::probe().await,
        },
        Report {
            name: "dns",
            enabled: config.modules.dns,
            result: dns::probe().await,
        },
        Report {
            name: "volume",
            enabled: config.modules.volume,
            result: volume::probe().await,
        },
        Report {
            name: "brightness",
            enabled: config.modules.brightness,
            result: brightness::probe(),
        },
        Report {
            name: "gamemode",
            enabled: config.modules.gamemode,
            result: gamemode::probe().await,
        },
        Report {
            name: "desktop",
            enabled: config.modules.dark_mode || config.modules.tiling,
            result: system::probe(),
        },
    ];

    for report in &reports {
        println!("{}", report.line());
    }

    let failures = reports.iter().filter(|r| r.is_failure()).count();
    println!();
    if failures == 0 {
        println!("all enabled modules readable");
        0
    } else {
        // Deliberately not phrased as an error: on a desktop, "wifi MISSING" is
        // the correct and expected answer, and the applet handles it by hiding
        // the tile. The exit code exists for scripts, not to alarm the reader.
        println!("{failures} module(s) unreadable — those tiles will not appear");
        EXIT_DEGRADED
    }
}
