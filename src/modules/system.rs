//! Desktop toggles backed by `cosmic-config`: dark mode and window tiling.
//!
//! # Why these read the config instead of the running theme
//!
//! The obvious way to render the dark-mode toggle is `core.system_theme()`,
//! which the applet already holds. It is also wrong, and produced a real bug:
//! the cached theme does not update the instant the config is written, so
//! pressing the toggle a second time recomputed the target from a stale value,
//! wrote the same thing again, and appeared to do nothing. The toggle worked
//! exactly once per theme reload.
//!
//! So both toggles read their key fresh at press time and flip *that*. The
//! displayed state comes from polling the same key, which means an external
//! change — `cosmic-settings`, another applet, a schedule — is reflected here
//! too.

use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet};
use cosmic::iced::Subscription;
use std::time::Duration;

use super::{poll_subscription, Availability};

/// Theme mode lives in its own config, versioned separately from the compositor.
const THEME_MODE_ID: &str = "com.system76.CosmicTheme.Mode";
const THEME_MODE_VERSION: u64 = 1;
const KEY_IS_DARK: &str = "is_dark";
/// A day/night schedule that would otherwise undo a manual choice.
const KEY_AUTO_SWITCH: &str = "auto_switch";

const COMP_ID: &str = "com.system76.CosmicComp";
const COMP_VERSION: u64 = 1;
const KEY_AUTOTILE: &str = "autotile";

/// Fast enough that an external theme change shows up while the popup is open,
/// slow enough to be nothing. These are small local file reads, not IPC.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub dark: bool,
    pub tiling: bool,
    /// False when the compositor config is missing, i.e. not running COSMIC.
    pub tiling_available: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed { dark: bool, tiling: Option<bool> },
}

impl State {
    pub fn update(&mut self, event: Event) {
        let Event::Changed { dark, tiling } = event;
        self.availability = Availability::Available;
        self.dark = dark;
        self.tiling_available = tiling.is_some();
        self.tiling = tiling.unwrap_or(false);
    }

    /// Flip dark mode, reading the current value from the config first.
    pub fn toggle_dark(&mut self) -> impl std::future::Future<Output = ()> {
        // Optimistic for the label; the poll confirms.
        self.dark = !self.dark;
        async move {
            if let Err(err) = flip_dark() {
                tracing::warn!("could not switch theme mode: {err}");
            }
        }
    }

    pub fn toggle_tiling(&mut self) -> impl std::future::Future<Output = ()> {
        self.tiling = !self.tiling;
        async move {
            if let Err(err) = flip_tiling() {
                tracing::warn!("could not switch window tiling: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("system-toggles", POLL_INTERVAL, || async {
            Some(Event::Changed {
                dark: read_dark().unwrap_or(false),
                tiling: read_tiling(),
            })
        })
    }
}

fn theme_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(THEME_MODE_ID, THEME_MODE_VERSION)
}

fn comp_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(COMP_ID, COMP_VERSION)
}

fn read_dark() -> Option<bool> {
    theme_config().ok()?.get::<bool>(KEY_IS_DARK).ok()
}

fn read_tiling() -> Option<bool> {
    comp_config().ok()?.get::<bool>(KEY_AUTOTILE).ok()
}

fn flip_dark() -> Result<(), cosmic::cosmic_config::Error> {
    let config = theme_config()?;
    // Read-then-write, rather than trusting anything cached. This is the whole
    // point of the module — see the note at the top.
    let current = config.get::<bool>(KEY_IS_DARK).unwrap_or(false);

    // A day/night schedule would flip the theme back minutes later, which reads
    // as the toggle being flaky. Choosing a mode by hand means opting out of it.
    if config.get::<bool>(KEY_AUTO_SWITCH).unwrap_or(false) {
        config.set::<bool>(KEY_AUTO_SWITCH, false)?;
    }

    config.set::<bool>(KEY_IS_DARK, !current)
}

fn flip_tiling() -> Result<(), cosmic::cosmic_config::Error> {
    let config = comp_config()?;
    let current = config.get::<bool>(KEY_AUTOTILE).unwrap_or(false);
    config.set::<bool>(KEY_AUTOTILE, !current)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiling_hides_itself_when_the_compositor_config_is_absent() {
        // Running the applet outside COSMIC, or before cosmic-comp has written
        // its defaults. A toggle that writes into nothing is worse than none.
        let mut state = State::default();
        state.update(Event::Changed {
            dark: true,
            tiling: None,
        });
        assert!(state.availability.is_shown());
        assert!(!state.tiling_available);
    }

    #[test]
    fn tiling_state_is_reported_when_present() {
        let mut state = State::default();
        state.update(Event::Changed {
            dark: false,
            tiling: Some(true),
        });
        assert!(state.tiling_available);
        assert!(state.tiling);
        assert!(!state.dark);
    }

    #[test]
    fn toggling_flips_the_displayed_state_immediately() {
        // The poll is up to 1.5s behind; without the optimistic flip the label
        // lags the press badly enough to look broken.
        let mut state = State::default();
        state.update(Event::Changed {
            dark: true,
            tiling: Some(false),
        });

        let _write = state.toggle_dark();
        assert!(!state.dark);

        let _write = state.toggle_tiling();
        assert!(state.tiling);
    }

    #[test]
    fn repeated_toggles_alternate() {
        // The regression this guards: deriving the target from a cached value
        // that does not update, so every press after the first writes the same
        // thing and nothing happens.
        let mut state = State::default();
        state.update(Event::Changed {
            dark: true,
            tiling: None,
        });

        let mut seen = Vec::new();
        for _ in 0..4 {
            let _write = state.toggle_dark();
            seen.push(state.dark);
        }
        assert_eq!(seen, vec![false, true, false, true]);
    }
}
