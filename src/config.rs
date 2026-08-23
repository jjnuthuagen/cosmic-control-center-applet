//! User configuration, read once at startup from `config.toml`.
//!
//! The point of this file is the `[modules]` table. A module switched off here
//! is never constructed and never opens a bus connection — see
//! [`Config::enabled`] and how `App::subscription` uses it. That is what lets a
//! desktop user hide the battery tile without leaving a dead D-Bus client
//! behind.

use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub modules: Modules,
    pub dns: Dns,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Modules {
    pub wifi: bool,
    pub bluetooth: bool,
    pub battery: bool,
    pub dns: bool,
    pub volume: bool,
    pub brightness: bool,
    pub dark_mode: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
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
}
