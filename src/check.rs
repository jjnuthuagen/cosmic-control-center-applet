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
use crate::modules::{
    battery, bluetooth, brightness, caffeine, custom, dns, gamemode, keyboard, media, network,
    system, tiling, volume, vpn,
};
use crate::tile_layout::TileKey;
use crate::ui::icons;

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

/// Whether this control has a consumer: an instance on the grid, or — for the
/// three the Connectivity group draws rows for — that group.
///
/// Mirrors `App::wanted`. `[modules]` no longer decides whether a tile is
/// drawn; being placed does. Reporting from the old table said "disabled in
/// config.toml" about controls the user had simply not placed.
fn wanted(config: &Config, key: TileKey) -> bool {
    let placed = |k: TileKey| config.appearance.layout.iter().any(|i| i.control.is(k));
    let via_group = matches!(key, TileKey::Wifi | TileKey::Bluetooth | TileKey::Vpn)
        && placed(TileKey::Connectivity);
    placed(key) || via_group
}

impl Report {
    fn line(&self) -> String {
        if !self.enabled {
            return format!("  {:<11} not on the grid", self.name);
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

/// `--icons`: print the icon each state resolves to under the active theme.
///
/// Icons are picked from a candidate list and fall back when a theme is missing
/// a name, which means the only way to know what a given theme actually gives
/// you is to ask it. Run this after switching icon themes; anything showing
/// `image-missing-symbolic`, or the same name for states that should differ, is
/// a gap worth reporting.
pub fn icon_report() -> i32 {
    println!("icon theme: {}\n", cosmic::icon_theme::default());

    println!("battery");
    for (percent, charging) in [
        (Some(5.0), false),
        (Some(25.0), false),
        (Some(50.0), false),
        (Some(95.0), false),
        (Some(100.0), false),
        (Some(45.0), true),
        (Some(100.0), true),
        (None, false),
    ] {
        let label = match percent {
            Some(p) => format!("{p:>5.0}%{}", if charging { " charging" } else { "" }),
            None => "  none".to_string(),
        };
        println!("  {label:<16} {}", icons::battery(percent, charging));
    }

    println!("\nbluetooth");
    for (powered, connected) in [(false, 0), (true, 0), (true, 2)] {
        let label = if !powered {
            "off".to_string()
        } else {
            format!("on, {connected} connected")
        };
        println!("  {label:<16} {}", icons::bluetooth(powered, connected));
    }

    println!("\nwifi");
    // Named so the tuple does not read as five anonymous booleans.
    struct WifiCase {
        label: &'static str,
        airplane: bool,
        hardware_killed: bool,
        enabled: bool,
        connected: bool,
        strength: u8,
    }
    let states = [
        WifiCase {
            label: "airplane",
            airplane: true,
            hardware_killed: false,
            enabled: true,
            connected: false,
            strength: 0,
        },
        WifiCase {
            label: "hardware off",
            airplane: false,
            hardware_killed: true,
            enabled: true,
            connected: false,
            strength: 0,
        },
        WifiCase {
            label: "off",
            airplane: false,
            hardware_killed: false,
            enabled: false,
            connected: false,
            strength: 0,
        },
        WifiCase {
            label: "disconnected",
            airplane: false,
            hardware_killed: false,
            enabled: true,
            connected: false,
            strength: 0,
        },
        WifiCase {
            label: "connected weak",
            airplane: false,
            hardware_killed: false,
            enabled: true,
            connected: true,
            strength: 20,
        },
        WifiCase {
            label: "connected strong",
            airplane: false,
            hardware_killed: false,
            enabled: true,
            connected: true,
            strength: 90,
        },
    ];
    for case in states {
        println!(
            "  {:<16} {}",
            case.label,
            icons::wifi(
                case.airplane,
                case.hardware_killed,
                case.enabled,
                case.connected,
                case.strength
            )
        );
    }

    println!("\nnetwork rows");
    for strength in [10u8, 40, 65, 95] {
        println!(
            "  {strength:>3}% open      {}\n  {strength:>3}% secured   {}",
            icons::signal(strength, false),
            icons::signal(strength, true)
        );
    }

    println!("\ntiling");
    println!("  {:<16} {}", "tiled", icons::tiling(true));
    println!("  {:<16} {}", "floating", icons::tiling(false));

    println!("\nsliders");
    println!("  {:<16} {}", "volume muted", icons::volume(50.0, true));
    println!("  {:<16} {}", "volume low", icons::volume(10.0, false));
    println!("  {:<16} {}", "volume high", icons::volume(90.0, false));
    println!(
        "  {:<16} {}",
        "brightness dim",
        icons::brightness(1.0, true)
    );
    println!(
        "  {:<16} {}",
        "brightness high",
        icons::brightness(90.0, false)
    );

    println!("\nfixed");
    println!("  {:<16} {}", "applet", icons::applet());
    println!("  {:<16} {}", "back", icons::back());
    println!("  {:<16} {}", "dns", icons::dns());
    println!("  {:<16} {}", "airplane", icons::airplane());
    println!("  {:<16} {}", "game mode", icons::game_mode());
    for (label, profile) in [
        ("power saver", icons::PowerProfile::PowerSaver),
        ("balanced", icons::PowerProfile::Balanced),
        ("performance", icons::PowerProfile::Performance),
    ] {
        println!("  {label:<16} {}", icons::power_profile(profile));
    }

    0
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
            enabled: wanted(&config, TileKey::Wifi),
            result: network::probe().await,
        },
        Report {
            name: "bluetooth",
            enabled: wanted(&config, TileKey::Bluetooth),
            result: bluetooth::probe().await,
        },
        Report {
            name: "battery",
            enabled: wanted(&config, TileKey::Battery),
            result: battery::probe().await,
        },
        Report {
            name: "dns",
            enabled: wanted(&config, TileKey::Dns),
            result: dns::probe().await,
        },
        Report {
            name: "volume",
            enabled: wanted(&config, TileKey::Volume),
            result: volume::probe(volume::Direction::Output).await,
        },
        Report {
            name: "microphone",
            enabled: wanted(&config, TileKey::Microphone),
            result: volume::probe(volume::Direction::Input).await,
        },
        Report {
            name: "keyboard",
            enabled: wanted(&config, TileKey::KeyboardBacklight),
            result: keyboard::probe(),
        },
        Report {
            name: "brightness",
            enabled: wanted(&config, TileKey::Brightness),
            result: brightness::probe(),
        },
        Report {
            name: "gamemode",
            enabled: config.modules.gamemode,
            result: gamemode::probe().await,
        },
        Report {
            name: "tiling",
            enabled: wanted(&config, TileKey::Tiling),
            result: tiling::probe(),
        },
        Report {
            name: "media",
            enabled: config.modules.media,
            result: media::probe().await,
        },
        Report {
            name: "vpn",
            enabled: wanted(&config, TileKey::Vpn),
            result: vpn::probe().await,
        },
        Report {
            name: "keep-awake",
            enabled: wanted(&config, TileKey::KeepAwake),
            result: caffeine::probe().await,
        },
        Report {
            name: "custom",
            // Always considered on: the list being empty is what turns it off.
            enabled: true,
            result: custom::probe(&config.custom),
        },
        Report {
            name: "desktop",
            enabled: wanted(&config, TileKey::DarkMode),
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
