//! Wi-Fi state and the radio toggle.
//!
//! `org.freedesktop.NetworkManager.enable-disable-wifi` is granted as `yes` to
//! active sessions, so toggling the radio needs no polkit prompt — unlike
//! editing DNS on a system-owned connection. See [`crate::modules::dns`].
//!
//! Scanning and joining networks are not here. The tile reports what is
//! connected and turns the radio on and off; picking a different network stays
//! in `cosmic-settings`, where the password prompts already live.

use cosmic::iced::Subscription;
use futures::StreamExt;

use super::{dbus_subscription, Availability};
use zbus::zvariant::OwnedObjectPath;

/// `NM_DEVICE_TYPE_WIFI`.
const DEVICE_TYPE_WIFI: u32 = 2;

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_wireless_enabled(&self, enabled: bool) -> zbus::Result<()>;
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;

    fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NmDevice {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn active_connection(&self) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub enabled: bool,
    /// Name of the connected network, or `None` when the radio is on but idle.
    pub ssid: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed {
        enabled: bool,
        ssid: Option<String>,
    },
    /// No Wi-Fi device, or NetworkManager is not running.
    Unavailable,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed { enabled, ssid } => {
                self.availability = Availability::Available;
                self.enabled = enabled;
                self.ssid = ssid;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.enabled = false;
                self.ssid = None;
            }
        }
    }

    pub fn toggle(&mut self) -> impl std::future::Future<Output = ()> {
        self.enabled = !self.enabled;
        // Turning the radio off drops the connection; clear the SSID now rather
        // than leaving a stale network name under an "off" toggle until the
        // next signal arrives.
        if !self.enabled {
            self.ssid = None;
        }
        let enabled = self.enabled;
        async move {
            if let Err(err) = write_enabled(enabled).await {
                tracing::warn!("could not toggle Wi-Fi: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        dbus_subscription("wifi", || async {
            let connection = zbus::Connection::system().await?;
            let manager = NetworkManagerProxy::new(&connection).await?;

            let initial = read(&connection, &manager).await;
            let changes = futures::stream::select_all([
                manager
                    .receive_wireless_enabled_changed()
                    .await
                    .map(|_| ())
                    .boxed(),
                manager
                    .receive_primary_connection_changed()
                    .await
                    .map(|_| ())
                    .boxed(),
            ]);

            let updates = changes.filter_map(move |()| {
                let connection = connection.clone();
                let manager = manager.clone();
                async move { Some(read(&connection, &manager).await) }
            });

            Ok(futures::stream::once(async move { initial })
                .chain(updates)
                .boxed())
        })
    }
}

async fn read(connection: &zbus::Connection, manager: &NetworkManagerProxy<'_>) -> Event {
    match read_inner(connection, manager).await {
        Ok(event) => event,
        Err(err) => {
            tracing::debug!("could not read Wi-Fi state: {err}");
            Event::Unavailable
        }
    }
}

async fn read_inner(
    connection: &zbus::Connection,
    manager: &NetworkManagerProxy<'_>,
) -> zbus::Result<Event> {
    let Some(device) = first_wifi_device(connection, manager).await? else {
        // A desktop with no wireless card. Not an error, just nothing to show.
        return Ok(Event::Unavailable);
    };

    let enabled = manager.wireless_enabled().await.unwrap_or(false);
    let ssid = if enabled {
        active_connection_name(connection, &device).await
    } else {
        None
    };

    Ok(Event::Changed { enabled, ssid })
}

async fn first_wifi_device(
    connection: &zbus::Connection,
    manager: &NetworkManagerProxy<'_>,
) -> zbus::Result<Option<NmDeviceProxy<'static>>> {
    for path in manager.get_devices().await? {
        let device = NmDeviceProxy::builder(connection)
            .path(path)?
            .build()
            .await?;
        if device.device_type().await.unwrap_or(0) == DEVICE_TYPE_WIFI {
            return Ok(Some(device));
        }
    }
    Ok(None)
}

async fn active_connection_name(
    connection: &zbus::Connection,
    device: &NmDeviceProxy<'_>,
) -> Option<String> {
    let path = device.active_connection().await.ok()?;
    // "/" is NetworkManager's way of saying "nothing active" — the radio is on
    // but not associated with any network.
    if path.as_str() == "/" {
        return None;
    }
    let active = ActiveConnectionProxy::builder(connection)
        .path(path)
        .ok()?
        .build()
        .await
        .ok()?;
    let id = active.id().await.ok()?;
    (!id.is_empty()).then_some(id)
}

async fn write_enabled(enabled: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    NetworkManagerProxy::new(&connection)
        .await?
        .set_wireless_enabled(enabled)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turning_the_radio_off_clears_the_network_name() {
        // Otherwise the tile reads "Off / HomeNet", which looks like a bug.
        let mut state = State::default();
        state.update(Event::Changed {
            enabled: true,
            ssid: Some("HomeNet".into()),
        });
        // The optimistic state change happens before the future is awaited,
        // which is exactly what this test is checking; the future itself is the
        // backend write and is not needed here.
        let _write = state.toggle();
        assert!(!state.enabled);
        assert_eq!(state.ssid, None);
    }

    #[test]
    fn no_wifi_hardware_hides_the_tile() {
        let mut state = State::default();
        state.update(Event::Unavailable);
        assert!(!state.availability.is_shown());
    }

    #[test]
    fn an_unknown_state_is_not_shown_yet() {
        // Guards the first-second flicker: a tile must not appear before its
        // backend has answered.
        assert!(!State::default().availability.is_shown());
    }
}
