//! VPN connections, via NetworkManager.
//!
//! Lists saved VPN profiles and activates or deactivates them. Creating or
//! importing a profile stays in `cosmic-settings` — that needs certificates,
//! credentials and per-protocol options that do not fit a panel popup.
//!
//! Activation goes through `network-control`, which polkit grants to active
//! sessions outright, so switching a saved VPN on and off needs no prompt. The
//! credentials themselves are NetworkManager's problem: if a profile stores its
//! secrets, activation is silent; if it does not, NetworkManager raises its own
//! agent prompt, which is the correct behaviour and not something to work
//! around here.

use cosmic::iced::Subscription;
use std::time::Duration;

use super::{poll_subscription, Availability};

const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// A saved VPN profile, reduced to what the UI draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub uuid: String,
    pub active: bool,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub profiles: Vec<Profile>,
    /// UUID of a profile with a connect or disconnect in flight.
    pub busy: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed(Vec<Profile>),
    /// NetworkManager is not running, or there are no VPN profiles at all.
    Unavailable,
}

impl State {
    /// Name of whichever profile is up, for the tile's state line.
    pub fn active_name(&self) -> Option<&str> {
        self.profiles
            .iter()
            .find(|p| p.active)
            .map(|p| p.name.as_str())
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed(profiles) => {
                self.availability = Availability::Available;
                self.profiles = profiles;
                // Whatever was in flight has landed or failed; the fresh list
                // is the truth now.
                self.busy = None;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.profiles.clear();
                self.busy = None;
            }
        }
    }

    /// One-press toggle for the grouped tile: disconnect whatever is active,
    /// else connect the first saved profile.
    ///
    /// The standalone VPN page lists every profile and lets you pick; the row
    /// inside the Connectivity tile has one switch and no room for a list, so
    /// it needs a sensible default. "First saved profile" is the least
    /// surprising: it is what the page shows at the top, and a user with one
    /// VPN — the common case — gets exactly what they expect.
    pub fn toggle_quick(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        let uuid = self
            .profiles
            .iter()
            .find(|p| p.active)
            .or_else(|| self.profiles.first())
            .map(|p| p.uuid.clone())?;
        self.toggle(&uuid)
    }

    pub fn toggle(&mut self, uuid: &str) -> Option<impl std::future::Future<Output = ()>> {
        let profile = self.profiles.iter_mut().find(|p| p.uuid == uuid)?;
        let connect = !profile.active;

        // Optimistic, plus a busy marker: a VPN handshake takes seconds, and a
        // row that does nothing visible in that window reads as a dead button.
        profile.active = connect;
        let uuid = profile.uuid.clone();
        self.busy = Some(uuid.clone());

        Some(async move {
            if let Err(err) = set_active(&uuid, connect).await {
                tracing::warn!("could not change VPN connection: {err}");
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("vpn", POLL_INTERVAL, || async {
            Some(match profiles().await {
                Ok(profiles) if profiles.is_empty() => Event::Unavailable,
                Ok(profiles) => Event::Changed(profiles),
                Err(err) => {
                    tracing::debug!("VPN list unavailable: {err}");
                    Event::Unavailable
                }
            })
        })
    }
}

async fn profiles() -> Result<Vec<Profile>, String> {
    let manager = nmrs::NetworkManager::new()
        .await
        .map_err(|err| err.to_string())?;

    let saved = manager
        .list_vpn_connections()
        .await
        .map_err(|err| err.to_string())?;
    let active = manager.active_vpn_connections().await.unwrap_or_default();

    let mut profiles: Vec<Profile> = saved
        .into_iter()
        .map(|vpn| Profile {
            active: vpn.active || active.iter().any(|a| a.uuid == vpn.uuid),
            name: if vpn.name.is_empty() {
                vpn.id.clone()
            } else {
                vpn.name.clone()
            },
            uuid: vpn.uuid,
        })
        .collect();

    // Active first, then alphabetical, so the one that is up is never buried.
    profiles.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(profiles)
}

async fn set_active(uuid: &str, connect: bool) -> Result<(), String> {
    let manager = nmrs::NetworkManager::new()
        .await
        .map_err(|err| err.to_string())?;

    if connect {
        manager
            .connect_vpn_by_uuid(uuid)
            .await
            .map_err(|err| err.to_string())
    } else {
        manager
            .disconnect_vpn_by_uuid(uuid)
            .await
            .map_err(|err| err.to_string())
    }
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    let profiles = profiles().await?;
    if profiles.is_empty() {
        return Err("no VPN profiles are saved".to_string());
    }
    Ok(format!(
        "{} profile(s), active: {}",
        profiles.len(),
        profiles
            .iter()
            .find(|p| p.active)
            .map_or("none", |p| p.name.as_str())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, active: bool) -> Profile {
        Profile {
            name: name.to_string(),
            uuid: format!("uuid-{name}"),
            active,
        }
    }

    #[test]
    fn no_saved_profiles_hides_the_tile() {
        // A VPN tile that lists nothing is worse than no tile.
        let mut state = State::default();
        state.update(Event::Unavailable);
        assert!(!state.availability.is_shown());
    }

    #[test]
    fn the_active_profile_names_the_tile() {
        let mut state = State::default();
        state.update(Event::Changed(vec![
            profile("Work", false),
            profile("Home", true),
        ]));
        assert_eq!(state.active_name(), Some("Home"));
    }

    #[test]
    fn nothing_active_reads_as_none() {
        let mut state = State::default();
        state.update(Event::Changed(vec![profile("Work", false)]));
        assert_eq!(state.active_name(), None);
    }

    #[test]
    fn toggling_marks_the_row_busy_and_flips_it() {
        let mut state = State::default();
        state.update(Event::Changed(vec![profile("Work", false)]));

        let action = state.toggle("uuid-Work");
        assert!(action.is_some());
        assert!(state.profiles[0].active);
        assert_eq!(state.busy.as_deref(), Some("uuid-Work"));
    }

    #[test]
    fn toggling_an_unknown_profile_does_nothing() {
        let mut state = State::default();
        state.update(Event::Changed(vec![profile("Work", false)]));
        assert!(state.toggle("uuid-Missing").is_none());
    }

    #[test]
    fn a_fresh_list_clears_the_busy_marker() {
        let mut state = State::default();
        state.update(Event::Changed(vec![profile("Work", false)]));
        let _write = state.toggle("uuid-Work");
        state.update(Event::Changed(vec![profile("Work", true)]));
        assert!(state.busy.is_none());
    }
}
