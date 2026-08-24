//! Wi-Fi: radio state, visible networks, joining, airplane mode and connection
//! details.
//!
//! # Provenance
//!
//! This is an independent implementation. The stock `cosmic-applet-network` is
//! GPL-3.0-or-later and this crate is MIT/Apache-2.0 dual, so none of its code
//! is used here. What *is* shared is the underlying binding: `nmrs`, which is
//! MIT and does the awkward parts of NetworkManager (security-flag decoding,
//! secret agents, connection activation state machines) that are not worth
//! reimplementing against raw D-Bus.
//!
//! # Privileges
//!
//! `enable-disable-wifi` and `network-control` are both granted to active
//! sessions outright, so toggling the radio and activating a saved connection
//! never prompt. Joining a *new* network writes a connection profile, which
//! NetworkManager may scope as system-owned — see [`crate::modules::dns`] for
//! the same caveat, and [`Error::NeedsAuthorisation`] for how it surfaces.

use cosmic::iced::Subscription;
use std::time::Duration;

use super::Availability;

/// How often the network list refreshes while the Wi-Fi page is open.
///
/// Scanning is the expensive part, so this only runs while the user is looking
/// at the list. The tile itself needs far less.
const SCAN_INTERVAL: Duration = Duration::from_secs(4);
/// Refresh rate for the tile summary when the Wi-Fi page is closed.
const IDLE_INTERVAL: Duration = Duration::from_secs(15);

/// A visible network, reduced to what the UI draws.
///
/// Deliberately not `nmrs::Network`: keeping our own type means the widget code
/// and the tests do not depend on the binding's shape, and swapping the binding
/// later touches only this file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub ssid: String,
    /// 0-100.
    pub strength: u8,
    /// Needs a credential of some kind.
    pub secured: bool,
    /// 802.1X. We can join these only if a profile already exists — building an
    /// enterprise profile needs certificates and an identity, which is a
    /// Settings-sized job, not a panel-sized one.
    pub enterprise: bool,
    /// A saved profile exists, so joining needs no password from us.
    pub known: bool,
    pub connected: bool,
}

impl Network {
    /// Can this be joined from the applet, and does doing so need a password?
    pub fn join_kind(&self) -> JoinKind {
        if self.connected {
            JoinKind::AlreadyConnected
        } else if self.known || !self.secured {
            // A saved profile carries its own secrets; an open network needs
            // none. Either way we can activate without asking the user.
            JoinKind::Direct
        } else if self.enterprise {
            JoinKind::UnsupportedEnterprise
        } else {
            JoinKind::NeedsPassword
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    AlreadyConnected,
    Direct,
    NeedsPassword,
    /// Enterprise network with no saved profile — we send the user to Settings
    /// rather than pretending a password box would work.
    UnsupportedEnterprise,
}

/// Connection details for the active network.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Details {
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    /// The Wi-Fi adapter's hardware address, not the access point's.
    pub mac: Option<String>,
}

impl Details {
    pub fn is_empty(&self) -> bool {
        self.ipv4.is_none() && self.ipv6.is_none() && self.mac.is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// Wrong password, or the network rejected us.
    AuthFailed,
    /// NetworkManager wants an admin password to write the profile.
    NeedsAuthorisation,
    Timeout,
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub enabled: bool,
    /// rfkill has the radio off. `enabled = true` is a no-op until this clears,
    /// so the toggle must say so rather than appearing to do nothing.
    pub hardware_killed: bool,
    pub airplane_mode: bool,
    pub connected_ssid: Option<String>,
    pub networks: Vec<Network>,
    pub details: Details,

    /// Set while the Wi-Fi page is open, to scan faster.
    pub scanning: bool,
    /// SSID whose password field is showing.
    pub password_for: Option<String>,
    pub password_input: String,
    /// SSID currently being joined.
    pub connecting: Option<String>,
    pub last_error: Option<(String, Error)>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Snapshot(Box<Snapshot>),
    Unavailable,
    Connected(String),
    Failed(String, Error),
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub enabled: bool,
    pub hardware_killed: bool,
    pub airplane_mode: bool,
    pub connected_ssid: Option<String>,
    pub networks: Vec<Network>,
    pub details: Details,
}

impl State {
    /// One-line state for the tile: the network name, or why there isn't one.
    pub fn summary_key(&self) -> &'static str {
        if self.airplane_mode {
            "wifi-airplane"
        } else if self.hardware_killed {
            "wifi-hardware-off"
        } else if !self.enabled {
            "wifi-off"
        } else if self.connected_ssid.is_some() {
            // Caller substitutes the SSID itself.
            "wifi-connected"
        } else {
            "wifi-disconnected"
        }
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Snapshot(snapshot) => {
                let snapshot = *snapshot;
                self.availability = Availability::Available;
                self.enabled = snapshot.enabled;
                self.hardware_killed = snapshot.hardware_killed;
                self.airplane_mode = snapshot.airplane_mode;
                self.connected_ssid = snapshot.connected_ssid;
                self.details = snapshot.details;
                self.networks = snapshot.networks;
                // A scan that lands mid-join would otherwise clear the spinner
                // before the join actually resolves.
                if let Some(connecting) = &self.connecting {
                    if self.connected_ssid.as_ref() == Some(connecting) {
                        self.connecting = None;
                    }
                }
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.enabled = false;
                self.connected_ssid = None;
                self.networks.clear();
                self.details = Details::default();
            }
            Event::Connected(ssid) => {
                // Only clear the in-flight marker if this is the join we were
                // waiting on. Joins are fire-and-forget, so an older one can
                // land while a newer is running.
                if self.connecting.as_deref() == Some(ssid.as_str()) {
                    self.connecting = None;
                }
                self.clear_password_state_for(&ssid);
                self.connected_ssid = Some(ssid);
            }
            Event::Failed(ssid, error) => {
                if self.connecting.as_deref() == Some(ssid.as_str()) {
                    self.connecting = None;
                }
                // A wrong password keeps the field open so it can be corrected
                // without finding the network again. Anything else retyping
                // cannot fix, so the state is released.
                if !matches!(error, Error::AuthFailed) {
                    self.clear_password_state_for(&ssid);
                }
                // The error belongs to a network, and the UI only shows it on
                // pages about that network — see `wifi_error_text`.
                self.last_error = Some((ssid, error));
            }
        }
    }

    /// Begin joining `ssid`, or open the password field if one is needed.
    ///
    /// Returns `None` when the UI should just show the password box.
    pub fn select(&mut self, ssid: &str) -> Option<impl std::future::Future<Output = Event>> {
        let network = self.networks.iter().find(|n| n.ssid == ssid)?;
        match network.join_kind() {
            JoinKind::AlreadyConnected | JoinKind::UnsupportedEnterprise => None,
            JoinKind::NeedsPassword => {
                self.password_for = Some(ssid.to_string());
                self.password_input.clear();
                self.last_error = None;
                None
            }
            JoinKind::Direct => Some(self.begin_join(ssid.to_string(), None)),
        }
    }

    /// Join the network whose password field is open.
    pub fn submit_password(&mut self) -> Option<impl std::future::Future<Output = Event>> {
        let ssid = self.password_for.clone()?;
        // An empty password would be sent to NetworkManager and rejected several
        // seconds later; refusing here keeps the failure immediate and obvious.
        if self.password_input.is_empty() {
            return None;
        }
        // Refuse while a join for this network is already running. Pressing
        // Enter again would start a second NetworkManager activation for the
        // same SSID and reset the state the first one is reporting into.
        // Guarded here rather than in the view so every caller is covered.
        if self.connecting.as_deref() == Some(ssid.as_str()) {
            return None;
        }
        let password = self.password_input.clone();
        Some(self.begin_join(ssid, Some(password)))
    }

    fn begin_join(
        &mut self,
        ssid: String,
        password: Option<String>,
    ) -> impl std::future::Future<Output = Event> {
        self.connecting = Some(ssid.clone());
        self.last_error = None;
        async move {
            match join(&ssid, password).await {
                Ok(()) => Event::Connected(ssid),
                Err(error) => Event::Failed(ssid, error),
            }
        }
    }

    /// Release the password state, but only if it belongs to `ssid`.
    ///
    /// The guard is the point. Tapping a saved network starts a join with no
    /// page change, then tapping a secured one opens the password page; when
    /// the first join resolves, an unguarded clear would close the page and
    /// discard a password being typed for an entirely different network.
    fn clear_password_state_for(&mut self, ssid: &str) {
        if self.password_for.as_deref() != Some(ssid) {
            return;
        }
        self.password_for = None;
        self.password_input.clear();
        self.last_error = None;
    }

    pub fn cancel_password(&mut self) {
        self.password_for = None;
        self.password_input.clear();
        self.last_error = None;
    }

    pub fn toggle_radio(&mut self) -> impl std::future::Future<Output = ()> {
        self.enabled = !self.enabled;
        if !self.enabled {
            self.connected_ssid = None;
            self.networks.clear();
        }
        let enabled = self.enabled;
        async move {
            if let Err(err) = set_wireless(enabled).await {
                tracing::warn!("could not toggle Wi-Fi: {err}");
            }
        }
    }

    pub fn toggle_airplane_mode(&mut self) -> impl std::future::Future<Output = ()> {
        self.airplane_mode = !self.airplane_mode;
        // Airplane mode kills the Wi-Fi radio too; reflect that immediately
        // rather than leaving the Wi-Fi toggle showing "on" for a poll cycle.
        if self.airplane_mode {
            self.enabled = false;
            self.connected_ssid = None;
            self.networks.clear();
        }
        let enabled = self.airplane_mode;
        async move {
            if let Err(err) = set_airplane(enabled).await {
                tracing::warn!("could not set airplane mode: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        // Two ids so switching rate actually swaps the subscription: iced keys
        // on the data tuple, and the interval is part of it.
        if self.scanning {
            watch("wifi-scan", SCAN_INTERVAL, true)
        } else {
            watch("wifi-idle", IDLE_INTERVAL, false)
        }
    }
}

/// Poll NetworkManager on `interval`, reusing one connection.
///
/// Deliberately not built on [`super::poll_subscription`]: that reconnects per
/// sample, and `nmrs::NetworkManager::new` opens a bus connection and does
/// discovery. Holding one across the loop is the difference between a cheap
/// poll and an expensive one.
fn watch(id: &'static str, interval: Duration, scan: bool) -> Subscription<Event> {
    use futures::StreamExt;

    enum State {
        Disconnected { attempts: u32 },
        Connected(Box<nmrs::NetworkManager>),
    }

    Subscription::run_with((id, interval, scan), |&(_, interval, scan)| {
        futures::stream::unfold(
            State::Disconnected { attempts: 0 },
            move |state| async move {
                match state {
                    State::Disconnected { attempts } => {
                        if attempts > 0 {
                            tokio::time::sleep(Duration::from_secs(2u64.pow(attempts.min(4))))
                                .await;
                        }
                        match nmrs::NetworkManager::new().await {
                            Ok(manager) => Some((None, State::Connected(Box::new(manager)))),
                            Err(err) => {
                                tracing::debug!("NetworkManager unavailable: {err}");
                                Some((
                                    Some(Event::Unavailable),
                                    State::Disconnected {
                                        attempts: attempts.saturating_add(1),
                                    },
                                ))
                            }
                        }
                    }
                    State::Connected(manager) => {
                        if scan {
                            // NetworkManager only returns access points it
                            // already knows about. Without asking for a scan the
                            // list is whatever happened to be cached — typically
                            // just the network you are already on. Errors are
                            // ignored on purpose: NM refuses scans that come too
                            // soon after the last one, which is a rate limit,
                            // not a failure.
                            if let Err(err) = manager.scan_networks(None).await {
                                tracing::trace!("scan request declined: {err}");
                            }
                        }
                        let event = match snapshot(&manager).await {
                            Ok(snapshot) => Some(Event::Snapshot(Box::new(snapshot))),
                            Err(err) => {
                                tracing::debug!("network snapshot failed: {err}");
                                Some(Event::Unavailable)
                            }
                        };
                        tokio::time::sleep(interval).await;
                        Some((event, State::Connected(manager)))
                    }
                }
            },
        )
        .filter_map(|event| async move { event })
    })
}

/// Read everything the Wi-Fi module displays.
///
/// The error is a `String` rather than `nmrs::ConnectionError` on purpose: that
/// type is 136 bytes, which makes every `Result` carrying it large enough for
/// clippy to object, and no caller here does anything with it but log.
async fn snapshot(manager: &nmrs::NetworkManager) -> Result<Snapshot, String> {
    let radios = manager
        .airplane_mode_state()
        .await
        .map_err(|err| err.to_string())?;
    let wifi = radios.wifi;

    // No wireless hardware at all: report the module unavailable rather than
    // drawing a permanently-off toggle on a desktop.
    if !wifi.present {
        return Err("no wireless device on this machine".to_string());
    }

    let mut snapshot = Snapshot {
        enabled: wifi.enabled,
        hardware_killed: !wifi.hardware_enabled,
        airplane_mode: radios.is_airplane_mode(),
        ..Snapshot::default()
    };

    // Everything below needs the radio on. Asking for a network list with the
    // radio off returns stale entries NetworkManager has not yet forgotten,
    // which would show as joinable networks that are not there.
    if !wifi.enabled || snapshot.hardware_killed {
        return Ok(snapshot);
    }

    // Sorted so a machine with two adapters reports the same one every poll;
    // list order from NetworkManager is not guaranteed stable.
    if let Ok(Some(device)) = manager.list_wifi_devices().await.map(|mut devices| {
        devices.sort_by(|a, b| a.interface.cmp(&b.interface));
        devices.into_iter().next()
    }) {
        snapshot.details.mac = Some(device.hw_address);
    }

    let networks = manager.list_networks(None).await.unwrap_or_default();
    for network in &networks {
        if network.is_active {
            snapshot.connected_ssid = Some(network.ssid.clone());
            snapshot.details.ipv4 = network.ip4_address.clone();
            snapshot.details.ipv6 = network.ip6_address.clone();
        }
    }
    snapshot.networks = collapse(networks.iter().filter_map(to_row).collect());

    Ok(snapshot)
}

/// Map one NetworkManager entry to a row, or drop it.
///
/// `nmrs::Network` is `#[non_exhaustive]`, so this conversion is the only place
/// that touches it — which keeps [`collapse`] testable without a live bus.
fn to_row(network: &nmrs::Network) -> Option<Network> {
    // NetworkManager reports hidden APs under a placeholder SSID. There is
    // nothing useful to show and nothing joinable without typing the name, so
    // drop them rather than listing "<hidden>" repeatedly.
    if network.ssid.is_empty() || network.ssid == "<hidden>" {
        return None;
    }

    Some(Network {
        ssid: network.ssid.clone(),
        strength: network.strength.unwrap_or(0),
        secured: network.secured,
        enterprise: network.is_eap,
        known: network.known,
        connected: network.is_active,
    })
}

/// Reduce per-SSID entries to one row each, strongest first.
///
/// A mesh or repeater publishes the same SSID from several radios. Listing each
/// separately gives the user four identical rows and no way to tell them apart,
/// so keep the strongest and merge the flags.
fn collapse(networks: Vec<Network>) -> Vec<Network> {
    let mut rows: Vec<Network> = Vec::with_capacity(networks.len());

    for row in networks {
        match rows.iter_mut().find(|existing| existing.ssid == row.ssid) {
            Some(existing) => {
                existing.strength = existing.strength.max(row.strength);
                existing.known |= row.known;
                existing.connected |= row.connected;
            }
            None => rows.push(row),
        }
    }

    rows.sort_by(|a, b| {
        // Connected first, then known, then by strength. The network you are on
        // should never be somewhere down the list.
        b.connected
            .cmp(&a.connected)
            .then(b.known.cmp(&a.known))
            .then(b.strength.cmp(&a.strength))
            .then(a.ssid.cmp(&b.ssid))
    });
    rows
}

async fn join(ssid: &str, password: Option<String>) -> Result<(), Error> {
    let manager = nmrs::NetworkManager::new().await.map_err(classify)?;
    let security = match password {
        Some(psk) => nmrs::WifiSecurity::WpaPsk { psk },
        None => nmrs::WifiSecurity::Open,
    };
    manager
        .connect(ssid, None, security)
        .await
        .map_err(classify)
}

async fn set_wireless(enabled: bool) -> Result<(), String> {
    nmrs::NetworkManager::new()
        .await
        .map_err(|err| err.to_string())?
        .set_wireless_enabled(enabled)
        .await
        .map_err(|err| err.to_string())
}

async fn set_airplane(enabled: bool) -> Result<(), String> {
    nmrs::NetworkManager::new()
        .await
        .map_err(|err| err.to_string())?
        .set_airplane_mode(enabled)
        .await
        .map_err(|err| err.to_string())
}

fn classify(err: nmrs::ConnectionError) -> Error {
    match err {
        nmrs::ConnectionError::AuthFailed => Error::AuthFailed,
        nmrs::ConnectionError::Timeout | nmrs::ConnectionError::SupplicantTimeout => Error::Timeout,
        other => {
            let text = other.to_string();
            if text.contains("not authorized")
                || text.contains("NotAuthorized")
                || text.contains("AuthFailed")
            {
                Error::NeedsAuthorisation
            } else {
                Error::Other(text)
            }
        }
    }
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    let manager = nmrs::NetworkManager::new()
        .await
        .map_err(|err| format!("NetworkManager not reachable: {err}"))?;

    // Scan first, or the count reflects NetworkManager's cache rather than what
    // is actually in range — which makes this probe misleading in exactly the
    // situation it exists to diagnose.
    if manager.scan_networks(None).await.is_ok() {
        tokio::time::sleep(Duration::from_secs(3)).await;
    }

    let snapshot = snapshot(&manager).await.map_err(|err| err.to_string())?;

    Ok(format!(
        "radio {}{}{}, {} network(s) visible, connected to {}",
        if snapshot.enabled { "on" } else { "off" },
        if snapshot.hardware_killed {
            " (hardware killed)"
        } else {
            ""
        },
        if snapshot.airplane_mode {
            ", airplane mode"
        } else {
            ""
        },
        snapshot.networks.len(),
        snapshot.connected_ssid.as_deref().unwrap_or("nothing"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn network(ssid: &str, strength: u8) -> Network {
        Network {
            ssid: ssid.to_string(),
            strength,
            secured: true,
            enterprise: false,
            known: false,
            connected: false,
        }
    }

    /// A row as `to_row` would produce it.
    fn nm(ssid: &str, strength: u8) -> Network {
        network(ssid, strength)
    }

    #[test]
    fn a_saved_network_joins_without_asking_for_a_password() {
        let mut saved = network("HomeNet", 80);
        saved.known = true;
        assert_eq!(saved.join_kind(), JoinKind::Direct);
    }

    #[test]
    fn an_open_network_joins_without_asking_for_a_password() {
        let mut open = network("CafeWiFi", 60);
        open.secured = false;
        assert_eq!(open.join_kind(), JoinKind::Direct);
    }

    #[test]
    fn an_unknown_secured_network_asks_for_a_password() {
        assert_eq!(
            network("Neighbour", 40).join_kind(),
            JoinKind::NeedsPassword
        );
    }

    #[test]
    fn an_unknown_enterprise_network_is_not_offered_a_password_box() {
        // A password field would be a lie: enterprise needs an identity and
        // usually a certificate, which this applet does not collect.
        let mut eap = network("eduroam", 70);
        eap.enterprise = true;
        assert_eq!(eap.join_kind(), JoinKind::UnsupportedEnterprise);
    }

    #[test]
    fn a_known_enterprise_network_can_still_be_activated() {
        // The profile already exists, so NetworkManager has the credentials.
        let mut eap = network("eduroam", 70);
        eap.enterprise = true;
        eap.known = true;
        assert_eq!(eap.join_kind(), JoinKind::Direct);
    }

    #[test]
    fn duplicate_ssids_collapse_to_the_strongest() {
        let rows = collapse(vec![nm("Mesh", 40), nm("Mesh", 85), nm("Mesh", 20)]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].strength, 85);
    }

    #[test]
    fn collapsing_keeps_known_and_connected_from_any_duplicate() {
        // The weak entry carries the saved profile; losing that flag would make
        // a saved network ask for its password again.
        let mut weak = nm("Mesh", 20);
        weak.known = true;
        weak.connected = true;
        let rows = collapse(vec![nm("Mesh", 85), weak]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].known);
        assert!(rows[0].connected);
        assert_eq!(rows[0].strength, 85);
    }

    #[test]
    fn collapsing_preserves_distinct_networks() {
        let rows = collapse(vec![nm("A", 30), nm("B", 60), nm("C", 90)]);
        assert_eq!(rows.len(), 3);
        // Strongest first, since none are connected or known.
        let order: Vec<&str> = rows.iter().map(|r| r.ssid.as_str()).collect();
        assert_eq!(order, vec!["C", "B", "A"]);
    }

    #[test]
    fn the_connected_network_sorts_first_even_when_weak() {
        let mut connected = nm("Weak", 5);
        connected.connected = true;
        let rows = collapse(vec![nm("Strong", 99), connected]);
        assert_eq!(rows[0].ssid, "Weak");
    }

    #[test]
    fn a_bad_password_keeps_the_field_open() {
        // Otherwise the user has to find and re-select the network to retry.
        let mut state = State {
            password_for: Some("HomeNet".into()),
            ..State::default()
        };
        state.update(Event::Failed("HomeNet".into(), Error::AuthFailed));
        assert_eq!(state.password_for.as_deref(), Some("HomeNet"));
        assert!(state.connecting.is_none());
    }

    #[test]
    fn a_successful_join_closes_the_password_state() {
        // The connect page keys off `password_for`; leaving it set after a
        // successful join would strand the user on a page for a network they
        // are already on.
        let mut state = State {
            password_for: Some("HomeNet".into()),
            password_input: "hunter2".into(),
            ..State::default()
        };
        state.update(Event::Connected("HomeNet".into()));
        assert!(state.password_for.is_none());
        assert!(state.password_input.is_empty());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn another_networks_join_does_not_close_the_password_page() {
        // Tap a saved network, then a secured one while the first is still
        // connecting. When the first lands it must not wipe the password being
        // typed for the second.
        let mut state = State {
            password_for: Some("Neighbour".into()),
            password_input: "half-typed".into(),
            connecting: Some("HomeNet".into()),
            ..State::default()
        };

        state.update(Event::Connected("HomeNet".into()));

        assert_eq!(state.password_for.as_deref(), Some("Neighbour"));
        assert_eq!(state.password_input, "half-typed");
        assert!(state.connecting.is_none());
    }

    #[test]
    fn another_networks_failure_does_not_close_the_password_page() {
        // Same race on the failure path, and more likely: a timeout takes
        // tens of seconds, which is plenty of time to start typing elsewhere.
        let mut state = State {
            password_for: Some("Neighbour".into()),
            password_input: "half-typed".into(),
            connecting: Some("HomeNet".into()),
            ..State::default()
        };

        state.update(Event::Failed("HomeNet".into(), Error::Timeout));

        assert_eq!(state.password_for.as_deref(), Some("Neighbour"));
        assert_eq!(state.password_input, "half-typed");
    }

    #[test]
    fn submitting_twice_does_not_start_a_second_join() {
        // Pressing Enter again during a join would issue a duplicate
        // activation and reset the state the first is reporting into.
        let mut state = State {
            password_for: Some("HomeNet".into()),
            password_input: "hunter2".into(),
            ..State::default()
        };

        assert!(state.submit_password().is_some());
        assert_eq!(state.connecting.as_deref(), Some("HomeNet"));
        assert!(
            state.submit_password().is_none(),
            "a second submit must be refused while the first is in flight"
        );
    }

    #[test]
    fn cancelling_clears_the_password_state_and_any_error() {
        let mut state = State {
            password_for: Some("HomeNet".into()),
            password_input: "typo".into(),
            last_error: Some(("HomeNet".into(), Error::AuthFailed)),
            ..State::default()
        };
        state.cancel_password();
        assert!(state.password_for.is_none());
        assert!(state.password_input.is_empty());
        assert!(state.last_error.is_none());
    }

    #[test]
    fn a_non_password_failure_closes_the_field() {
        let mut state = State {
            password_for: Some("HomeNet".into()),
            ..State::default()
        };
        state.update(Event::Failed("HomeNet".into(), Error::Timeout));
        assert!(state.password_for.is_none());
    }

    #[test]
    fn an_empty_password_is_refused_locally() {
        let mut state = State {
            password_for: Some("HomeNet".into()),
            password_input: String::new(),
            ..State::default()
        };
        assert!(state.submit_password().is_none());
    }

    #[test]
    fn airplane_mode_turns_the_radio_off_immediately() {
        // The next poll is up to 15s away; leaving Wi-Fi showing "on" until
        // then looks like airplane mode did nothing.
        let mut state = State {
            enabled: true,
            connected_ssid: Some("HomeNet".into()),
            networks: vec![network("HomeNet", 80)],
            ..State::default()
        };

        let _write = state.toggle_airplane_mode();

        assert!(state.airplane_mode);
        assert!(!state.enabled);
        assert!(state.connected_ssid.is_none());
        assert!(state.networks.is_empty());
    }

    #[test]
    fn a_hardware_kill_is_reported_separately_from_being_switched_off() {
        // `enabled = true` is a no-op while rfkill holds the radio down, so the
        // UI must not present it as a working toggle.
        let mut state = State::default();
        state.update(Event::Snapshot(Box::new(Snapshot {
            enabled: true,
            hardware_killed: true,
            ..Snapshot::default()
        })));
        assert_eq!(state.summary_key(), "wifi-hardware-off");
    }

    #[test]
    fn summary_prefers_airplane_mode_over_everything() {
        let mut state = State::default();
        state.update(Event::Snapshot(Box::new(Snapshot {
            enabled: false,
            airplane_mode: true,
            hardware_killed: true,
            ..Snapshot::default()
        })));
        assert_eq!(state.summary_key(), "wifi-airplane");
    }

    #[test]
    fn a_scan_landing_mid_join_does_not_clear_the_spinner_early() {
        let mut state = State {
            connecting: Some("HomeNet".into()),
            ..State::default()
        };
        state.update(Event::Snapshot(Box::new(Snapshot {
            enabled: true,
            connected_ssid: Some("OldNet".into()),
            ..Snapshot::default()
        })));
        assert_eq!(state.connecting.as_deref(), Some("HomeNet"));

        state.update(Event::Snapshot(Box::new(Snapshot {
            enabled: true,
            connected_ssid: Some("HomeNet".into()),
            ..Snapshot::default()
        })));
        assert!(state.connecting.is_none());
    }
}
