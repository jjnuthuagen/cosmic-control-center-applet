//! Icon names, chosen for the current state and resolved against whatever icon
//! theme is active.
//!
//! # Two rules
//!
//! **Never name a single icon.** `icon::from_name` renders nothing when the name
//! is absent, so a hardcoded name that a user's theme happens not to ship leaves
//! a blank space with no error anywhere. Every lookup here is a list of
//! candidates, most specific first, resolved through [`resolve`] against the
//! active theme and ending in a name broad enough that any theme has it.
//!
//! **The icon is derived from state, not fixed per tile.** A battery tile that
//! always shows the same outline tells the user nothing they could not read from
//! the percentage; a Wi-Fi tile that looks identical connected and disconnected
//! is worse than no icon. The functions below take the state and pick.

use cosmic::widget::icon;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Last resort. Part of the freedesktop spec, so every theme has it, and it is
/// visibly wrong — which is what you want when a lookup has failed.
const FALLBACK: &str = "image-missing-symbolic";

type Cache = Mutex<HashMap<(String, &'static str), &'static str>>;

fn cache() -> &'static Cache {
    static CACHE: OnceLock<Cache> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The first candidate the active icon theme actually has.
///
/// Results are cached per icon theme: the lookup touches the filesystem, and
/// `view` runs on every frame. Keying on the theme name means switching themes
/// invalidates the cache on its own rather than pinning names from the old one.
pub fn resolve(candidates: &[&'static str]) -> &'static str {
    let Some(&first) = candidates.first() else {
        return FALLBACK;
    };

    let theme = cosmic::icon_theme::default();
    let key = (theme, first);

    // A poisoned cache is not worth propagating: fall through to a live lookup
    // rather than panicking inside a draw.
    if let Ok(cache) = cache().lock() {
        if let Some(&hit) = cache.get(&key) {
            return hit;
        }
    }

    let found = candidates
        .iter()
        .copied()
        .find(|name| icon::from_name(*name).path().is_some())
        // If nothing resolved, keep the most specific name rather than the
        // fallback: themes get updated, and the name documents the intent.
        .unwrap_or(first);

    if let Ok(mut cache) = cache().lock() {
        cache.insert(key, found);
    }
    found
}

/// Battery, by charge and charging state.
///
/// The `battery-level-N` family is the modern freedesktop naming and steps in
/// tens, so the level is rounded to the nearest ten rather than passed through.
/// Older themes only ship the coarse `battery-{empty,caution,low,good,full}`
/// set, which is why those follow as candidates.
pub fn battery(percent: Option<f64>, charging: bool) -> &'static str {
    let Some(percent) = percent else {
        // No battery: a desktop showing power profiles only.
        return resolve(&["battery-missing-symbolic", "battery-symbolic"]);
    };

    let level = bucket(percent);

    if charging {
        return match level {
            100 => resolve(&[
                "battery-level-100-charged-symbolic",
                "battery-level-100-charging-symbolic",
                "battery-full-charged-symbolic",
                "battery-symbolic",
            ]),
            90 => resolve(&["battery-level-90-charging-symbolic", "battery-symbolic"]),
            80 => resolve(&["battery-level-80-charging-symbolic", "battery-symbolic"]),
            70 => resolve(&["battery-level-70-charging-symbolic", "battery-symbolic"]),
            60 => resolve(&["battery-level-60-charging-symbolic", "battery-symbolic"]),
            50 => resolve(&["battery-level-50-charging-symbolic", "battery-symbolic"]),
            40 => resolve(&["battery-level-40-charging-symbolic", "battery-symbolic"]),
            30 => resolve(&["battery-level-30-charging-symbolic", "battery-symbolic"]),
            20 => resolve(&["battery-level-20-charging-symbolic", "battery-symbolic"]),
            10 => resolve(&["battery-level-10-charging-symbolic", "battery-symbolic"]),
            _ => resolve(&["battery-level-0-charging-symbolic", "battery-symbolic"]),
        };
    }

    // Critically low and not charging is the one case worth breaking the
    // level progression for — it should not look like just another step down.
    if percent <= CRITICAL_PERCENT {
        return resolve(&[
            "battery-level-0-symbolic",
            "battery-caution-symbolic",
            "battery-empty-symbolic",
            "battery-symbolic",
        ]);
    }

    match level {
        100 => resolve(&[
            "battery-level-100-symbolic",
            "battery-full-symbolic",
            "battery-symbolic",
        ]),
        90 => resolve(&[
            "battery-level-90-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        80 => resolve(&[
            "battery-level-80-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        70 => resolve(&[
            "battery-level-70-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        60 => resolve(&[
            "battery-level-60-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        50 => resolve(&[
            "battery-level-50-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        40 => resolve(&[
            "battery-level-40-symbolic",
            "battery-low-symbolic",
            "battery-symbolic",
        ]),
        30 => resolve(&[
            "battery-level-30-symbolic",
            "battery-low-symbolic",
            "battery-symbolic",
        ]),
        20 => resolve(&[
            "battery-level-20-symbolic",
            "battery-low-symbolic",
            "battery-symbolic",
        ]),
        _ => resolve(&[
            "battery-level-10-symbolic",
            "battery-caution-symbolic",
            "battery-symbolic",
        ]),
    }
}

/// At or below this, and not charging, the battery icon shows caution.
const CRITICAL_PERCENT: f64 = 10.0;

/// Round to the nearest ten, which is how the `battery-level-N` family steps.
fn bucket(percent: f64) -> u32 {
    let clamped = percent.clamp(0.0, 100.0);
    ((clamped / 10.0).round() as u32) * 10
}

/// Bluetooth, by adapter power and whether anything is connected.
pub fn bluetooth(powered: bool, connected: usize) -> &'static str {
    if !powered {
        return resolve(&[
            "bluetooth-disabled-symbolic",
            "bluetooth-inactive-symbolic",
            "bluetooth-symbolic",
        ]);
    }
    if connected > 0 {
        return resolve(&[
            "bluetooth-active-symbolic",
            "bluetooth-paired-symbolic",
            "bluetooth-symbolic",
        ]);
    }
    resolve(&["bluetooth-symbolic", "bluetooth-active-symbolic"])
}

/// The Wi-Fi tile icon: airplane, hardware-blocked, off, disconnected, or the
/// signal strength of what is connected.
pub fn wifi(
    airplane: bool,
    hardware_killed: bool,
    enabled: bool,
    connected: bool,
    strength: u8,
) -> &'static str {
    if airplane {
        return resolve(&[
            "airplane-mode-symbolic",
            "network-wireless-disabled-symbolic",
            "network-wireless-offline-symbolic",
        ]);
    }
    if hardware_killed {
        return resolve(&[
            "network-wireless-hardware-disabled-symbolic",
            "network-wireless-disabled-symbolic",
            "network-wireless-offline-symbolic",
        ]);
    }
    if !enabled {
        return resolve(&[
            "network-wireless-disabled-symbolic",
            "network-wireless-offline-symbolic",
            "network-wireless-symbolic",
        ]);
    }
    if !connected {
        // Radio on, associated with nothing. Distinct from "off": one is a
        // choice, the other is a problem to fix.
        return resolve(&[
            "network-wireless-offline-symbolic",
            "network-wireless-no-route-symbolic",
            "network-wireless-signal-none-symbolic",
            "network-wireless-symbolic",
        ]);
    }
    signal(strength, false)
}

/// Signal-strength icon for a network in a list.
///
/// `secure` picks the padlocked variant where the theme has one, which is the
/// only visual cue that joining will need a password.
pub fn signal(strength: u8, secure: bool) -> &'static str {
    match (band(strength), secure) {
        (Band::Excellent, true) => resolve(&[
            "network-wireless-signal-excellent-secure-symbolic",
            "network-wireless-signal-excellent-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Good, true) => resolve(&[
            "network-wireless-signal-good-secure-symbolic",
            "network-wireless-signal-good-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Ok, true) => resolve(&[
            "network-wireless-signal-ok-secure-symbolic",
            "network-wireless-signal-ok-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Weak, true) => resolve(&[
            "network-wireless-signal-weak-secure-symbolic",
            "network-wireless-signal-weak-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Excellent, false) => resolve(&[
            "network-wireless-signal-excellent-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Good, false) => resolve(&[
            "network-wireless-signal-good-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Ok, false) => resolve(&[
            "network-wireless-signal-ok-symbolic",
            "network-wireless-symbolic",
        ]),
        (Band::Weak, false) => resolve(&[
            "network-wireless-signal-weak-symbolic",
            "network-wireless-signal-none-symbolic",
            "network-wireless-symbolic",
        ]),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Band {
    Weak,
    Ok,
    Good,
    Excellent,
}

/// The thresholds NetworkManager's own tooling uses for its four bars.
fn band(strength: u8) -> Band {
    match strength {
        80..=u8::MAX => Band::Excellent,
        55..80 => Band::Good,
        30..55 => Band::Ok,
        _ => Band::Weak,
    }
}

/// Window tiling, by whether the current workspace is tiled or floating.
pub fn tiling(tiled: bool) -> &'static str {
    if tiled {
        resolve(&[
            "view-grid-symbolic",
            "view-app-grid-symbolic",
            "view-dual-symbolic",
        ])
    } else {
        resolve(&[
            "view-restore-symbolic",
            "focus-windows-symbolic",
            "window-restore-symbolic",
            "view-grid-symbolic",
        ])
    }
}

/// Volume, by level and mute.
pub fn volume(percent: f64, muted: bool) -> &'static str {
    if muted || percent <= 0.0 {
        return resolve(&[
            "audio-volume-muted-symbolic",
            "audio-volume-low-symbolic",
            "audio-volume-high-symbolic",
        ]);
    }
    match percent {
        p if p >= 66.0 => resolve(&["audio-volume-high-symbolic"]),
        p if p >= 33.0 => resolve(&["audio-volume-medium-symbolic", "audio-volume-high-symbolic"]),
        _ => resolve(&["audio-volume-low-symbolic", "audio-volume-high-symbolic"]),
    }
}

/// Brightness, by level and the dimmed toggle.
pub fn brightness(percent: f64, dimmed: bool) -> &'static str {
    if dimmed || percent <= 1.0 {
        return resolve(&[
            "display-brightness-low-symbolic",
            "display-brightness-off-symbolic",
            "display-brightness-symbolic",
        ]);
    }
    if percent >= 66.0 {
        return resolve(&[
            "display-brightness-high-symbolic",
            "display-brightness-symbolic",
        ]);
    }
    resolve(&[
        "display-brightness-medium-symbolic",
        "display-brightness-symbolic",
    ])
}

/// Dark mode, showing what is currently active.
pub fn dark_mode(dark: bool) -> &'static str {
    if dark {
        resolve(&[
            "weather-clear-night-symbolic",
            "display-brightness-low-symbolic",
        ])
    } else {
        resolve(&["weather-clear-symbolic", "display-brightness-high-symbolic"])
    }
}

/// The panel button.
pub fn applet() -> &'static str {
    resolve(&[
        "preferences-system-symbolic",
        "emblem-system-symbolic",
        "applications-system-symbolic",
    ])
}

/// Back, on a drill-down header. Mirrored automatically for RTL by the theme.
pub fn back() -> &'static str {
    resolve(&["go-previous-symbolic", "pan-start-symbolic"])
}

/// DNS.
pub fn dns() -> &'static str {
    resolve(&[
        "network-server-symbolic",
        "network-workgroup-symbolic",
        "network-wired-symbolic",
    ])
}

/// Airplane mode.
pub fn airplane() -> &'static str {
    resolve(&[
        "airplane-mode-symbolic",
        "airplane-mode-disabled-symbolic",
        "network-wireless-disabled-symbolic",
    ])
}

/// Feral GameMode.
pub fn game_mode() -> &'static str {
    resolve(&[
        "applications-games-symbolic",
        "input-gaming-symbolic",
        "preferences-system-symbolic",
    ])
}

/// A power-profiles-daemon profile.
///
/// The `power-profile-*` names came in with power-profiles-daemon. The COSMIC
/// icon theme does not ship them — it inherits only Pop and hicolor, and neither
/// has them — so on a stock COSMIC install these all fall through. The fallback
/// is deliberately a low/medium/high battery-level progression rather than three
/// identical icons: it carries the same ordering the profiles have. The stock
/// battery applet has the same gap and solves it by not using icons at all.
pub fn power_profile(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::PowerSaver => resolve(&[
            "power-profile-power-saver-symbolic",
            "battery-level-30-symbolic",
            "battery-symbolic",
        ]),
        PowerProfile::Balanced => resolve(&[
            "power-profile-balanced-symbolic",
            "battery-level-70-symbolic",
            "battery-symbolic",
        ]),
        PowerProfile::Performance => resolve(&[
            "power-profile-performance-symbolic",
            "battery-level-100-symbolic",
            "battery-symbolic",
        ]),
    }
}

/// Mirrors `modules::battery::Profile`, kept separate so this module does not
/// depend on a backend module just to name an icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

#[cfg(test)]
mod tests {
    use super::*;

    // These assert the *selection logic*, not that a particular theme ships a
    // particular file — that is what `resolve` handles at runtime, and asserting
    // it here would make the suite depend on the machine's installed themes.

    #[test]
    fn battery_levels_round_to_the_nearest_ten() {
        assert_eq!(bucket(0.0), 0);
        assert_eq!(bucket(4.0), 0);
        assert_eq!(bucket(5.0), 10);
        assert_eq!(bucket(47.0), 50);
        assert_eq!(bucket(94.0), 90);
        assert_eq!(bucket(100.0), 100);
    }

    #[test]
    fn battery_buckets_never_leave_the_family() {
        // A value outside 0-100 from a backend must not produce a name like
        // `battery-level-130-symbolic`, which no theme has.
        assert_eq!(bucket(-20.0), 0);
        assert_eq!(bucket(250.0), 100);
    }

    #[test]
    fn signal_bands_are_ordered_and_total() {
        assert_eq!(band(0), Band::Weak);
        assert_eq!(band(29), Band::Weak);
        assert_eq!(band(30), Band::Ok);
        assert_eq!(band(54), Band::Ok);
        assert_eq!(band(55), Band::Good);
        assert_eq!(band(79), Band::Good);
        assert_eq!(band(80), Band::Excellent);
        assert_eq!(band(100), Band::Excellent);
        // No panic at the top of the range.
        assert_eq!(band(u8::MAX), Band::Excellent);
    }

    #[test]
    fn an_empty_candidate_list_yields_the_spec_fallback() {
        assert_eq!(resolve(&[]), FALLBACK);
    }
}
