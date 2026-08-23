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
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    pub wifi: bool,
    pub bluetooth: bool,
    pub battery: bool,
    pub dns: bool,
    pub volume: bool,
    pub brightness: bool,
    pub dark_mode: bool,
    pub tiling: bool,
    pub gamemode: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Appearance {
    pub style: TileStyle,
    pub icon: PanelIcon,
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

/// What the panel button shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "lowercase")]
pub enum PanelIcon {
    /// Whatever the icon theme gives for the default name.
    #[default]
    System,
    /// One of the glyphs shipped in `data/icons`.
    Preset(String),
    /// An icon-theme name or an absolute path to an image the user supplied.
    Custom(String),
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
            wifi: true,
            bluetooth: true,
            battery: true,
            dns: true,
            volume: true,
            brightness: true,
            dark_mode: true,
            tiling: true,
            gamemode: true,
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
        assert_eq!(decoded.appearance.icon, PanelIcon::System);
    }

    #[test]
    fn an_unknown_key_is_rejected_rather_than_ignored() {
        let typo = "[modules]\nwifii = true\n";
        assert!(toml::from_str::<Config>(typo).is_err());
    }
}
