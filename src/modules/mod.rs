//! Backend modules.
//!
//! Each module owns one concern, talks to exactly one system service, and knows
//! how to disappear. Two rules hold everywhere in here:
//!
//! 1. **Absence is normal, not exceptional.** No battery, no `bluetoothd`, no
//!    backlight — the module reports itself unavailable and the UI omits it. A
//!    module must never panic or block startup because its hardware is missing.
//! 2. **No blocking in `update`.** Every backend call returns a `Task` and every
//!    state feed is a `Subscription`. The iced update loop drives the whole
//!    panel; a synchronous D-Bus round-trip in there freezes it.

pub mod battery;
pub mod bluetooth;
pub mod brightness;
pub mod dns;
pub mod network;
pub mod system;
pub mod volume;

use cosmic::iced::Subscription;
use futures::{stream, StreamExt};
use std::time::Duration;

/// Whether a module has a working backend behind it.
///
/// `Unavailable` is a first-class outcome: it means "this machine has no such
/// hardware or daemon", which is the desktop-without-a-battery case. It is not
/// an error and is not surfaced to the user beyond the tile being absent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Availability {
    /// Not probed yet. Modules start here so the UI can avoid flashing a tile
    /// in and then out again during the first second of a session.
    #[default]
    Unknown,
    Available,
    Unavailable,
}

impl Availability {
    /// Should the tile be drawn? `Unknown` counts as no, so the popup settles
    /// into its final shape instead of reflowing as each backend answers.
    pub fn is_shown(self) -> bool {
        matches!(self, Availability::Available)
    }
}

/// Repeatedly run `poll`, emitting whatever it produces.
///
/// Used where a service has no change signal to subscribe to (sysfs backlight,
/// PipeWire via `wpctl`). The interval is a deliberate compromise: fast enough
/// that pressing a hardware key updates the slider while the popup is open,
/// slow enough to be invisible in idle power draw.
///
/// `id` must be unique per subscription — iced deduplicates by the `data`
/// argument, so two pollers sharing an id silently collapse into one.
///
/// Note the shape: `Subscription::run_with` takes a **function pointer**, not a
/// closure, so nothing can be captured. Everything the stream needs has to
/// travel in the `data` tuple, which is why `poll` is `fn() -> Fut` rather than
/// a generic `F: Fn`.
pub fn poll_subscription<T, Fut>(
    id: &'static str,
    interval: Duration,
    poll: fn() -> Fut,
) -> Subscription<T>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = Option<T>> + Send + 'static,
{
    Subscription::run_with((id, interval, poll), |&(_, interval, poll)| {
        stream::unfold(true, move |first| async move {
            // Don't wait before the very first sample, or the tile shows stale
            // defaults for a whole interval after the popup opens.
            if !first {
                tokio::time::sleep(interval).await;
            }
            Some((poll().await, false))
        })
        .filter_map(|value| async move { value })
    })
}

/// State machine for [`dbus_subscription`].
enum ConnectionState<S> {
    Disconnected { attempts: u32 },
    Connected(S),
}

/// Connect to a bus, then stream events from it, retrying on failure.
///
/// The retry matters on a cold session: an applet can start before
/// `bluetoothd` or NetworkManager have claimed their names, and a module that
/// gave up on the first refusal would stay blank until the user logged out.
/// Once `connect` succeeds we stay on its stream until it ends, then try again.
///
/// Same function-pointer constraint as [`poll_subscription`].
pub fn dbus_subscription<T, Fut, S>(id: &'static str, connect: fn() -> Fut) -> Subscription<T>
where
    T: Send + 'static,
    Fut: std::future::Future<Output = zbus::Result<S>> + Send + 'static,
    S: futures::Stream<Item = T> + Send + Unpin + 'static,
{
    Subscription::run_with((id, connect), |&(_, connect)| {
        stream::unfold(
            ConnectionState::<S>::Disconnected { attempts: 0 },
            move |state| async move {
                match state {
                    ConnectionState::Disconnected { attempts } => {
                        if attempts > 0 {
                            // Back off, but cap it: a service that appears ten
                            // minutes into a session should still be picked up
                            // reasonably promptly.
                            let delay = Duration::from_secs(2u64.pow(attempts.min(4)));
                            tokio::time::sleep(delay).await;
                        }
                        match connect().await {
                            Ok(stream) => Some((None, ConnectionState::Connected(stream))),
                            Err(err) => {
                                tracing::debug!("bus connect failed (attempt {attempts}): {err}");
                                Some((
                                    None,
                                    ConnectionState::Disconnected {
                                        attempts: attempts.saturating_add(1),
                                    },
                                ))
                            }
                        }
                    }
                    ConnectionState::Connected(mut stream) => match stream.next().await {
                        Some(event) => Some((Some(event), ConnectionState::Connected(stream))),
                        // The stream ended, which means the service dropped off
                        // the bus. Fall back to reconnecting rather than ending
                        // the subscription, which iced would never restart.
                        None => Some((None, ConnectionState::Disconnected { attempts: 1 })),
                    },
                }
            },
        )
        .filter_map(|event| async move { event })
    })
}

/// Clamp a 0.0..=100.0 percentage that came from a backend we don't control.
pub fn clamp_percent(value: f64) -> f64 {
    value.clamp(0.0, 100.0)
}
