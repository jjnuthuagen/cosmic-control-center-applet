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
    pub icon: PanelIcon,
    /// The order tiles are drawn in. Empty means the default order.
    ///
    /// Reconciled through [`crate::tile_layout::resolve_order`]: duplicates
    /// dropped first-wins, tiles missing from the list appended in default
    /// order so a config predating a tile still draws it, unknown names
    /// rejected at parse time by `deny_unknown_fields` on the enum.
    #[serde(default)]
    pub order: Vec<crate::tile_layout::TileKey>,
    /// Per-tile shape overrides: `shapes = { battery = "half", dns = "half" }`.
    ///
    /// Absent tiles use their default shape. `half` is icon-only, four to a
    /// row; the others are the shapes each tile already had.
    #[serde(default)]
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
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(err) => {
                tracing::warn!("could not read {}: {err}", path.display());
                return Self::default();
            }
        };

        match toml::from_str(&raw) {
            Ok(config) => config,
            Err(err) => {
                tracing::error!("{} is invalid, using defaults: {err}", path.display());
                Self::default()
            }
        }
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
