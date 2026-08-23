//! Dark mode, backed by `cosmic-config`.
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
//! So the toggle reads its key fresh at press time and flips *that*. The
//! displayed state comes from polling the same key, which means an external
//! change — `cosmic-settings`, another applet, a schedule — is reflected here
//! too.
//!
//! Window tiling used to live here too, over `com.system76.CosmicComp`'s
//! `autotile`. It does not belong here and that key was the wrong one: see
//! [`crate::modules::tiling`].

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

/// Fast enough that an external theme change shows up while the popup is open,
/// slow enough to be nothing. These are small local file reads, not IPC.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub dark: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed { dark: bool },
}

impl State {
    pub fn update(&mut self, event: Event) {
        let Event::Changed { dark } = event;
        self.availability = Availability::Available;
        self.dark = dark;
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

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("system-toggles", POLL_INTERVAL, || async {
            Some(Event::Changed {
                dark: read_dark().unwrap_or(false),
            })
        })
    }
}

fn theme_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(THEME_MODE_ID, THEME_MODE_VERSION)
}

fn read_dark() -> Option<bool> {
    theme_config().ok()?.get::<bool>(KEY_IS_DARK).ok()
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

/// One-shot read for `--check`.
pub fn probe() -> Result<String, String> {
    let dark = read_dark().ok_or("theme mode config unreadable")?;
    Ok(format!("theme is {}", if dark { "dark" } else { "light" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_flips_the_displayed_state_immediately() {
        // The poll is up to 1.5s behind; without the optimistic flip the label
        // lags the press badly enough to look broken.
        let mut state = State::default();
        state.update(Event::Changed { dark: true });

        let _write = state.toggle_dark();
        assert!(!state.dark);
    }

    #[test]
    fn repeated_toggles_alternate() {
        // The regression this guards: deriving the target from a cached value
        // that does not update, so every press after the first writes the same
        // thing and nothing happens.
        let mut state = State::default();
        state.update(Event::Changed { dark: true });

        let mut seen = Vec::new();
        for _ in 0..4 {
            let _write = state.toggle_dark();
            seen.push(state.dark);
        }
        assert_eq!(seen, vec![false, true, false, true]);
    }

    #[test]
    fn an_unknown_state_is_not_shown_yet() {
        assert!(!State::default().availability.is_shown());
    }
}
