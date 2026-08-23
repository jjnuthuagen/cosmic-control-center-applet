//! Desktop toggles backed by `cosmic-config`: dark mode and Do Not Disturb.
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

const NOTIFICATIONS_ID: &str = "com.system76.CosmicNotifications";
const NOTIFICATIONS_VERSION: u64 = 1;
const KEY_DND: &str = "do_not_disturb";

/// Fast enough that an external theme change shows up while the popup is open,
/// slow enough to be nothing. These are small local file reads, not IPC.
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub dark: bool,
    pub do_not_disturb: bool,
    /// False when the notifications config is absent, i.e. nothing would read
    /// the key we wrote.
    pub dnd_available: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed {
        dark: bool,
        do_not_disturb: Option<bool>,
    },
}

impl State {
    pub fn update(&mut self, event: Event) {
        let Event::Changed {
            dark,
            do_not_disturb,
        } = event;
        self.availability = Availability::Available;
        self.dark = dark;
        self.dnd_available = do_not_disturb.is_some();
        self.do_not_disturb = do_not_disturb.unwrap_or(false);
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

    pub fn toggle_do_not_disturb(&mut self) -> impl std::future::Future<Output = ()> {
        self.do_not_disturb = !self.do_not_disturb;
        async move {
            if let Err(err) = flip_dnd() {
                tracing::warn!("could not switch Do Not Disturb: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("system-toggles", POLL_INTERVAL, || async {
            Some(Event::Changed {
                dark: read_dark().unwrap_or(false),
                do_not_disturb: read_dnd(),
            })
        })
    }
}

fn theme_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(THEME_MODE_ID, THEME_MODE_VERSION)
}

fn notifications_config() -> Result<Config, cosmic::cosmic_config::Error> {
    Config::new(NOTIFICATIONS_ID, NOTIFICATIONS_VERSION)
}

fn read_dnd() -> Option<bool> {
    notifications_config().ok()?.get::<bool>(KEY_DND).ok()
}

fn flip_dnd() -> Result<(), cosmic::cosmic_config::Error> {
    let config = notifications_config()?;
    // Read-then-write, for the same reason dark mode does: never trust a cached
    // copy of a value another process also writes.
    let current = config.get::<bool>(KEY_DND).unwrap_or(false);
    config.set::<bool>(KEY_DND, !current)
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
    Ok(format!(
        "theme is {}, do not disturb {}",
        if dark { "dark" } else { "light" },
        match read_dnd() {
            Some(true) => "on",
            Some(false) => "off",
            None => "unavailable",
        }
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_flips_the_displayed_state_immediately() {
        // The poll is up to 1.5s behind; without the optimistic flip the label
        // lags the press badly enough to look broken.
        let mut state = State::default();
        state.update(Event::Changed {
            dark: true,
            do_not_disturb: Some(false),
        });

        let _write = state.toggle_dark();
        assert!(!state.dark);
    }

    #[test]
    fn repeated_toggles_alternate() {
        // The regression this guards: deriving the target from a cached value
        // that does not update, so every press after the first writes the same
        // thing and nothing happens.
        let mut state = State::default();
        state.update(Event::Changed {
            dark: true,
            do_not_disturb: Some(false),
        });

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
