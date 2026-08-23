//! Battery level and power profiles.
//!
//! Two independent backends behind one tile:
//!
//! * `org.freedesktop.UPower` supplies the charge percentage.
//! * `org.freedesktop.UPower.PowerProfiles` (power-profiles-daemon) supplies
//!   and sets the Power Saver / Balanced / Performance switch.
//!
//! **These are gated separately and must stay that way.** A desktop typically
//! runs power-profiles-daemon with no battery attached, so it should get the
//! profile switch and no percentage. Collapsing the two checks into one
//! "battery available?" flag loses the profile switch on every desktop — which
//! is exactly the audience the module toggle in `config.toml` exists for.
//!
//! Switching profiles needs no polkit prompt: `power-profiles-daemon.policy`
//! grants `switch-profile` to any active session.

use cosmic::iced::Subscription;
use futures::StreamExt;

use super::{clamp_percent, dbus_subscription, Availability};

#[zbus::proxy(
    interface = "org.freedesktop.UPower.PowerProfiles",
    default_service = "org.freedesktop.UPower.PowerProfiles",
    default_path = "/org/freedesktop/UPower/PowerProfiles"
)]
trait PowerProfiles {
    #[zbus(property)]
    fn active_profile(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_active_profile(&self, profile: &str) -> zbus::Result<()>;
    /// Non-empty when the daemon is throttling, e.g. "lap-detected".
    #[zbus(property)]
    fn performance_degraded(&self) -> zbus::Result<String>;
    /// Each entry has at least a `Profile` key naming the profile.
    #[zbus(property)]
    fn profiles(
        &self,
    ) -> zbus::Result<Vec<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower/devices/DisplayDevice"
)]
trait Device {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    /// UPower's `BatteryState`; 1 is charging, 5 is discharging.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
}

/// The three profiles, in the order they are shown.
///
/// power-profiles-daemon reports which of these it actually supports; the
/// `performance` profile in particular is absent on hardware without a platform
/// profile driver, so the UI must render whatever `Profiles` returned rather
/// than assuming all three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Profile {
    PowerSaver,
    Balanced,
    Performance,
}

impl Profile {
    pub const ORDER: [Profile; 3] = [Profile::PowerSaver, Profile::Balanced, Profile::Performance];

    pub fn as_dbus(self) -> &'static str {
        match self {
            Profile::PowerSaver => "power-saver",
            Profile::Balanced => "balanced",
            Profile::Performance => "performance",
        }
    }

    pub fn from_dbus(value: &str) -> Option<Self> {
        match value {
            "power-saver" => Some(Profile::PowerSaver),
            "balanced" => Some(Profile::Balanced),
            "performance" => Some(Profile::Performance),
            _ => None,
        }
    }

    /// Fluent key for the profile's display name.
    pub fn l10n_key(self) -> &'static str {
        match self {
            Profile::PowerSaver => "profile-power-saver",
            Profile::Balanced => "profile-balanced",
            Profile::Performance => "profile-performance",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct State {
    /// Whether a battery is present. Governs the percentage only.
    pub battery: Availability,
    /// Whether power-profiles-daemon is running. Governs the profile switch only.
    pub profiles: Availability,

    pub percent: Option<f64>,
    pub charging: bool,
    pub active_profile: Option<Profile>,
    pub supported_profiles: Vec<Profile>,
    /// Non-empty when the daemon is throttling, e.g. "lap-detected".
    pub performance_degraded: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Battery {
        present: bool,
        percent: f64,
        charging: bool,
    },
    /// power-profiles-daemon answered. An empty `supported` means it is running
    /// but offers nothing usable; a daemon that is not running produces no
    /// event at all, leaving the module `Unknown` and the switch hidden.
    Profiles {
        active: Option<Profile>,
        supported: Vec<Profile>,
        degraded: Option<String>,
    },
}

impl State {
    /// The tile is worth drawing if *either* half has something to show.
    pub fn is_shown(&self) -> bool {
        self.battery.is_shown() || self.profiles.is_shown()
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Battery {
                present,
                percent,
                charging,
            } => {
                self.battery = if present {
                    Availability::Available
                } else {
                    Availability::Unavailable
                };
                self.percent = present.then(|| clamp_percent(percent));
                self.charging = charging;
            }
            Event::Profiles {
                active,
                supported,
                degraded,
            } => {
                self.profiles = if supported.is_empty() {
                    Availability::Unavailable
                } else {
                    Availability::Available
                };
                self.active_profile = active;
                self.supported_profiles = supported;
                self.performance_degraded = degraded.filter(|reason| !reason.is_empty());
            }
        }
    }

    /// Select a profile, optimistically and then for real.
    pub fn set_profile(&mut self, profile: Profile) -> impl std::future::Future<Output = ()> {
        self.active_profile = Some(profile);
        async move {
            if let Err(err) = write_profile(profile).await {
                tracing::warn!("could not switch power profile: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        Subscription::batch([
            dbus_subscription("battery-upower", || async {
                let connection = zbus::Connection::system().await?;
                let proxy = DeviceProxy::new(&connection).await?;

                let initial = read_battery(&proxy).await?;
                // UPower emits a change for each of these separately, so merge
                // the three streams and re-read the whole set on any of them.
                // Reading all three keeps the tile self-consistent; reacting to
                // one property at a time can show 0% on a present battery.
                let changes = futures::stream::select_all([
                    proxy.receive_percentage_changed().await.map(|_| ()).boxed(),
                    proxy.receive_is_present_changed().await.map(|_| ()).boxed(),
                    proxy.receive_state_changed().await.map(|_| ()).boxed(),
                ]);

                let updates = changes.filter_map(move |()| {
                    let proxy = proxy.clone();
                    async move { read_battery(&proxy).await.ok() }
                });

                Ok(futures::stream::once(async move { initial })
                    .chain(updates)
                    .boxed())
            }),
            dbus_subscription("battery-profiles", || async {
                let connection = zbus::Connection::system().await?;
                let proxy = PowerProfilesProxy::new(&connection).await?;

                let initial = read_profiles(&proxy).await?;
                let changes = futures::stream::select_all([
                    proxy
                        .receive_active_profile_changed()
                        .await
                        .map(|_| ())
                        .boxed(),
                    proxy.receive_profiles_changed().await.map(|_| ()).boxed(),
                ]);

                let updates = changes.filter_map(move |()| {
                    let proxy = proxy.clone();
                    async move { read_profiles(&proxy).await.ok() }
                });

                Ok(futures::stream::once(async move { initial })
                    .chain(updates)
                    .boxed())
            }),
        ])
    }
}

async fn read_battery(proxy: &DeviceProxy<'_>) -> zbus::Result<Event> {
    // UPower's DisplayDevice always exists; `IsPresent` is what distinguishes a
    // laptop from a desktop, not the presence of the object.
    let present = proxy.is_present().await.unwrap_or(false);
    let percent = if present {
        proxy.percentage().await.unwrap_or(0.0)
    } else {
        0.0
    };
    // 1 = charging, 4 = fully charged. Anything else is treated as not charging.
    let charging = matches!(proxy.state().await.unwrap_or(0), 1 | 4);
    Ok(Event::Battery {
        present,
        percent,
        charging,
    })
}

async fn read_profiles(proxy: &PowerProfilesProxy<'_>) -> zbus::Result<Event> {
    let supported = proxy
        .profiles()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|entry| {
            let value = entry.get("Profile")?;
            let name: &str = value.downcast_ref().ok()?;
            Profile::from_dbus(name)
        })
        .collect::<Vec<_>>();

    // Preserve our display order rather than the daemon's enumeration order,
    // so the switch always reads saver -> balanced -> performance.
    let supported = Profile::ORDER
        .into_iter()
        .filter(|profile| supported.contains(profile))
        .collect();

    Ok(Event::Profiles {
        active: proxy
            .active_profile()
            .await
            .ok()
            .as_deref()
            .and_then(Profile::from_dbus),
        supported,
        degraded: proxy.performance_degraded().await.ok(),
    })
}

async fn write_profile(profile: Profile) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    PowerProfilesProxy::new(&connection)
        .await?
        .set_active_profile(profile.as_dbus())
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_round_trip() {
        for profile in Profile::ORDER {
            assert_eq!(Profile::from_dbus(profile.as_dbus()), Some(profile));
        }
    }

    #[test]
    fn unknown_profile_names_are_ignored() {
        // power-profiles-daemon could grow a fourth profile; an unrecognised
        // name must not be rendered as one of ours.
        assert_eq!(Profile::from_dbus("turbo"), None);
    }

    #[test]
    fn a_desktop_keeps_its_profile_switch() {
        // The regression this guards: gating the profile switch on battery
        // presence, which silently removes it from every desktop.
        let mut state = State::default();
        state.update(Event::Battery {
            present: false,
            percent: 0.0,
            charging: false,
        });
        state.update(Event::Profiles {
            active: Some(Profile::Balanced),
            supported: Profile::ORDER.to_vec(),
            degraded: None,
        });

        assert!(!state.battery.is_shown(), "no battery should be reported");
        assert!(state.profiles.is_shown(), "the profile switch must survive");
        assert!(state.is_shown(), "the tile must still be drawn");
        assert_eq!(state.percent, None);
    }

    #[test]
    fn a_laptop_without_the_daemon_keeps_its_percentage() {
        // The mirror image: no power-profiles-daemon, but a real battery.
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 80.0,
            charging: false,
        });
        state.update(Event::Profiles {
            active: None,
            supported: Vec::new(),
            degraded: None,
        });

        assert!(state.battery.is_shown());
        assert!(!state.profiles.is_shown());
        assert_eq!(state.percent, Some(80.0));
    }

    #[test]
    fn empty_degraded_reason_is_treated_as_not_degraded() {
        // The daemon reports "" rather than omitting the property.
        let mut state = State::default();
        state.update(Event::Profiles {
            active: Some(Profile::Performance),
            supported: Profile::ORDER.to_vec(),
            degraded: Some(String::new()),
        });
        assert_eq!(state.performance_degraded, None);
    }

    #[test]
    fn out_of_range_percentages_are_clamped() {
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 137.0,
            charging: true,
        });
        assert_eq!(state.percent, Some(100.0));
    }
}
