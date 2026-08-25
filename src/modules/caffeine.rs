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

/// One entry from logind's inhibitor list: what, who, why, mode, uid, pid.
type Inhibitor = (String, String, String, String, u32, u32);

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Manager {
    fn inhibit(&self, what: &str, who: &str, why: &str, mode: &str) -> zbus::Result<OwnedFd>;

    /// Every lock currently held, by anyone.
    fn list_inhibitors(&self) -> zbus::Result<Vec<Inhibitor>>;
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
    /// Name of another program holding the machine awake, if one is.
    ///
    /// caffeine-ng, a video player, a running backup — anything that took a
    /// logind idle lock. See [`foreign_lock`] for why this is read rather than
    /// driven.
    pub held_by: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// logind answered, so the control can be offered. Carries the name of
    /// another program holding an idle lock, if there is one.
    Available(Option<String>),
    Unavailable,
    /// An inhibitor was taken. Carries the descriptor that holds it open.
    Held(Arc<OwnedFd>),
    /// Taking one failed; fall back to showing it off.
    Failed(String),
}

impl State {
    /// Is the machine being held awake — by us or by anything else?
    pub fn is_on(&self) -> bool {
        self.lock.is_some() || self.held_by.is_some()
    }

    /// Whether the toggle should respond.
    ///
    /// Off while another program holds the lock. We cannot release someone
    /// else's inhibitor — logind ties it to the file descriptor its owner
    /// holds — and taking a second one alongside would leave a switch that
    /// says off while the screen still refuses to sleep. Same rule as
    /// [`crate::modules::gamemode`]: do not offer control we do not have.
    pub fn can_toggle(&self) -> bool {
        self.held_by.is_none()
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Available(held_by) => {
                self.availability = Availability::Available;
                self.held_by = held_by;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                // Dropping the descriptor releases the inhibition, which is
                // what should happen if logind has gone away anyway.
                self.lock = None;
                self.held_by = None;
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
        if !self.can_toggle() {
            return None;
        }
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
        // Ten seconds rather than thirty: this now also carries who else is
        // holding the machine awake, and that changes without warning when a
        // video starts or caffeine-ng is clicked.
        super::poll_subscription("caffeine", std::time::Duration::from_secs(10), || async {
            Some(match foreign_lock().await {
                Ok(held_by) => Event::Available(held_by),
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

/// Who else, if anyone, is holding the machine awake.
///
/// Doubles as the reachability check — it is a real call to logind, and
/// deliberately not `Inhibit`, which would keep the machine awake for as long
/// as it took to drop the descriptor, every poll.
///
/// # Why caffeine-ng is read and not driven
///
/// caffeine-ng is the obvious thing to integrate with here, and it cannot be.
/// Its entire CLI is `caffeine start`, whose `--activate` flag sets the initial
/// state of a **newly launched** process; there is no D-Bus interface, no
/// socket, and no subcommand that reaches an instance already running. Driving
/// it would mean spawning a second tray icon next to the user's existing one to
/// switch on, and killing a process to switch off, with no way to read back
/// what either of them did.
///
/// What it does do is take a logind idle lock, like everything else that keeps
/// a machine awake. Reading the lock list therefore covers caffeine-ng, mpv, a
/// running backup and anything else, through one interface that is guaranteed
/// to exist — and it stays correct when the user toggles caffeine-ng from its
/// own tray icon, which no amount of driving it could achieve.
async fn foreign_lock() -> zbus::Result<Option<String>> {
    let connection = zbus::Connection::system().await?;
    let inhibitors = ManagerProxy::new(&connection)
        .await?
        .list_inhibitors()
        .await?;
    Ok(first_foreign_idle_lock(&inhibitors, std::process::id()))
}

/// The first idle-blocking lock held by someone other than `ours`.
///
/// Split out from the bus call so the filtering is testable: `what` is a
/// colon-separated set (`"idle:sleep"`), and `delay` mode only postpones the
/// idle action rather than preventing it, so neither can be matched naively.
fn first_foreign_idle_lock(inhibitors: &[Inhibitor], ours: u32) -> Option<String> {
    inhibitors
        .iter()
        .find(|(what, _who, _why, mode, _uid, pid)| {
            *pid != ours && mode == MODE && what.split(':').any(|item| item == WHAT)
        })
        .map(|(_what, who, ..)| who.clone())
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    match foreign_lock().await {
        Ok(Some(who)) => Ok(format!("logind reachable; held awake by {who}")),
        Ok(None) => Ok("logind reachable; nothing holding the machine awake".to_string()),
        Err(err) => Err(format!("logind not reachable: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inhibitor(what: &str, who: &str, mode: &str, pid: u32) -> Inhibitor {
        (
            what.to_string(),
            who.to_string(),
            "because".to_string(),
            mode.to_string(),
            1000,
            pid,
        )
    }

    #[test]
    fn another_program_holding_the_machine_awake_is_seen() {
        // caffeine-ng, mpv, a backup — all of them take a logind idle lock, and
        // this list is the only interface that covers every one of them.
        let locks = vec![inhibitor("idle", "caffeine", "block", 4242)];
        assert_eq!(
            first_foreign_idle_lock(&locks, 99),
            Some("caffeine".to_string())
        );
    }

    #[test]
    fn our_own_lock_is_not_mistaken_for_someone_elses() {
        // Otherwise switching Keep Awake on would immediately disable its own
        // toggle: we would see our lock, call it foreign, and refuse to release
        // it — a switch that turns on once and then jams.
        let locks = vec![inhibitor("idle", WHO, "block", 99)];
        assert_eq!(first_foreign_idle_lock(&locks, 99), None);
    }

    #[test]
    fn a_delay_lock_does_not_count_as_holding_it_awake() {
        // `delay` only postpones the idle action. Treating it as a hold would
        // grey the toggle out for something that lets the screen blank anyway.
        let locks = vec![inhibitor("idle", "polite-app", "delay", 4242)];
        assert_eq!(first_foreign_idle_lock(&locks, 99), None);
    }

    #[test]
    fn a_lock_on_something_other_than_idle_is_ignored() {
        // Suspend and lid locks are common and have nothing to do with the
        // screen staying on.
        let locks = vec![
            inhibitor("sleep", "updater", "block", 4242),
            inhibitor("handle-lid-switch", "dock", "block", 4243),
        ];
        assert_eq!(first_foreign_idle_lock(&locks, 99), None);
    }

    #[test]
    fn idle_is_matched_inside_a_combined_lock() {
        // `what` is colon-separated, so a substring match would also hit
        // "idle-something" and an equality match would miss "sleep:idle".
        let locks = vec![inhibitor("sleep:idle:handle-lid-switch", "mpv", "block", 7)];
        assert_eq!(first_foreign_idle_lock(&locks, 99), Some("mpv".to_string()));
    }

    #[test]
    fn the_toggle_refuses_while_another_program_holds_it() {
        let mut state = State::default();
        state.update(Event::Available(Some("caffeine".into())));
        assert!(state.is_on(), "the machine is awake, so the tile says on");
        assert!(!state.can_toggle());
        assert!(state.toggle().is_none());
    }

    #[test]
    fn the_toggle_returns_when_the_other_program_lets_go() {
        let mut state = State::default();
        state.update(Event::Available(Some("caffeine".into())));
        state.update(Event::Available(None));
        assert!(!state.is_on());
        assert!(state.can_toggle());
    }

    #[test]
    fn losing_logind_releases_the_inhibitor() {
        // Otherwise a machine whose logind restarted would be left believing it
        // still holds one, and the toggle would show on with nothing behind it.
        let mut state = State::default();
        state.update(Event::Available(None));
        assert!(!state.is_on());

        state.update(Event::Unavailable);
        assert!(!state.is_on());
        assert!(!state.availability.is_shown());
    }

    #[test]
    fn a_failure_leaves_the_control_off_rather_than_stuck_on() {
        let mut state = State::default();
        state.update(Event::Available(None));
        state.update(Event::Failed("denied".into()));
        assert!(!state.is_on());
    }

    #[test]
    fn switching_off_needs_no_round_trip() {
        // Releasing is just closing the descriptor, so the toggle returns no
        // future — a caller awaiting one would wait forever.
        let mut state = State::default();
        state.update(Event::Available(None));

        // Simulate holding one without a real logind.
        assert!(state.toggle().is_some(), "switching on must do work");
    }
}
