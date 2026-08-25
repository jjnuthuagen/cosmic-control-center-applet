//! One Settings window, however many times you right-click the panel button.
//!
//! The applet opens Settings by spawning itself with `--settings`, which is the
//! simplest thing that works and, on its own, opens a new window every time.
//! Ten right-clicks left ten identical windows to close.
//!
//! # Why a bus name and not a lock file
//!
//! A lock file answers "is one already running?" and nothing else. The useful
//! behaviour is the second half — *reveal* the one that is running — and that
//! needs the first process to be told. A well-known bus name gives both from
//! one mechanism: claiming it is the mutual exclusion, and a method call on it
//! is the request to come forward.
//!
//! It also fails in the right direction. If the session bus is unreachable, the
//! window opens anyway; a settings window that refuses to start because it
//! could not talk to D-Bus would be a worse bug than the one this fixes.

use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc;

const NAME: &str = "io.github.jjnuthuagen.ControlCenterSettings";
const PATH: &str = "/io/github/jjnuthuagen/ControlCenterSettings";

/// What [`claim`] found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Claim {
    /// Nothing else held the name. This process owns the window.
    Ours,
    /// Another window is already open and has been asked to come forward.
    /// This process should exit without drawing anything.
    AlreadyOpen,
}

/// Requests from later invocations, read by the window's subscription.
static PRESENT: OnceLock<Mutex<Option<mpsc::UnboundedReceiver<()>>>> = OnceLock::new();

#[zbus::proxy(
    interface = "io.github.jjnuthuagen.ControlCenterSettings",
    default_service = "io.github.jjnuthuagen.ControlCenterSettings",
    default_path = "/io/github/jjnuthuagen/ControlCenterSettings"
)]
trait Present {
    /// Ask the running window to raise and focus itself.
    fn present(&self) -> zbus::Result<()>;
}

/// The object served by whichever process owns the name.
struct Service {
    requests: mpsc::UnboundedSender<()>,
}

#[zbus::interface(name = "io.github.jjnuthuagen.ControlCenterSettings")]
impl Service {
    fn present(&self) {
        // A closed receiver means the window is on its way out; the caller is
        // about to find the name free and open its own.
        let _ = self.requests.send(());
    }
}

/// Claim the name, or ask the existing window to come forward.
///
/// Blocks briefly on a background thread that keeps running for the life of the
/// process: the connection has to stay alive to keep serving `Present`, and
/// dropping it would release the name and let the next right-click open a
/// second window.
pub fn claim() -> Claim {
    let (answer_tx, answer_rx) = std::sync::mpsc::channel();
    let (present_tx, present_rx) = mpsc::unbounded_channel();

    let started = std::thread::Builder::new()
        .name("settings-single-instance".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(err) => {
                    tracing::warn!("no runtime for the single-instance check: {err}");
                    let _ = answer_tx.send(Claim::Ours);
                    return;
                }
            };

            runtime.block_on(async move {
                match register(present_tx).await {
                    Ok(Registration {
                        claim: Claim::Ours,
                        connection: Some(connection),
                    }) => {
                        let _ = answer_tx.send(Claim::Ours);
                        // Hold `connection` for the process's lifetime: dropping
                        // it releases the name, and the next right-click would
                        // then find it free. `pending` is what keeps this
                        // runtime alive and, with it, the connection's reader.
                        let _hold = connection;
                        std::future::pending::<()>().await;
                    }
                    Ok(Registration {
                        claim: Claim::AlreadyOpen,
                        ..
                    }) => {
                        let _ = answer_tx.send(Claim::AlreadyOpen);
                    }
                    // Not reachable — Ours always carries a connection — but a
                    // panic here would be silent behind the thread boundary, so
                    // fall through to opening the window.
                    Ok(Registration {
                        claim: Claim::Ours,
                        connection: None,
                    }) => {
                        let _ = answer_tx.send(Claim::Ours);
                    }
                    Err(err) => {
                        // No bus, or it refused. Opening a window is the safer
                        // failure: a duplicate is an annoyance, a settings
                        // window that will not open is a broken feature.
                        tracing::debug!("single-instance check unavailable: {err}");
                        let _ = answer_tx.send(Claim::Ours);
                    }
                }
            });
        });

    if let Err(err) = started {
        tracing::warn!("could not start the single-instance check: {err}");
        return Claim::Ours;
    }

    let claim = answer_rx.recv().unwrap_or(Claim::Ours);
    if claim == Claim::Ours {
        let _ = PRESENT.set(Mutex::new(Some(present_rx)));
    }
    claim
}

/// The claim's outcome, plus the connection to hang on to when we own it.
///
/// The connection has to be returned to the caller and held for the life of
/// the process, not dropped at the end of this function: dropping it releases
/// the bus name, and the next right-click sees it free and opens its own
/// window — which is exactly the bug this whole module exists to fix.
struct Registration {
    claim: Claim,
    connection: Option<zbus::Connection>,
}

async fn register(requests: mpsc::UnboundedSender<()>) -> zbus::Result<Registration> {
    use zbus::fdo::RequestNameFlags;
    use zbus::fdo::RequestNameReply;

    let connection = zbus::Connection::session().await?;
    connection
        .object_server()
        .at(PATH, Service { requests })
        .await?;

    // `DO_NOT_QUEUE` is what makes this a test rather than a wait: without it
    // a second instance would sit in the queue and take the name the moment
    // the first window closed.
    //
    // zbus 5 turns REPLY_EXISTS into `Err(Error::NameTaken)` rather than
    // returning an `Ok(RequestNameReply::Exists)`, so the "someone else has
    // it" branch has to match on the error, not on the reply. Reaching for the
    // reply was the whole reason `--settings` opened a new window every time.
    let reply = match connection
        .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(reply) => reply,
        Err(zbus::Error::NameTaken) => {
            if let Ok(proxy) = PresentProxy::new(&connection).await {
                if let Err(err) = proxy.present().await {
                    tracing::debug!("could not raise the open window: {err}");
                }
            }
            return Ok(Registration {
                claim: Claim::AlreadyOpen,
                connection: None,
            });
        }
        Err(err) => return Err(err),
    };

    if matches!(
        reply,
        RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner
    ) {
        return Ok(Registration {
            claim: Claim::Ours,
            connection: Some(connection),
        });
    }

    // Anything else (e.g. `InQueue`, which shouldn't happen with `DO_NOT_QUEUE`
    // but is not impossible if the flag is ever removed): treat as taken.
    if let Ok(proxy) = PresentProxy::new(&connection).await {
        if let Err(err) = proxy.present().await {
            tracing::debug!("could not raise the open window: {err}");
        }
    }
    Ok(Registration {
        claim: Claim::AlreadyOpen,
        connection: None,
    })
}

/// A stream of "come forward" requests from later invocations.
///
/// Yields nothing at all when this process did not claim the name, or when the
/// receiver has already been taken — the subscription is built once, but iced
/// is free to call `subscription()` as often as it likes.
pub fn requests() -> Option<mpsc::UnboundedReceiver<()>> {
    PRESENT.get()?.lock().ok()?.take()
}
