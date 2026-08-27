//! User configuration, read from `config.toml` and written by the Settings
//! window.
//!
//! Two things live here. The `[modules]` table decides which controls exist at
//! all — a module switched off is never constructed and never opens a bus
//! connection, which is what lets a desktop user hide the battery tile without
//! leaving a dead D-Bus client behind. The `[appearance]` table decides how what
//! remains is drawn.
//!
//! The file stays the source of truth rather than moving to `cosmic-config`,
//! because it is meant to be hand-editable: it is documented, commented, and
//! shipped as `config.example.toml`. Writing it back therefore has to preserve
//! that — see [`Config::save`].

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub modules: Modules,
    /// Badges beside the panel button. See [`Indicators`].
    #[serde(default)]
    pub indicators: Indicators,
    pub appearance: Appearance,
    pub dns: Dns,
    /// User-defined tiles that run a command. See [`crate::modules::custom`].
    #[serde(default)]
    pub custom: Vec<crate::modules::custom::Tile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    /// The Wi-Fi + Bluetooth + VPN grouped tile.
    ///
    /// Independent of the three flags below: the group and the standalone
    /// tiles are separate choices, so you can have the group, the individual
    /// tiles, both, or neither. Rows inside the group appear whenever the
    /// hardware is there — `wifi = false` hides the standalone Wi-Fi tile,
    /// it does not empty the group's Wi-Fi row.
    pub connectivity: bool,
    pub wifi: bool,
    pub bluetooth: bool,
    pub battery: bool,
    pub dns: bool,
    pub volume: bool,
    pub brightness: bool,
    pub dark_mode: bool,
    pub tiling: bool,
    pub gamemode: bool,
    pub microphone: bool,
    pub keyboard_backlight: bool,
    pub do_not_disturb: bool,
    pub keep_awake: bool,
    pub media: bool,
    pub vpn: bool,
    pub charge_threshold: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub style: TileStyle,
    /// How tile surfaces are painted. See [`TileFinish`].
    #[serde(default)]
    pub finish: TileFinish,
    pub icon: PanelIcon,
    /// The placed tiles: what is drawn, at what size, and where.
    ///
    /// `[[appearance.layout]]` entries. Empty means "not migrated yet" —
    /// [`Config::load`] synthesises it from `order`/`shapes` (or the defaults)
    /// through [`crate::tile_layout::migrate_from_packed`], and
    /// [`crate::tile_layout::validate`] drops overlaps first-wins.
    #[serde(default)]
    pub layout: Vec<crate::tile_layout::Instance>,
    /// **Pre-0.2, migration only.** The order tiles were drawn in. Read once
    /// to build `layout`, then dropped from the file on the next save.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<crate::tile_layout::TileKey>,
    /// **Pre-0.2, migration only.** Per-tile shape overrides. Same lifecycle
    /// as `order`.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub shapes:
        std::collections::HashMap<crate::tile_layout::TileKey, crate::tile_layout::TileShape>,
}

/// How strongly a tile signals that its control is on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TileStyle {
    /// The whole tile fills with the accent colour when on.
    #[default]
    High,
    /// The tile stays neutral; a base shape behind the icon takes the accent.
    Medium,
    /// Tiles never show an on state. Selection appears only inside a
    /// drill-down page — the way the Battery tile already behaves, where you
    /// see which power profile is active only after opening it.
    Low,
}

impl TileStyle {
    pub const ALL: [TileStyle; 3] = [TileStyle::High, TileStyle::Medium, TileStyle::Low];

    pub fn l10n_key(self) -> &'static str {
        match self {
            TileStyle::High => "style-high",
            TileStyle::Medium => "style-medium",
            TileStyle::Low => "style-low",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            TileStyle::High => "style-high-detail",
            TileStyle::Medium => "style-medium-detail",
            TileStyle::Low => "style-low-detail",
        }
    }
}

/// How a tile's own surface is painted.
///
/// Separate from [`TileStyle`], which is about the *on* state. This is about
/// the material: how much of the popup's frosted glass comes through the tile
/// sitting on it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TileFinish {
    /// A filled card. Frost-aware — it thins when the desktop's frosted
    /// styling is on — but still reads as a solid surface on the glass.
    Solid,
    /// A frosted card: the blur behind comes through, but the tile is clearly
    /// denser than the popup around it, so it still reads as a distinct
    /// surface rather than a smudge.
    ///
    /// The default. With the popup's blur working this is the look the
    /// desktop's own frosted styling is asking for; `Solid` keeps the tiles
    /// opaque for anyone who would rather read them than see through them.
    #[default]
    Frosted,
    /// No fill at all — a semi-transparent stroke, and one uninterrupted
    /// sheet of frosted glass across the whole popup.
    Outline,
}

impl TileFinish {
    pub const ALL: [TileFinish; 3] = [TileFinish::Solid, TileFinish::Frosted, TileFinish::Outline];

    pub fn l10n_key(self) -> &'static str {
        match self {
            TileFinish::Solid => "finish-solid",
            TileFinish::Frosted => "finish-frosted",
            TileFinish::Outline => "finish-outline",
        }
    }

    pub fn description_key(self) -> &'static str {
        match self {
            TileFinish::Solid => "finish-solid-detail",
            TileFinish::Frosted => "finish-frosted-detail",
            TileFinish::Outline => "finish-outline-detail",
        }
    }
}

/// Which side of the panel button a badge sits on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BadgeSide {
    Left,
    #[default]
    Right,
}

impl BadgeSide {
    pub const ALL: [BadgeSide; 2] = [BadgeSide::Left, BadgeSide::Right];

    pub fn l10n_key(self) -> &'static str {
        match self {
            BadgeSide::Left => "badge-side-left",
            BadgeSide::Right => "badge-side-right",
        }
    }
}

/// How long a badge that announces an *event* stays up.
///
/// A disconnection is a moment, not a state you want shouted at forever —
/// after a while you know, and a permanent badge is just noise. The battery
/// badge has no equivalent, because a flat battery does not stop being true
/// while you are not looking at it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BadgeTimeout {
    #[serde(rename = "30s")]
    HalfMinute,
    #[default]
    #[serde(rename = "1m")]
    Minute,
    #[serde(rename = "5m")]
    FiveMinutes,
    /// Until it is fixed.
    Never,
}

impl BadgeTimeout {
    pub const ALL: [BadgeTimeout; 4] = [
        BadgeTimeout::HalfMinute,
        BadgeTimeout::Minute,
        BadgeTimeout::FiveMinutes,
        BadgeTimeout::Never,
    ];

    /// How long the badge lasts, or `None` for "until it is fixed".
    pub fn duration(self) -> Option<std::time::Duration> {
        match self {
            BadgeTimeout::HalfMinute => Some(std::time::Duration::from_secs(30)),
            BadgeTimeout::Minute => Some(std::time::Duration::from_secs(60)),
            BadgeTimeout::FiveMinutes => Some(std::time::Duration::from_secs(300)),
            BadgeTimeout::Never => None,
        }
    }

    pub fn l10n_key(self) -> &'static str {
        match self {
            BadgeTimeout::HalfMinute => "badge-timeout-30s",
            BadgeTimeout::Minute => "badge-timeout-1m",
            BadgeTimeout::FiveMinutes => "badge-timeout-5m",
            BadgeTimeout::Never => "badge-timeout-never",
        }
    }
}

/// Small icons beside the panel button, for the things worth knowing without
/// opening the popup.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Indicators {
    /// A red battery when the charge is low and nothing is charging it.
    pub battery_low: bool,
    /// What counts as low, in percent.
    pub battery_low_percent: u8,
    /// A badge when Wi-Fi drops.
    pub wifi_disconnected: bool,
    /// How long that badge stays up. See [`BadgeTimeout`].
    pub wifi_timeout: BadgeTimeout,
    /// Which side of the panel button the badges sit on.
    pub side: BadgeSide,
}

impl Default for Indicators {
    fn default() -> Self {
        Self {
            // Off by default: an applet that puts a second icon on someone's
            // panel uninvited is taking a decision that is theirs to make.
            battery_low: false,
            // Where COSMIC's own battery applet starts warning.
            battery_low_percent: 20,
            wifi_disconnected: false,
            wifi_timeout: BadgeTimeout::default(),
            side: BadgeSide::default(),
        }
    }
}

impl Indicators {
    /// Whether the battery badge should be up.
    ///
    /// Charging is never low, however few percent it is at: the number is
    /// going up, and a red badge over a charging battery is the applet crying
    /// wolf. Pure, so the rule can be tested without a battery.
    pub fn battery_badge(&self, shown: bool, charging: bool, percent: Option<f64>) -> bool {
        self.battery_low
            && shown
            && !charging
            && percent.is_some_and(|percent| percent <= f64::from(self.battery_low_percent))
    }

    /// Whether the Wi-Fi badge should be up, given how long ago it dropped.
    ///
    /// `None` means Wi-Fi is up, so there is nothing to say. A `Never` timeout
    /// keeps the badge until it reconnects; the rest lapse, because a
    /// disconnection is news for a while and clutter after that.
    pub fn wifi_badge(&self, since_lost: Option<std::time::Duration>) -> bool {
        let Some(elapsed) = since_lost else {
            return false;
        };
        self.wifi_disconnected
            && match self.wifi_timeout.duration() {
                Some(limit) => elapsed < limit,
                None => true,
            }
    }
}

/// The preset shown on a fresh install.
///
/// A shipped glyph rather than the system default, because the system default
/// is `preferences-system-symbolic` — the same cog several other things use, so
/// a new install puts an unrecognisable button on the panel. This one is drawn
/// for this applet and reads as "controls" at panel size.
pub const DEFAULT_PRESET: &str = "toggles";

/// What the panel button shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum PanelIcon {
    /// Whatever the icon theme gives for the default name.
    System,
    /// One of the glyphs shipped in `data/icons`.
    Preset(String),
    /// An icon-theme name or an absolute path to an image the user supplied.
    Custom(String),
}

impl Default for PanelIcon {
    fn default() -> Self {
        PanelIcon::Preset(DEFAULT_PRESET.to_string())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dns {
    /// Extra providers appended to the built-in list, as `["Name", "1.1.1.1", "1.0.0.1"]`.
    pub custom_providers: Vec<Vec<String>>,
}

impl Default for Modules {
    fn default() -> Self {
        // Everything on by default: a fresh install should show the full applet,
        // and each module hides itself anyway when its hardware or daemon is
        // absent. Opting out here is for people who have working hardware but
        // still don't want the tile.
        Self {
            // The group on, the three standalone tiles off: showing both by
            // default would put Wi-Fi on the grid twice.
            connectivity: true,
            wifi: false,
            bluetooth: false,
            battery: true,
            dns: true,
            volume: true,
            brightness: true,
            dark_mode: true,
            tiling: true,
            gamemode: true,
            microphone: true,
            keyboard_backlight: true,
            do_not_disturb: true,
            keep_awake: true,
            media: true,
            vpn: false,
            charge_threshold: true,
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        Some(
            dirs::config_dir()?
                .join("cosmic-control-center-applet")
                .join("config.toml"),
        )
    }

    /// Load the config, falling back to defaults.
    ///
    /// A missing file is normal and silent. A *malformed* file is not: we warn
    /// loudly and carry on with defaults rather than refusing to start, because
    /// an applet that fails to launch gives the user no way to see the error —
    /// the panel simply shows nothing.
    pub fn load() -> Self {
        let Some(path) = Self::path() else {
            return Self::default();
        };

        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            // First run. The shipped launchers are offered here and nowhere
            // else: injecting them whenever `custom` happens to be empty would
            // put them back every time someone deleted the last one.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                let fresh = Self {
                    custom: crate::modules::custom::installed(
                        crate::modules::custom::default_launchers(),
                    ),
                    ..Self::default()
                };
                return fresh.migrated();
            }
            Err(err) => {
                tracing::warn!("could not read {}: {err}", path.display());
                return Self::default();
            }
        };

        let config: Self = match toml::from_str(&raw) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("{} is invalid, using defaults: {err}", path.display());
                Self::default()
            }
        };
        config.migrated()
    }

    /// Whether `[modules]` has this tile switched on.
    ///
    /// Pre-0.2 selection. Kept for the migration and for the non-tile toggles
    /// that survive on the Styling tab.
    pub fn module_enabled(&self, key: crate::tile_layout::TileKey) -> bool {
        use crate::tile_layout::TileKey as K;
        let m = &self.modules;
        match key {
            K::Connectivity => m.connectivity,
            K::Wifi => m.wifi,
            K::Bluetooth => m.bluetooth,
            K::Vpn => m.vpn,
            K::Battery => m.battery,
            K::Dns => m.dns,
            K::DarkMode => m.dark_mode,
            K::Tiling => m.tiling,
            K::GameMode => m.gamemode,
            K::Media => m.media,
            K::DoNotDisturb => m.do_not_disturb,
            K::KeepAwake => m.keep_awake,
            K::ChargeThreshold => m.charge_threshold,
            K::KeyboardBacklight => m.keyboard_backlight,
            K::Volume => m.volume,
            K::Brightness => m.brightness,
            K::Microphone => m.microphone,
        }
    }

    /// Bring a just-parsed config up to the instance-based layout.
    ///
    /// If `layout` is empty — a 0.1.6 file with `order`/`shapes`, or a fresh
    /// install with neither — run the old packer once so the grid opens
    /// exactly as it did, then forget `order`/`shapes` so the next save drops
    /// them. A populated `layout` is only validated: hand edits can overlap.
    pub fn migrated(mut self) -> Self {
        use crate::tile_layout as tl;
        if self.appearance.layout.is_empty() {
            self.appearance.layout =
                tl::migrate_from_packed(&self.appearance.order, &self.appearance.shapes, |k| {
                    self.module_enabled(k)
                });
            if !self.appearance.order.is_empty() || !self.appearance.shapes.is_empty() {
                tracing::info!(
                    "migrated `order`/`shapes` into {} layout instances",
                    self.appearance.layout.len()
                );
            }
        } else {
            self.appearance.layout = tl::validate(&self.appearance.layout);
        }

        // A custom tile is addressed by its index in `[[custom]]`, and that
        // array can be edited by hand. An index it no longer has would draw
        // nothing at all, so the instance goes rather than leaving a hole
        // nobody can fill.
        let defined = self.custom.len();
        self.appearance.layout.retain(|instance| {
            let live = match instance.control {
                tl::Control::Custom { custom } => custom < defined,
                tl::Control::Builtin(_) => true,
            };
            if !live {
                tracing::warn!("dropping {instance:?}: no such custom tile");
            }
            live
        });

        // Custom tiles used to be appended after the grid rather than placed
        // on it. Anything defined but not placed goes on now, at the first
        // free space, so upgrading puts them where they already appeared
        // instead of losing them.
        for index in 0..defined {
            let control = tl::Control::custom(index);
            if !self.custom[index].is_usable()
                || self.appearance.layout.iter().any(|i| i.control == control)
            {
                continue;
            }
            let shape = self.custom[index].shape;
            let (col, row) = tl::first_free(&self.appearance.layout, shape);
            self.appearance
                .layout
                .push(tl::Instance::new(control, shape, col, row));
        }

        self.appearance.order.clear();
        self.appearance.shapes.clear();
        self
    }

    /// Write the config back, with the explanatory header intact.
    ///
    /// The header is re-emitted rather than preserved from the old file: a
    /// round-trip through `toml` drops comments entirely, and silently turning a
    /// documented file into a bare table the first time someone touches the
    /// Settings window would be a poor trade.
    pub fn save(&self) -> Result<(), String> {
        let path = Self::path().ok_or("no config directory")?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
        }

        let body =
            toml::to_string_pretty(self).map_err(|err| format!("could not encode: {err}"))?;
        let contents = format!("{HEADER}\n{body}");

        // Write-then-rename, so an interrupted write cannot leave a truncated
        // file that the applet would then report as invalid on next start.
        let temporary = path.with_extension("toml.tmp");
        std::fs::write(&temporary, contents)
            .map_err(|err| format!("could not write {}: {err}", temporary.display()))?;
        std::fs::rename(&temporary, &path)
            .map_err(|err| format!("could not replace {}: {err}", path.display()))?;
        Ok(())
    }
}

const HEADER: &str = "\
# Control Center configuration.
#
# Written by the Settings window (right-click the panel button), and safe to
# edit by hand. Unknown keys are rejected rather than ignored, so a typo shows
# up as a warning instead of silently doing nothing.
#
# A module set to false is never constructed and never connects to its bus.
# Modules also hide themselves when the hardware or daemon is missing, so a
# desktop does not need to disable `battery` to avoid an empty tile.
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_toml() {
        let original = Config::default();
        let encoded = toml::to_string_pretty(&original).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn every_style_and_icon_choice_round_trips() {
        // `PanelIcon` is an adjacently-tagged enum, which is easy to get wrong
        // in a way that only shows up when someone picks a non-default.
        for icon in [
            PanelIcon::System,
            PanelIcon::Preset("sliders".into()),
            PanelIcon::Custom("/home/someone/icon.svg".into()),
        ] {
            for style in TileStyle::ALL {
                let original = Config {
                    appearance: Appearance {
                        style,
                        icon: icon.clone(),
                        ..Appearance::default()
                    },
                    ..Config::default()
                };
                let encoded = toml::to_string_pretty(&original).unwrap();
                let decoded: Config = toml::from_str(&encoded)
                    .unwrap_or_else(|err| panic!("{style:?}/{icon:?} failed: {err}\n{encoded}"));
                assert_eq!(original, decoded);
            }
        }
    }

    #[test]
    fn a_saved_config_is_still_readable_as_toml() {
        // The header is prepended by hand, so it has to remain valid TOML
        // comments — a stray unescaped character would make the file we just
        // wrote unparseable on next start.
        let config = Config::default();
        let body = toml::to_string_pretty(&config).unwrap();
        let contents = format!("{HEADER}\n{body}");
        let decoded: Config = toml::from_str(&contents).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn the_default_panel_icon_is_a_shipped_glyph_not_the_system_cog() {
        // The system default is `preferences-system-symbolic`, which several
        // other things also use — a fresh install would put an anonymous cog on
        // the panel.
        assert_eq!(
            Config::default().appearance.icon,
            PanelIcon::Preset(DEFAULT_PRESET.to_string())
        );
        // And the preset named here has to be one that actually ships.
        assert!(crate::ui::icons::PRESETS
            .iter()
            .any(|(name, _)| *name == DEFAULT_PRESET));
    }

    #[test]
    fn choosing_the_system_icon_is_still_possible() {
        // It stopped being the default; it must not stop being an option.
        let chosen = Config {
            appearance: Appearance {
                icon: PanelIcon::System,
                ..Appearance::default()
            },
            ..Config::default()
        };
        let encoded = toml::to_string_pretty(&chosen).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.appearance.icon, PanelIcon::System);
    }

    #[test]
    fn a_charging_battery_is_never_low() {
        // However few percent it is at, the number is going up.
        let on = Indicators {
            battery_low: true,
            ..Indicators::default()
        };
        assert!(on.battery_badge(true, false, Some(5.0)));
        assert!(!on.battery_badge(true, true, Some(5.0)));
        // The threshold is inclusive, and a percent above it is not low.
        assert!(on.battery_badge(true, false, Some(20.0)));
        assert!(!on.battery_badge(true, false, Some(21.0)));
        // No battery, or nothing read yet, says nothing.
        assert!(!on.battery_badge(false, false, Some(5.0)));
        assert!(!on.battery_badge(true, false, None));
        // And off is off.
        assert!(!Indicators::default().battery_badge(true, false, Some(5.0)));
    }

    #[test]
    fn the_wifi_badge_lapses_unless_it_is_set_to_never() {
        use std::time::Duration;
        let on = Indicators {
            wifi_disconnected: true,
            wifi_timeout: BadgeTimeout::Minute,
            ..Indicators::default()
        };
        // Wi-Fi up: nothing to say.
        assert!(!on.wifi_badge(None));
        assert!(on.wifi_badge(Some(Duration::from_secs(59))));
        assert!(!on.wifi_badge(Some(Duration::from_secs(61))));

        let forever = Indicators {
            wifi_timeout: BadgeTimeout::Never,
            ..on.clone()
        };
        assert!(forever.wifi_badge(Some(Duration::from_secs(60 * 60))));
        // But still nothing once it is back.
        assert!(!forever.wifi_badge(None));

        // Off is off, however long ago it dropped.
        assert!(!Indicators::default().wifi_badge(Some(Duration::from_secs(1))));
    }

    #[test]
    fn badges_are_off_until_asked_for() {
        // Putting a second icon on someone's panel uninvited is a decision
        // that is theirs, so a fresh config shows none.
        let fresh = Indicators::default();
        assert!(!fresh.battery_low);
        assert!(!fresh.wifi_disconnected);
        // And a config written before badges existed reads the same way.
        let older: Config = toml::from_str("[modules]\nwifi = true\n").unwrap();
        assert_eq!(older.indicators, Indicators::default());
    }

    #[test]
    fn badge_timeouts_round_trip_as_the_words_a_person_would_type() {
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        struct Wrap {
            wifi_timeout: BadgeTimeout,
        }
        for (timeout, written) in [
            (BadgeTimeout::HalfMinute, "30s"),
            (BadgeTimeout::Minute, "1m"),
            (BadgeTimeout::FiveMinutes, "5m"),
            (BadgeTimeout::Never, "never"),
        ] {
            let encoded = toml::to_string(&Wrap {
                wifi_timeout: timeout,
            })
            .unwrap();
            assert!(
                encoded.contains(written),
                "{timeout:?} wrote {encoded}, expected {written}"
            );
            let decoded: Wrap = toml::from_str(&encoded).unwrap();
            assert_eq!(decoded.wifi_timeout, timeout);
        }
    }

    #[test]
    fn only_never_means_the_badge_has_no_deadline() {
        assert_eq!(BadgeTimeout::Never.duration(), None);
        let mut previous = std::time::Duration::ZERO;
        for timeout in [
            BadgeTimeout::HalfMinute,
            BadgeTimeout::Minute,
            BadgeTimeout::FiveMinutes,
        ] {
            let d = timeout.duration().expect("has a deadline");
            assert!(
                d > previous,
                "{timeout:?} must be longer than the one before"
            );
            previous = d;
        }
    }

    #[test]
    fn a_0_1_6_config_migrates_to_the_cells_the_packer_drew() {
        // Risk #1 in the design: a config from before free placement must open
        // looking identical. The migration runs the real packer, so this pins
        // the result against the packer rather than against hand-written cells.
        use crate::tile_layout::{migrate_from_packed, TileKey, TileShape};
        let raw = r#"
[appearance]
order = ["volume", "battery", "dns"]
shapes = { battery = "half" }

[modules]
wifi = false
"#;
        let parsed: Config = toml::from_str(raw).unwrap();
        let before = parsed.clone();
        let migrated = parsed.migrated();

        let expected =
            migrate_from_packed(&before.appearance.order, &before.appearance.shapes, |k| {
                before.module_enabled(k)
            });
        assert_eq!(migrated.appearance.layout, expected);
        assert!(migrated.appearance.layout[0].control.is(TileKey::Volume));
        assert_eq!(migrated.appearance.layout[1].shape, TileShape::Half);
        // A module switched off contributes no instance.
        assert!(!migrated
            .appearance
            .layout
            .iter()
            .any(|i| i.control.is(TileKey::Wifi)));
        // And the legacy keys are gone, so the next save drops them.
        assert!(migrated.appearance.order.is_empty());
        assert!(migrated.appearance.shapes.is_empty());
        let encoded = toml::to_string_pretty(&migrated).unwrap();
        assert!(!encoded.contains("order"));
        assert!(!encoded.contains("shapes"));
        assert!(encoded.contains("[[appearance.layout]]"));
    }

    #[test]
    fn custom_tiles_are_placed_on_the_grid_rather_than_appended_after_it() {
        // They used to be drawn after the grid and were not part of the
        // layout, so upgrading has to put them on it — otherwise a launcher
        // someone had simply disappears.
        let raw = r#"
[[custom]]
name = "Terminal"
command = ["true"]
shape = "half"

[[custom]]
name = "Off"
command = ["true"]
enabled = false
"#;
        let migrated: Config = toml::from_str::<Config>(raw).unwrap().migrated();
        let placed: Vec<_> = migrated
            .appearance
            .layout
            .iter()
            .filter(|i| matches!(i.control, crate::tile_layout::Control::Custom { .. }))
            .collect();
        assert_eq!(placed.len(), 1, "only the usable one is placed");
        assert_eq!(placed[0].control, crate::tile_layout::Control::custom(0));
        // At the shape its own entry asks for.
        assert_eq!(placed[0].shape, crate::tile_layout::TileShape::Half);
        // Placing it twice on a second load would stack duplicates.
        let again = migrated.clone().migrated();
        assert_eq!(again.appearance.layout, migrated.appearance.layout);
    }

    #[test]
    fn a_custom_instance_with_no_entry_behind_it_is_dropped() {
        // `[[custom]]` is hand-editable, so an index can outlive its tile. It
        // would draw nothing at all, leaving a hole nobody could fill.
        let raw = r#"
[[appearance.layout]]
control = { custom = 7 }
shape = "half"
col = 0
row = 0
"#;
        let migrated: Config = toml::from_str::<Config>(raw).unwrap().migrated();
        assert!(!migrated
            .appearance
            .layout
            .iter()
            .any(|i| matches!(i.control, crate::tile_layout::Control::Custom { .. })));
    }

    #[test]
    fn a_control_reads_as_a_name_and_a_custom_tile_as_a_table() {
        // The common case has to stay readable by hand.
        use crate::tile_layout::{Control, Instance, TileKey, TileShape};
        #[derive(Deserialize, Serialize, PartialEq, Debug)]
        struct Wrap {
            layout: Vec<Instance>,
        }
        let wrap = Wrap {
            layout: vec![
                Instance::new(TileKey::Battery, TileShape::Small, 0, 0),
                Instance::new(Control::custom(2), TileShape::Half, 2, 0),
            ],
        };
        let encoded = toml::to_string(&wrap).unwrap();
        assert!(encoded.contains(r#"control = "battery""#), "{encoded}");
        assert!(encoded.contains("custom = 2"), "{encoded}");
        assert_eq!(toml::from_str::<Wrap>(&encoded).unwrap(), wrap);
    }

    #[test]
    fn a_fresh_config_migrates_to_the_default_grid() {
        let migrated = Config::default().migrated();
        assert!(!migrated.appearance.layout.is_empty());
        assert_eq!(
            crate::tile_layout::validate(&migrated.appearance.layout),
            migrated.appearance.layout
        );
    }

    #[test]
    fn a_hand_edited_overlap_is_dropped_rather_than_drawn() {
        let raw = r#"
[[appearance.layout]]
control = "battery"
shape = "small"
col = 0
row = 0

[[appearance.layout]]
control = "dns"
shape = "small"
col = 1
row = 0
"#;
        let migrated: Config = toml::from_str::<Config>(raw).unwrap().migrated();
        assert_eq!(migrated.appearance.layout.len(), 1);
        assert!(migrated.appearance.layout[0]
            .control
            .is(crate::tile_layout::TileKey::Battery));
    }

    #[test]
    fn an_order_round_trips_and_an_empty_one_means_default() {
        use crate::tile_layout::TileKey;
        let with_order = Config {
            appearance: Appearance {
                order: vec![TileKey::Volume, TileKey::Battery],
                ..Appearance::default()
            },
            ..Config::default()
        };
        let encoded = toml::to_string_pretty(&with_order).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.appearance.order,
            vec![TileKey::Volume, TileKey::Battery]
        );

        // And a config with no `order` key at all — every config written
        // before this field existed — parses to the empty list.
        let older: Config = toml::from_str("[appearance]\nstyle = \"high\"\n").unwrap();
        assert!(older.appearance.order.is_empty());
    }

    #[test]
    fn a_shape_override_round_trips_and_absent_means_default() {
        use crate::tile_layout::{TileKey, TileShape};
        let mut shapes = std::collections::HashMap::new();
        shapes.insert(TileKey::Battery, TileShape::Half);
        let with = Config {
            appearance: Appearance {
                shapes,
                ..Appearance::default()
            },
            ..Config::default()
        };
        let encoded = toml::to_string_pretty(&with).unwrap();
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.appearance.shapes.get(&TileKey::Battery),
            Some(&TileShape::Half)
        );
        assert_eq!(
            TileKey::Battery.shape_with(&decoded.appearance.shapes),
            TileShape::Half
        );
        assert_eq!(
            TileKey::Dns.shape_with(&decoded.appearance.shapes),
            TileShape::Small
        );
    }

    #[test]
    fn an_older_config_without_the_appearance_section_still_loads() {
        // Anyone who wrote a config.toml before this section existed must not
        // have it rejected by deny_unknown_fields' stricter cousin, a missing
        // field.
        let older = r#"
[modules]
wifi = true
bluetooth = false
"#;
        let decoded: Config = toml::from_str(older).unwrap();
        assert!(decoded.modules.wifi);
        assert!(!decoded.modules.bluetooth);
        assert_eq!(decoded.appearance.style, TileStyle::High);
        assert_eq!(decoded.appearance.icon, PanelIcon::default());
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        let typo = "[modules]\nwifii = true\n";
        assert!(toml::from_str::<Config>(typo).is_err());
    }
}
