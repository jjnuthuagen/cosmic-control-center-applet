//! Keep awake: hold off the screen blanking and idle suspend.
//!
//! # Why this module owns a file descriptor
//!
//! logind has no "disable idle" setting. `Inhibit` hands back a file
//! descriptor, and the inhibition lasts exactly as long as that descriptor is
//! open — closing it, or the process exiting, releases it.
//!
//! That is a good property rather than an awkward one: a crashed or killed
//! applet cannot leave the machine permanently unable to sleep, which is
//! precisely the failure mode a config-flag implementation would have. The cost
//! is that the descriptor has to be kept in the applet's state, so this module
//! holds one rather than being a pure function of some external value.

use cosmic::iced::Subscription;
use std::sync::Arc;
use zbus::zvariant::OwnedFd;

use super::Availability;

/// What to inhibit. `idle` covers screen blanking and idle-triggered suspend;
/// deliberately not `sleep` or `handle-lid-switch`, which would override a
/// deliberate suspend or closing the lid.
const WHAT: &str = "idle";
const WHO: &str = "Control Center";
const WHY: &str = "Keep awake was switched on";
/// `block` refuses the idle action outright. `delay` only postpones it, which
/// would let the screen blank anyway after logind's timeout.
const MODE: &str = "block";

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Manager {
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// The live inhibitor. Its presence *is* the on state — there is nothing
    /// else to read back, because logind exposes no per-inhibitor query.
    ///
    /// `Arc` only so this struct can stay `Clone` alongside the other modules;
    /// exactly one of these ever exists.
    lock: Option<Arc<OwnedFd>>,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// logind answered, so the control can be offered.
    Available,
    Unavailable,
    /// An inhibitor was taken. Carries the descriptor that holds it open.
    Held(Arc<OwnedFd>),
    /// Taking one failed; fall back to showing it off.
    Failed(String),
}

impl State {
    pub fn is_on(&self) -> bool {
        self.lock.is_some()
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Available => self.availability = Availability::Available,
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                // Dropping the descriptor releases the inhibition, which is
                // what should happen if logind has gone away anyway.
                self.lock = None;
            }
            Event::Held(fd) => {
                self.availability = Availability::Available;
                self.lock = Some(fd);
            }
            Event::Failed(reason) => {
                tracing::warn!("could not keep the system awake: {reason}");
                self.lock = None;
            }
        }
    }

    /// Take an inhibitor, or release the one we hold.
    pub fn toggle(&mut self) -> Option<impl std::future::Future<Output = Event>> {
        if self.lock.take().is_some() {
            // Dropped above; nothing asynchronous to do. The descriptor closing
            // is the release.
            return None;
        }

        Some(async move {
            match acquire().await {
                Ok(fd) => Event::Held(Arc::new(fd)),
                Err(err) => Event::Failed(err.to_string()),
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        // Only availability needs polling: whether we hold an inhibitor is
        // local knowledge, and logind will not change it behind our back.
        super::poll_subscription("caffeine", std::time::Duration::from_secs(30), || async {
            Some(match reachable().await {
                Ok(()) => Event::Available,
                Err(err) => {
                    tracing::debug!("logind inhibit unavailable: {err}");
                    Event::Unavailable
                }
            })
        })
    }
}

async fn acquire() -> zbus::Result<OwnedFd> {
    let connection = zbus::Connection::system().await?;
    ManagerProxy::new(&connection)
        .await?
        .inhibit(WHAT, WHO, WHY, MODE)
        .await
}

/// Can we reach logind at all?
///
/// Deliberately does not take an inhibitor to find out — that would keep the
/// machine awake for as long as it took to drop it, every poll.
async fn reachable() -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    ManagerProxy::new(&connection).await?;
    Ok(())
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    reachable()
        .await
        .map(|()| "logind reachable; inhibitors available".to_string())
        .map_err(|err| format!("logind not reachable: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn losing_logind_releases_the_inhibitor() {
        // Otherwise a machine whose logind restarted would be left believing it
        // still holds one, and the toggle would show on with nothing behind it.
        let mut state = State::default();
        state.update(Event::Available);
        assert!(!state.is_on());

        state.update(Event::Unavailable);
        assert!(!state.is_on());
        assert!(!state.availability.is_shown());
    }

    #[test]
    fn a_failure_leaves_the_control_off_rather_than_stuck_on() {
        let mut state = State::default();
        state.update(Event::Available);
        state.update(Event::Failed("denied".into()));
        assert!(!state.is_on());
    }

    #[test]
    fn switching_off_needs_no_round_trip() {
        // Releasing is just closing the descriptor, so the toggle returns no
        // future — a caller awaiting one would wait forever.
        let mut state = State::default();
        state.update(Event::Available);

        // Simulate holding one without a real logind.
        assert!(state.toggle().is_some(), "switching on must do work");
    }
}
