//! Feral GameMode.
//!
//! # Why this is not a fourth power profile
//!
//! It looks like one, and it is tempting to put it next to Power Saver,
//! Balanced and Performance. It is not one. power-profiles-daemon reports
//! exactly three profiles on this hardware, and GameMode is a separate daemon
//! with a different model: it is **per-process and reference-counted**. A game
//! launched with `gamemoderun` registers its own PID, GameMode applies its
//! optimisations while at least one client is registered, and drops them when
//! the last one leaves.
//!
//! That makes it orthogonal to the power profile rather than an alternative to
//! it — you can be on Balanced with GameMode active — so it gets its own row.
//!
//! # Holding it on from a panel
//!
//! There is no "enable GameMode" call. The only way to hold it on is to be a
//! registered client, so the toggle registers *this process* and unregisters it
//! again. That is the same trick other panel integrations use, and it behaves
//! correctly: if the applet exits, its registration goes with it and GameMode
//! releases, rather than leaving the machine stuck in a tuned state.
//!
//! # Detecting it without starting it
//!
//! `com.feralinteractive.GameMode` is D-Bus **activatable**, so simply reading
//! `ClientCount` launches `gamemoded`. An earlier version of this module did
//! exactly that, which meant opening the popup started a daemon the user was not
//! running — a side effect no status display should have.
//!
//! So availability is decided from the bus's own name lists first: if the name
//! is neither owned nor activatable, GameMode is not installed and the row is
//! hidden. If it is activatable but not yet owned, nothing is running, so the
//! client count is zero by definition and there is no need to ask. Only a name
//! that is *already* owned gets queried.

use cosmic::iced::Subscription;
use std::time::Duration;

use super::{poll_subscription, Availability};

const POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[zbus::proxy(
    interface = "com.feralinteractive.GameMode",
    default_service = "com.feralinteractive.GameMode",
    default_path = "/com/feralinteractive/GameMode"
)]
trait GameMode {
    /// Returns 0 on success, negative on error.
    fn register_game(&self, pid: i32) -> zbus::Result<i32>;
    fn unregister_game(&self, pid: i32) -> zbus::Result<i32>;
    /// How many clients currently hold GameMode on.
    #[zbus(property)]
    fn client_count(&self) -> zbus::Result<i32>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// GameMode is active — by anyone, not necessarily us.
    pub active: bool,
    /// We are one of the registered clients.
    ///
    /// Tracked separately because unregistering a PID that was never registered
    /// is an error, and because a running game holding GameMode on is not
    /// something this applet should be able to switch off.
    pub held_by_us: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed { clients: i32 },
    Unavailable,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed { clients } => {
                self.availability = Availability::Available;
                self.active = clients > 0;
                // If the count fell to zero, whatever we thought we were holding
                // is gone — GameMode restarted, or something reset it.
                if !self.active {
                    self.held_by_us = false;
                }
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.active = false;
                self.held_by_us = false;
            }
        }
    }

    /// Whether the toggle should respond.
    ///
    /// Off when a *game* is holding GameMode on: this applet did not turn that
    /// on and should not be able to turn it off underneath a running game.
    pub fn can_toggle(&self) -> bool {
        !self.active || self.held_by_us
    }

    pub fn toggle(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        if !self.can_toggle() {
            return None;
        }

        let register = !self.held_by_us;
        self.held_by_us = register;
        self.active = register;

        Some(async move {
            if let Err(err) = hold(register).await {
                tracing::warn!("could not change GameMode: {err}");
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("gamemode", POLL_INTERVAL, || async {
            Some(match client_count().await {
                Ok(Some(clients)) => Event::Changed { clients },
                Ok(None) => Event::Unavailable,
                Err(err) => {
                    tracing::debug!("GameMode unavailable: {err}");
                    Event::Unavailable
                }
            })
        })
    }
}

/// Our own PID, which is what gets registered.
fn our_pid() -> i32 {
    // `id()` is a u32; PIDs fit in i32 on every platform this runs on, and a
    // saturating cast is still better than a wrapping one if that ever changes.
    std::process::id().min(i32::MAX as u32) as i32
}

/// How many clients hold GameMode, or `None` when it is not installed.
///
/// Never activates the daemon — see the note at the top of this module.
async fn client_count() -> zbus::Result<Option<i32>> {
    let connection = zbus::Connection::session().await?;
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let name = zbus::names::BusName::try_from(BUS_NAME)
        .map_err(|err| zbus::Error::Failure(err.to_string()))?;

    if bus.name_has_owner(name).await.unwrap_or(false) {
        return GameModeProxy::new(&connection)
            .await?
            .client_count()
            .await
            .map(Some);
    }

    let installed = bus
        .list_activatable_names()
        .await
        .unwrap_or_default()
        .iter()
        .any(|candidate| candidate.as_str() == BUS_NAME);

    // Installed but not running means nothing has registered, which is a zero
    // count without having to start it to find out.
    Ok(installed.then_some(0))
}

const BUS_NAME: &str = "com.feralinteractive.GameMode";

async fn hold(register: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = GameModeProxy::new(&connection).await?;
    let pid = our_pid();

    let result = if register {
        proxy.register_game(pid).await?
    } else {
        proxy.unregister_game(pid).await?
    };

    // GameMode reports failure in the return value, not as a D-Bus error, so a
    // plain `?` above would call a rejected request a success.
    if result < 0 {
        return Err(zbus::Error::Failure(format!(
            "GameMode refused the request (code {result})"
        )));
    }
    Ok(())
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    match client_count().await {
        Ok(Some(clients)) => Ok(format!(
            "{} client(s) registered, {}",
            clients,
            if clients > 0 { "active" } else { "idle" }
        )),
        Ok(None) => Err("gamemode is not installed".to_string()),
        Err(err) => Err(format!("could not ask the session bus: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_holding_gamemode_cannot_be_switched_off_here() {
        // Someone launched a game with gamemoderun. The applet did not turn that
        // on and must not be able to yank it out from under the game.
        let mut state = State::default();
        state.update(Event::Changed { clients: 1 });
        assert!(state.active);
        assert!(!state.held_by_us);
        assert!(!state.can_toggle());
        assert!(state.toggle().is_none());
    }

    #[test]
    fn our_own_hold_can_be_released() {
        let mut state = State::default();
        state.update(Event::Changed { clients: 0 });

        let action = state.toggle();
        assert!(action.is_some());
        assert!(state.held_by_us);
        assert!(state.active);
        assert!(state.can_toggle());

        let action = state.toggle();
        assert!(action.is_some());
        assert!(!state.held_by_us);
        assert!(!state.active);
    }

    #[test]
    fn the_count_falling_to_zero_clears_our_hold() {
        // gamemoded restarted, or something else reset it. Continuing to think
        // we hold a registration would make the toggle try to unregister a PID
        // gamemoded has never heard of.
        let mut state = State::default();
        state.update(Event::Changed { clients: 0 });
        let _write = state.toggle();
        assert!(state.held_by_us);

        state.update(Event::Changed { clients: 0 });
        assert!(!state.held_by_us);
        assert!(!state.active);
    }

    #[test]
    fn an_absent_daemon_hides_the_row() {
        let mut state = State::default();
        state.update(Event::Unavailable);
        assert!(!state.availability.is_shown());
    }

    #[test]
    fn a_pid_is_representable() {
        assert!(our_pid() > 0);
    }
}
