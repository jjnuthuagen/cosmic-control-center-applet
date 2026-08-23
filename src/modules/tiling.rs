//! Window tiling for the current workspace.
//!
//! # Why this is not a config write
//!
//! The obvious implementation — and the one this module replaced — sets
//! `autotile` in `com.system76.CosmicComp`. That is wrong, and it produced a
//! toggle that visibly did nothing.
//!
//! COSMIC's `autotile_behavior` defaults to `PerWorkspace`, and the stock
//! tiling applet forces it there on startup. Under that behaviour `autotile` is
//! only the **default applied to newly created workspaces**. The workspace you
//! are actually looking at keeps its own tiling state, held by the compositor
//! and reachable only over Wayland — `zcosmic_workspace_handle_v2`'s
//! `set_tiling_state`, followed by a commit on the workspace manager.
//!
//! So this module opens a short-lived Wayland connection, finds the active
//! workspace and drives that. It also *reads* tiling from the same place, which
//! matters: the config value and the real state of the current workspace
//! routinely disagree, and showing the config value would mean the tile lies
//! about what is happening on screen.
//!
//! # Why the connection is short-lived
//!
//! The alternative is a permanent Wayland client on its own thread with a
//! calloop loop. That is what the stock applet does, because it needs to react
//! to workspace changes continuously. This applet only needs an answer while
//! its popup is open, so connect-read-disconnect per poll is far less machinery
//! for the same result, and there is no thread to outlive the popup.

use cosmic::cctk::cosmic_protocols::workspace::v2::client::zcosmic_workspace_handle_v2;
use cosmic::cctk::sctk::registry::{ProvidesRegistryState, RegistryState};
use cosmic::cctk::wayland_client::globals::registry_queue_init;
use cosmic::cctk::wayland_client::{Connection, QueueHandle, WEnum};
use cosmic::cctk::wayland_protocols::ext::workspace::v1::client::ext_workspace_handle_v1;
use cosmic::cctk::workspace::{Workspace, WorkspaceHandler, WorkspaceState};
use cosmic::cctk::{delegate_workspace, sctk};
use cosmic::iced::Subscription;
use std::time::Duration;

use super::{poll_subscription, Availability};

const POLL_INTERVAL: Duration = Duration::from_millis(1500);

/// How many dispatch rounds to give the compositor to describe its workspaces.
///
/// The manager announces groups, then workspaces, then their state, so one
/// roundtrip is not enough. This is a ceiling, not a target — the loop stops as
/// soon as `done` fires.
const MAX_ROUNDTRIPS: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// Tiling is on for the workspace currently in view.
    pub tiled: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed { tiled: bool },
    Unavailable,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed { tiled } => {
                self.availability = Availability::Available;
                self.tiled = tiled;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.tiled = false;
            }
        }
    }

    pub fn toggle(&mut self) -> impl std::future::Future<Output = ()> {
        self.tiled = !self.tiled;
        let tiled = self.tiled;
        async move {
            if let Err(err) = set_tiling(tiled) {
                tracing::warn!("could not change window tiling: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("tiling", POLL_INTERVAL, || async {
            Some(match read_tiling() {
                Ok(tiled) => Event::Changed { tiled },
                Err(err) => {
                    tracing::debug!("tiling state unavailable: {err}");
                    Event::Unavailable
                }
            })
        })
    }
}

/// Wayland client state, alive only for the length of one query.
struct Client {
    registry_state: RegistryState,
    workspace_state: WorkspaceState,
    /// Set when the compositor has finished describing the current layout.
    done: bool,
}

impl WorkspaceHandler for Client {
    fn workspace_state(&mut self) -> &mut WorkspaceState {
        &mut self.workspace_state
    }

    fn done(&mut self) {
        self.done = true;
    }
}

impl ProvidesRegistryState for Client {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    sctk::registry_handlers!();
}

delegate_workspace!(Client);
sctk::delegate_registry!(Client);

/// Connect, let the compositor describe its workspaces, then hand them to `f`.
///
/// Runs synchronously. A Wayland roundtrip over the local socket is
/// sub-millisecond, and the alternative — a blocking thread-pool hop — costs
/// more than it saves for something this short.
fn with_workspaces<T>(f: impl FnOnce(&Client) -> Result<T, String>) -> Result<T, String> {
    let connection =
        Connection::connect_to_env().map_err(|err| format!("no Wayland connection: {err}"))?;
    let (globals, mut queue) = registry_queue_init::<Client>(&connection)
        .map_err(|err| format!("could not read the Wayland registry: {err}"))?;
    let handle: QueueHandle<Client> = queue.handle();

    let registry_state = RegistryState::new(&globals);
    let workspace_state = WorkspaceState::new(&registry_state, &handle);
    let mut client = Client {
        registry_state,
        workspace_state,
        done: false,
    };

    for _ in 0..MAX_ROUNDTRIPS {
        queue
            .roundtrip(&mut client)
            .map_err(|err| format!("Wayland roundtrip failed: {err}"))?;
        if client.done {
            break;
        }
    }

    if !client.done {
        // The compositor never finished describing its workspaces, so anything
        // read now would be a partial picture.
        return Err("compositor did not report its workspaces".to_string());
    }

    let result = f(&client)?;

    // Wayland requests are buffered client-side. Anything `f` sent — the
    // tiling change and its commit — is still sitting in that buffer, and
    // dropping the connection here would discard it. This roundtrip is what
    // actually delivers it, and its absence is a silent no-op: every call
    // succeeds and nothing happens.
    queue
        .roundtrip(&mut client)
        .map_err(|err| format!("Wayland roundtrip failed: {err}"))?;

    Ok(result)
}

/// The workspace currently in view.
///
/// Tiling is per-workspace, so every read and write has to be aimed at this one
/// rather than at whichever workspace happens to come first.
fn active_workspace(client: &Client) -> Option<&Workspace> {
    client.workspace_state.workspace_groups().find_map(|group| {
        group
            .workspaces
            .iter()
            .filter_map(|handle| client.workspace_state.workspace_info(handle))
            .find(|workspace| {
                workspace
                    .state
                    .contains(ext_workspace_handle_v1::State::Active)
            })
    })
}

fn read_tiling() -> Result<bool, String> {
    with_workspaces(|client| {
        let workspace = active_workspace(client).ok_or("no active workspace")?;
        match workspace.tiling {
            Some(WEnum::Value(zcosmic_workspace_handle_v2::TilingState::TilingEnabled)) => Ok(true),
            Some(WEnum::Value(zcosmic_workspace_handle_v2::TilingState::FloatingOnly)) => Ok(false),
            // The compositor is too old to report tiling, or sent a value this
            // build does not know. Either way, guessing would put the wrong
            // label on the tile.
            _ => Err("compositor does not report tiling state".to_string()),
        }
    })
}

fn set_tiling(tiled: bool) -> Result<(), String> {
    with_workspaces(|client| {
        let workspace = active_workspace(client).ok_or("no active workspace")?;
        let handle = workspace
            .cosmic_handle
            .as_ref()
            // The ext protocol alone has no tiling concept; without the COSMIC
            // extension there is nothing to set.
            .ok_or("workspace has no COSMIC handle")?;

        handle.set_tiling_state(if tiled {
            zcosmic_workspace_handle_v2::TilingState::TilingEnabled
        } else {
            zcosmic_workspace_handle_v2::TilingState::FloatingOnly
        });

        // Workspace requests are staged and only take effect on commit. Without
        // this the call is accepted and silently does nothing — the same class
        // of bug as writing the config default.
        client
            .workspace_state
            .workspace_manager()
            .get()
            .map_err(|_| "workspace manager went away".to_string())?
            .commit();

        Ok(())
    })
}

/// Flip tiling on the active workspace and report the new state.
///
/// Backs `--toggle-tiling`. Reads first so the flip is against what the
/// compositor actually has, not against anything cached.
pub fn toggle_now() -> Result<bool, String> {
    let target = !read_tiling()?;
    set_tiling(target)?;
    Ok(target)
}

/// One-shot read for `--check`.
pub fn probe() -> Result<String, String> {
    let tiled = read_tiling()?;
    Ok(format!(
        "active workspace is {}",
        if tiled { "tiled" } else { "floating" }
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
        state.update(Event::Changed { tiled: false });

        let _write = state.toggle();
        assert!(state.tiled);

        let _write = state.toggle();
        assert!(!state.tiled);
    }

    #[test]
    fn an_unreachable_compositor_hides_the_tile() {
        // Running outside COSMIC, or on a compositor without the workspace
        // extension. A toggle that cannot reach anything is worse than none.
        let mut state = State::default();
        state.update(Event::Changed { tiled: true });
        assert!(state.availability.is_shown());

        state.update(Event::Unavailable);
        assert!(!state.availability.is_shown());
        assert!(!state.tiled);
    }

    #[test]
    fn an_unknown_state_is_not_shown_yet() {
        // Guards the first-poll flicker: the tile must not appear before the
        // compositor has answered.
        assert!(!State::default().availability.is_shown());
    }
}
