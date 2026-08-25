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
/// Uses COSMIC's own `cosmic-applet-battery-level-N` family, which is what the
/// stock battery applet draws — so the tile matches the battery already on the
/// panel instead of sitting next to it in a different style. The two families
/// are not interchangeable: the generic freedesktop `battery-level-N` icons are
/// a portrait battery in most themes, COSMIC's are landscape, and mixing them
/// is exactly the mismatch this fixes.
///
/// The generic names follow as fallbacks, because a user on GNOME or KDE with
/// no COSMIC icon theme installed must still get a battery rather than a blank
/// square. Older themes ship only the coarse
/// `battery-{caution,low,good,full}` set, which is why those come last.
pub fn battery(percent: Option<f64>, charging: bool) -> &'static str {
    let Some(percent) = percent else {
        // No battery: a desktop showing power profiles only.
        return resolve(&["battery-missing-symbolic", "battery-symbolic"]);
    };

    let level = bucket(percent);

    if charging {
        return match level {
            100 => resolve(&[
                "cosmic-applet-battery-level-100-charging-symbolic",
                "cosmic-applet-battery-level-100-symbolic",
                "battery-level-100-charging-symbolic",
                "battery-level-100-charged-symbolic",
                "battery-full-charged-symbolic",
                "battery-symbolic",
            ]),
            90 => resolve(&[
                "cosmic-applet-battery-level-90-charging-symbolic",
                "battery-level-90-charging-symbolic",
                "battery-symbolic",
            ]),
            80 => resolve(&[
                "cosmic-applet-battery-level-80-charging-symbolic",
                "battery-level-80-charging-symbolic",
                "battery-symbolic",
            ]),
            65 => resolve(&[
                "cosmic-applet-battery-level-65-charging-symbolic",
                "battery-level-60-charging-symbolic",
                "battery-symbolic",
            ]),
            50 => resolve(&[
                "cosmic-applet-battery-level-50-charging-symbolic",
                "battery-level-50-charging-symbolic",
                "battery-symbolic",
            ]),
            35 => resolve(&[
                "cosmic-applet-battery-level-35-charging-symbolic",
                "battery-level-30-charging-symbolic",
                "battery-symbolic",
            ]),
            20 => resolve(&[
                "cosmic-applet-battery-level-20-charging-symbolic",
                "battery-level-20-charging-symbolic",
                "battery-symbolic",
            ]),
            10 => resolve(&[
                "cosmic-applet-battery-level-10-charging-symbolic",
                "battery-level-10-charging-symbolic",
                "battery-symbolic",
            ]),
            5 => resolve(&[
                "cosmic-applet-battery-level-5-charging-symbolic",
                "battery-level-10-charging-symbolic",
                "battery-symbolic",
            ]),
            _ => resolve(&[
                "cosmic-applet-battery-level-0-charging-symbolic",
                "battery-level-0-charging-symbolic",
                "battery-symbolic",
            ]),
        };
    }

    // Critically low and not charging is the one case worth breaking the
    // level progression for — it should not look like just another step down.
    if percent <= CRITICAL_PERCENT {
        return resolve(&[
            "cosmic-applet-battery-level-0-symbolic",
            "battery-level-0-symbolic",
            "battery-caution-symbolic",
            "battery-empty-symbolic",
            "battery-symbolic",
        ]);
    }

    match level {
        100 => resolve(&[
            "cosmic-applet-battery-level-100-symbolic",
            "battery-level-100-symbolic",
            "battery-full-symbolic",
            "battery-symbolic",
        ]),
        90 => resolve(&[
            "cosmic-applet-battery-level-90-symbolic",
            "battery-level-90-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        80 => resolve(&[
            "cosmic-applet-battery-level-80-symbolic",
            "battery-level-80-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        65 => resolve(&[
            "cosmic-applet-battery-level-65-symbolic",
            "battery-level-60-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        50 => resolve(&[
            "cosmic-applet-battery-level-50-symbolic",
            "battery-level-50-symbolic",
            "battery-good-symbolic",
            "battery-symbolic",
        ]),
        35 => resolve(&[
            "cosmic-applet-battery-level-35-symbolic",
            "battery-level-30-symbolic",
            "battery-low-symbolic",
            "battery-symbolic",
        ]),
        20 => resolve(&[
            "cosmic-applet-battery-level-20-symbolic",
            "battery-level-20-symbolic",
            "battery-low-symbolic",
            "battery-symbolic",
        ]),
        10 => resolve(&[
            "cosmic-applet-battery-level-10-symbolic",
            "battery-level-10-symbolic",
            "battery-caution-symbolic",
            "battery-symbolic",
        ]),
        5 => resolve(&[
            "cosmic-applet-battery-level-5-symbolic",
            "battery-level-10-symbolic",
            "battery-caution-symbolic",
            "battery-symbolic",
        ]),
        _ => resolve(&[
            "cosmic-applet-battery-level-0-symbolic",
            "battery-level-0-symbolic",
            "battery-caution-symbolic",
            "battery-symbolic",
        ]),
    }
}

/// At or below this, and not charging, the battery icon shows caution.
const CRITICAL_PERCENT: f64 = 10.0;

/// The charge levels COSMIC's battery icons are drawn at.
///
/// Not a round decade series — COSMIC ships 35 and 65 but no 30, 40, 60 or 70,
/// so rounding to the nearest ten produces names that do not exist.
const COSMIC_LEVELS: [u32; 10] = [0, 5, 10, 20, 35, 50, 65, 80, 90, 100];

/// The largest level at or below `percent`.
///
/// Rounds **down**, not to nearest. A battery at 19% must not draw the icon for
/// 20: overstating a nearly-flat battery is the one error here with a real
/// consequence, and the cost of understating is that a full battery reads as
/// full only at exactly 100%.
fn bucket(percent: f64) -> u32 {
    let clamped = percent.clamp(0.0, 100.0);
    COSMIC_LEVELS
        .into_iter()
        .rev()
        .find(|level| f64::from(*level) <= clamped)
        .unwrap_or(0)
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
    // COSMIC's own tiling applet ships a matched pair, `.On` and `.Off`, and
    // using them means the tile reads the same as the tiling button already on
    // the panel. They are plain app icons rather than the `-symbolic` family,
    // which is why the names look unlike everything else here.
    if tiled {
        resolve(&[
            "com.system76.CosmicAppletTiling.On",
            "com.system76.CosmicAppletTiling-symbolic",
            "view-grid-symbolic",
            "view-app-grid-symbolic",
            "view-dual-symbolic",
        ])
    } else {
        resolve(&[
            "com.system76.CosmicAppletTiling.Off",
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

/// Prefix for the glyphs this project ships, installed into hicolor so they
/// resolve under any icon theme.
const PRESET_PREFIX: &str = "cosmic-control-center-";

/// The presets offered in the Settings window, in display order.
pub const PRESETS: [(&str, &str); 4] = [
    ("sliders", "preset-sliders"),
    ("toggles", "preset-toggles"),
    ("dials", "preset-dials"),
    ("grid", "preset-grid"),
];

/// Icon-theme name for a preset.
pub fn preset_name(preset: &str) -> String {
    format!("{PRESET_PREFIX}{preset}-symbolic")
}

/// Build the panel button's icon from the user's choice.
///
/// Returns a handle rather than a name because a custom choice may be a path on
/// disk, which has no name to look up.
pub fn panel_handle(choice: &crate::config::PanelIcon, size: u16) -> cosmic::widget::icon::Handle {
    use crate::config::PanelIcon;

    match choice {
        PanelIcon::System => named_handle(applet(), size),
        PanelIcon::Preset(preset) => {
            let name = preset_name(preset);
            // A preset that will not resolve means the icons were not
            // installed. Falling back to the system icon keeps a button on the
            // panel rather than leaving a blank gap the user cannot click.
            if cosmic::widget::icon::from_name(name.clone())
                .path()
                .is_some()
            {
                cosmic::widget::icon::from_name(name)
                    .symbolic(true)
                    .size(size)
                    .handle()
            } else {
                tracing::warn!("preset icon `{name}` is not installed; using the system icon");
                named_handle(applet(), size)
            }
        }
        PanelIcon::Custom(value) => custom_handle(value, size),
    }
}

/// A user-supplied icon: an absolute path, or an icon-theme name.
///
/// Both are accepted because both are reasonable things to type, and telling
/// them apart is unambiguous — a path starts with `/`.
fn custom_handle(value: &str, size: u16) -> cosmic::widget::icon::Handle {
    let trimmed = value.trim();

    if trimmed.starts_with('/') {
        let path = std::path::Path::new(trimmed);
        if path.is_file() {
            return cosmic::widget::icon::from_path(path.to_path_buf());
        }
        tracing::warn!("custom icon `{trimmed}` does not exist; using the system icon");
        return named_handle(applet(), size);
    }

    if !trimmed.is_empty()
        && cosmic::widget::icon::from_name(trimmed.to_string())
            .path()
            .is_some()
    {
        return cosmic::widget::icon::from_name(trimmed.to_string())
            .symbolic(true)
            .size(size)
            .handle();
    }

    tracing::warn!("custom icon `{trimmed}` did not resolve; using the system icon");
    named_handle(applet(), size)
}

fn named_handle(name: &'static str, size: u16) -> cosmic::widget::icon::Handle {
    cosmic::widget::icon::from_name(name)
        .symbolic(true)
        .size(size)
        .handle()
}

/// VPN, by whether a tunnel is up.
pub fn vpn(connected: bool) -> &'static str {
    if connected {
        resolve(&[
            "network-vpn-symbolic",
            "network-vpn-acquiring-symbolic",
            "channel-secure-symbolic",
            "network-wired-symbolic",
        ])
    } else {
        resolve(&[
            "network-vpn-disconnected-symbolic",
            "network-vpn-disabled-symbolic",
            "network-vpn-symbolic",
            "network-wired-symbolic",
        ])
    }
}

/// Keyboard backlight, by whether it is lit.
pub fn keyboard(lit: bool) -> &'static str {
    if lit {
        resolve(&[
            "keyboard-brightness-symbolic",
            "input-keyboard-symbolic",
            "display-brightness-high-symbolic",
        ])
    } else {
        resolve(&[
            "keyboard-brightness-off-symbolic",
            "keyboard-brightness-symbolic",
            "input-keyboard-symbolic",
            "display-brightness-low-symbolic",
        ])
    }
}

/// Do Not Disturb.
pub fn do_not_disturb(on: bool) -> &'static str {
    if on {
        resolve(&[
            "notification-disabled-symbolic",
            "preferences-system-notifications-symbolic",
            "dialog-information-symbolic",
        ])
    } else {
        resolve(&[
            "notification-symbolic",
            "preferences-system-notifications-symbolic",
            "dialog-information-symbolic",
        ])
    }
}

/// Keep awake.
pub fn keep_awake(on: bool) -> &'static str {
    if on {
        resolve(&[
            "my-caffeine-on-symbolic",
            "preferences-desktop-screensaver-symbolic",
            "weather-clear-symbolic",
        ])
    } else {
        resolve(&[
            "my-caffeine-off-symbolic",
            "preferences-desktop-screensaver-symbolic",
            "weather-clear-night-symbolic",
        ])
    }
}

/// Microphone, by level and mute.
pub fn microphone(percent: f64, muted: bool) -> &'static str {
    if muted || percent <= 0.0 {
        return resolve(&[
            "microphone-sensitivity-muted-symbolic",
            "audio-input-microphone-muted-symbolic",
            "audio-input-microphone-symbolic",
        ]);
    }
    match percent {
        p if p >= 66.0 => resolve(&[
            "microphone-sensitivity-high-symbolic",
            "audio-input-microphone-symbolic",
        ]),
        p if p >= 33.0 => resolve(&[
            "microphone-sensitivity-medium-symbolic",
            "audio-input-microphone-symbolic",
        ]),
        _ => resolve(&[
            "microphone-sensitivity-low-symbolic",
            "audio-input-microphone-symbolic",
        ]),
    }
}

/// Transport controls.
pub fn media_play_pause(playing: bool) -> &'static str {
    if playing {
        resolve(&["media-playback-pause-symbolic"])
    } else {
        resolve(&["media-playback-start-symbolic"])
    }
}

pub fn media_next() -> &'static str {
    resolve(&["media-skip-forward-symbolic", "go-next-symbolic"])
}

pub fn media_previous() -> &'static str {
    resolve(&["media-skip-backward-symbolic", "go-previous-symbolic"])
}

/// Resolve a name that is not known at compile time.
///
/// Used for user-defined tiles, whose icon comes from the config file. Returns
/// an owned string because there is no `'static` name to hand back: the caller
/// keeps it alive for the widget.
///
/// Falls back to a generic executable glyph rather than rendering blank, so a
/// typo in a config file still produces a pressable tile.
pub fn resolve_owned(name: &str) -> String {
    if !name.is_empty()
        && cosmic::widget::icon::from_name(name.to_string())
            .path()
            .is_some()
    {
        return name.to_string();
    }
    tracing::debug!("custom tile icon `{name}` did not resolve");
    resolve(&[
        "application-x-executable-symbolic",
        "system-run-symbolic",
        "preferences-system-symbolic",
    ])
    .to_string()
}

/// The icon for a media player, from whatever it tells us about itself.
///
/// Tried in order of how well each identifies the app. `DesktopEntry` is the
/// property MPRIS provides for this and is the only one guaranteed to name an
/// installed `.desktop` file — but it is optional, and Chromium (among others)
/// does not publish it, so the bus name and `Identity` follow as guesses that
/// happen to be right for most players. Anything unrecognised gets a generic
/// media glyph rather than a blank space.
pub fn media_player(desktop_entry: Option<&str>, bus_suffix: &str, identity: &str) -> String {
    let candidates = [
        desktop_entry.unwrap_or_default().to_string(),
        bus_suffix.to_ascii_lowercase(),
        identity.to_ascii_lowercase(),
        // Spaces are not legal in icon names and some players have them:
        // "Firefox Nightly" is shipped as `firefox-nightly`.
        identity.to_ascii_lowercase().replace(' ', "-"),
    ];

    for candidate in candidates {
        if candidate.is_empty() {
            continue;
        }
        if cosmic::widget::icon::from_name(candidate.clone())
            .path()
            .is_some()
        {
            return candidate;
        }
    }

    resolve(&[
        "multimedia-player-symbolic",
        "audio-x-generic-symbolic",
        "media-playback-start-symbolic",
    ])
    .to_string()
}

/// The default panel button.
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
    fn battery_levels_step_down_through_cosmics_scale() {
        // Not a decade scale, and not nearest-match: see `bucket`.
        assert_eq!(bucket(0.0), 0);
        assert_eq!(bucket(4.0), 0);
        assert_eq!(bucket(5.0), 5);
        assert_eq!(bucket(9.0), 5);
        assert_eq!(bucket(47.0), 35);
        assert_eq!(bucket(65.0), 65);
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
    fn battery_buckets_are_only_levels_cosmic_actually_ships() {
        // COSMIC ships 35 and 65 but no 30, 40, 60 or 70. Rounding to the
        // nearest ten — which is what this did before — names files that do not
        // exist, and the icon silently falls back to the generic family, which
        // is the portrait battery this change exists to stop drawing.
        for percent in 0u32..=100 {
            let level = bucket(f64::from(percent));
            assert!(
                COSMIC_LEVELS.contains(&level),
                "{percent}% produced level {level}, which COSMIC does not ship"
            );
        }
    }

    #[test]
    fn a_battery_icon_never_overstates_the_charge() {
        // Rounding down is the whole point: 19% must not draw the 20% icon.
        // Understating is harmless; telling someone their nearly-flat battery
        // has more in it than it does is not.
        for percent in 0u32..=100 {
            let level = bucket(f64::from(percent));
            assert!(percent >= level, "{percent}% drew the icon for {level}%");
        }
        assert_eq!(bucket(19.0), 10);
        assert_eq!(bucket(20.0), 20);
        assert_eq!(bucket(99.9), 90);
        assert_eq!(bucket(100.0), 100);
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
