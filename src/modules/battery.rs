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
    /// Stop charging near a set level to reduce wear. Not all hardware can:
    /// `ChargeThresholdSettingsSupported` is a bitfield, and zero means no.
    fn enable_charge_threshold(&self, enable: bool) -> zbus::Result<()>;

    #[zbus(property)]
    fn charge_threshold_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn charge_threshold_settings_supported(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn is_present(&self) -> zbus::Result<bool>;
    /// UPower's `BatteryState`: 1 charging, 2 discharging, 3 empty,
    /// 4 fully charged, 5 pending charge, 6 pending discharge.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Seconds until flat. **Zero means unknown**, not "right now" — UPower
    /// reports 0 until it has enough discharge history to estimate.
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    /// Seconds until full, with the same zero-means-unknown rule.
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
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

    /// Whether this hardware can limit its charge at all.
    pub charge_threshold_supported: bool,
    pub charge_threshold_enabled: bool,

    /// Seconds until flat or until full, whichever applies. `None` when UPower
    /// has no estimate yet.
    pub time_remaining: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Battery {
        present: bool,
        percent: f64,
        charging: bool,
        charge_threshold_supported: bool,
        charge_threshold_enabled: bool,
        time_remaining: Option<i64>,
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
                charge_threshold_supported,
                charge_threshold_enabled,
                time_remaining,
            } => {
                self.charge_threshold_supported = charge_threshold_supported;
                self.charge_threshold_enabled = charge_threshold_enabled;
                self.time_remaining = time_remaining;
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

    /// Turn the charge limit on or off.
    pub fn toggle_charge_threshold(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        if !self.charge_threshold_supported {
            return None;
        }
        self.charge_threshold_enabled = !self.charge_threshold_enabled;
        let enable = self.charge_threshold_enabled;
        Some(async move {
            if let Err(err) = write_charge_threshold(enable).await {
                tracing::warn!("could not change the charge threshold: {err}");
            }
        })
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
    let state = proxy.state().await.unwrap_or(0);
    let charging = matches!(state, 1 | 4);

    // Which estimate applies depends on direction, and UPower reports 0 for
    // "no estimate yet" rather than omitting it — showing that verbatim would
    // read as "0 minutes left", which is alarming and wrong.
    let seconds = if charging {
        proxy.time_to_full().await.unwrap_or(0)
    } else {
        proxy.time_to_empty().await.unwrap_or(0)
    };
    let time_remaining = (present && seconds > 0).then_some(seconds);
    Ok(Event::Battery {
        present,
        percent,
        charging,
        // A zero bitfield means the hardware cannot do this, which is the
        // common case — most laptops have no charge limit at all.
        charge_threshold_supported: proxy
            .charge_threshold_settings_supported()
            .await
            .unwrap_or(0)
            != 0,
        charge_threshold_enabled: proxy.charge_threshold_enabled().await.unwrap_or(false),
        time_remaining,
    })
}

async fn write_charge_threshold(enable: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    DeviceProxy::new(&connection)
        .await?
        .enable_charge_threshold(enable)
        .await
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

/// One-shot read for `--check`.
///
/// Reports the two halves separately, because they genuinely are: a desktop has
/// power profiles and no battery.
pub async fn probe() -> Result<String, String> {
    let connection = zbus::Connection::system()
        .await
        .map_err(|err| format!("no system bus: {err}"))?;

    let battery = match DeviceProxy::new(&connection).await {
        Ok(proxy) => match read_battery(&proxy).await {
            Ok(Event::Battery {
                present: true,
                percent,
                charging,
                charge_threshold_supported,
                time_remaining,
                ..
            }) => format!(
                "{percent:.0}%{}{}{}",
                if charging { " charging" } else { "" },
                match time_remaining {
                    Some(seconds) => format!(
                        ", {} {}",
                        format_duration(seconds),
                        if charging { "until full" } else { "remaining" }
                    ),
                    // Expected on mains, or before UPower has enough history.
                    None => ", no time estimate yet".to_string(),
                },
                if charge_threshold_supported {
                    ", charge limit supported"
                } else {
                    ""
                }
            ),
            Ok(_) => "no battery present".to_string(),
            Err(err) => format!("unreadable ({err})"),
        },
        Err(err) => format!("UPower unreachable ({err})"),
    };

    let profiles = match PowerProfilesProxy::new(&connection).await {
        Ok(proxy) => match read_profiles(&proxy).await {
            Ok(Event::Profiles {
                active, supported, ..
            }) => format!(
                "{} of [{}]",
                active.map_or("none", Profile::as_dbus),
                supported
                    .iter()
                    .map(|p| p.as_dbus())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            _ => "unreadable".to_string(),
        },
        Err(_) => "power-profiles-daemon not running".to_string(),
    };

    Ok(format!("battery: {battery}; profiles: {profiles}"))
}

/// Render a duration as "2h 15m", or just "45m" under an hour.
///
/// Rounds to the minute: a battery estimate accurate to the second would be
/// false precision, and a ticking seconds count in a popup is a distraction.
pub fn format_duration(seconds: i64) -> String {
    let minutes = (seconds / 60).max(1);
    let hours = minutes / 60;
    let minutes = minutes % 60;

    if hours > 0 {
        format!("{hours}h {minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_read_as_hours_and_minutes() {
        assert_eq!(format_duration(60 * 135), "2h 15m");
        assert_eq!(format_duration(60 * 45), "45m");
        assert_eq!(format_duration(60 * 60), "1h 00m");
    }

    #[test]
    fn a_sub_minute_estimate_does_not_render_as_zero() {
        // "0m" reads as "it is about to die"; anything under a minute is 1m.
        assert_eq!(format_duration(30), "1m");
        assert_eq!(format_duration(0), "1m");
    }

    #[test]
    fn no_estimate_is_absent_rather_than_zero() {
        // UPower reports 0 until it has discharge history. Showing that
        // verbatim would say "0 minutes remaining" on a full battery.
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 100.0,
            charging: false,
            charge_threshold_supported: false,
            charge_threshold_enabled: false,
            time_remaining: None,
        });
        assert_eq!(state.time_remaining, None);
    }

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
            charge_threshold_supported: false,
            charge_threshold_enabled: false,
            time_remaining: None,
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
            charge_threshold_supported: false,
            charge_threshold_enabled: false,
            time_remaining: None,
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
    fn unsupported_hardware_offers_no_charge_limit() {
        // Most laptops cannot do this; offering a switch that silently fails
        // would be worse than not offering one.
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 80.0,
            charging: false,
            charge_threshold_supported: false,
            charge_threshold_enabled: false,
            time_remaining: None,
        });
        assert!(!state.charge_threshold_supported);
        assert!(state.toggle_charge_threshold().is_none());
    }

    #[test]
    fn supported_hardware_can_flip_the_charge_limit() {
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 80.0,
            charging: false,
            charge_threshold_supported: true,
            charge_threshold_enabled: false,
            time_remaining: None,
        });
        assert!(state.toggle_charge_threshold().is_some());
        assert!(state.charge_threshold_enabled);
    }

    #[test]
    fn out_of_range_percentages_are_clamped() {
        let mut state = State::default();
        state.update(Event::Battery {
            present: true,
            percent: 137.0,
            charging: true,
            charge_threshold_supported: false,
            charge_threshold_enabled: false,
            time_remaining: None,
        });
        assert_eq!(state.percent, Some(100.0));
    }
}
