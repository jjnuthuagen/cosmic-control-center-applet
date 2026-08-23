//! Bluetooth adapter power and connected-device count.
//!
//! BlueZ has no single "bluetooth state" object: adapters and devices are
//! separate objects discovered through the standard `ObjectManager`. The tile
//! therefore reflects the *first* adapter, which on every machine with one
//! adapter is the only sensible reading, and counts devices reporting
//! `Connected = true` across all of them.
//!
//! Pairing and per-device connect live in `cosmic-settings`; the tile turns the
//! adapter on and off and says how many things are connected.

use cosmic::iced::Subscription;
use futures::StreamExt;
use std::collections::HashMap;

use super::{dbus_subscription, Availability};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

const ADAPTER_INTERFACE: &str = "org.bluez.Adapter1";
const DEVICE_INTERFACE: &str = "org.bluez.Device1";

#[zbus::proxy(
    interface = "org.freedesktop.DBus.ObjectManager",
    default_service = "org.bluez",
    default_path = "/"
)]
trait ObjectManager {
    fn get_managed_objects(&self) -> zbus::Result<ManagedObjects>;

    #[zbus(signal)]
    fn interfaces_added(
        &self,
        object: OwnedObjectPath,
        interfaces: HashMap<String, HashMap<String, OwnedValue>>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    fn interfaces_removed(
        &self,
        object: OwnedObjectPath,
        interfaces: Vec<String>,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
trait Adapter {
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_powered(&self, powered: bool) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub powered: bool,
    pub connected_devices: usize,
    adapter_path: Option<OwnedObjectPath>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed {
        powered: bool,
        connected_devices: usize,
        adapter_path: OwnedObjectPath,
    },
    /// No adapter, or `bluetoothd` is not running.
    Unavailable,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed {
                powered,
                connected_devices,
                adapter_path,
            } => {
                self.availability = Availability::Available;
                self.powered = powered;
                self.connected_devices = connected_devices;
                self.adapter_path = Some(adapter_path);
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.powered = false;
                self.connected_devices = 0;
                self.adapter_path = None;
            }
        }
    }

    pub fn toggle(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        let path = self.adapter_path.clone()?;
        self.powered = !self.powered;
        // Powering the adapter down disconnects everything; zero the count now
        // so the tile doesn't read "Off / 2 devices".
        if !self.powered {
            self.connected_devices = 0;
        }
        let powered = self.powered;
        Some(async move {
            if let Err(err) = write_powered(path, powered).await {
                tracing::warn!("could not toggle Bluetooth: {err}");
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        dbus_subscription("bluetooth", || async {
            let connection = zbus::Connection::system().await?;
            let manager = ObjectManagerProxy::new(&connection).await?;

            let initial = read(&manager).await;
            // Devices connecting and disconnecting show up as interfaces being
            // added and removed, so both signals feed the same re-read. Adapter
            // `Powered` changes arrive as PropertiesChanged on the adapter,
            // which the periodic re-read below also catches.
            let changes = futures::stream::select(
                manager.receive_interfaces_added().await?.map(|_| ()),
                manager.receive_interfaces_removed().await?.map(|_| ()),
            );

            let updates = changes.filter_map(move |()| {
                let manager = manager.clone();
                async move { Some(read(&manager).await) }
            });

            Ok(futures::stream::once(async move { initial })
                .chain(updates)
                .boxed())
        })
    }
}

async fn read(manager: &ObjectManagerProxy<'_>) -> Event {
    let Ok(objects) = manager.get_managed_objects().await else {
        return Event::Unavailable;
    };
    summarise(&objects).unwrap_or(Event::Unavailable)
}

/// Reduce BlueZ's object tree to the three numbers the tile needs.
///
/// Split out from the D-Bus call so it can be tested without a bus.
fn summarise(objects: &ManagedObjects) -> Option<Event> {
    // Sort for a stable choice of adapter: `get_managed_objects` returns a
    // HashMap, so iteration order would otherwise vary run to run and a
    // two-adapter machine would flip between them.
    let mut adapters: Vec<(&OwnedObjectPath, &HashMap<String, OwnedValue>)> = objects
        .iter()
        .filter_map(|(path, interfaces)| Some((path, interfaces.get(ADAPTER_INTERFACE)?)))
        .collect();
    adapters.sort_by_key(|(path, _)| path.as_str());

    let (adapter_path, adapter) = adapters.first()?;

    let powered = adapter
        .get("Powered")
        .and_then(|value| bool::try_from(value.clone()).ok())
        .unwrap_or(false);

    let connected_devices = objects
        .values()
        .filter_map(|interfaces| interfaces.get(DEVICE_INTERFACE))
        .filter(|device| {
            device
                .get("Connected")
                .and_then(|value| bool::try_from(value.clone()).ok())
                .unwrap_or(false)
        })
        .count();

    Some(Event::Changed {
        powered,
        connected_devices,
        adapter_path: (*adapter_path).clone(),
    })
}

async fn write_powered(path: OwnedObjectPath, powered: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    AdapterProxy::builder(&connection)
        .path(path)?
        .build()
        .await?
        .set_powered(powered)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(
        interface: &str,
        key: &str,
        value: bool,
    ) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut properties = HashMap::new();
        properties.insert(key.to_string(), OwnedValue::from(value));
        let mut interfaces = HashMap::new();
        interfaces.insert(interface.to_string(), properties);
        interfaces
    }

    fn path(text: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(text).unwrap()
    }

    #[test]
    fn no_adapter_means_unavailable() {
        // A machine with bluetoothd running but no radio: BlueZ answers, with
        // nothing in it.
        assert!(summarise(&ManagedObjects::new()).is_none());
    }

    #[test]
    fn counts_only_connected_devices() {
        let mut objects = ManagedObjects::new();
        objects.insert(
            path("/org/bluez/hci0"),
            object(ADAPTER_INTERFACE, "Powered", true),
        );
        objects.insert(
            path("/org/bluez/hci0/dev_AA"),
            object(DEVICE_INTERFACE, "Connected", true),
        );
        objects.insert(
            path("/org/bluez/hci0/dev_BB"),
            object(DEVICE_INTERFACE, "Connected", true),
        );
        // Paired but not connected — must not be counted.
        objects.insert(
            path("/org/bluez/hci0/dev_CC"),
            object(DEVICE_INTERFACE, "Connected", false),
        );

        let Some(Event::Changed {
            powered,
            connected_devices,
            ..
        }) = summarise(&objects)
        else {
            panic!("expected an adapter");
        };
        assert!(powered);
        assert_eq!(connected_devices, 2);
    }

    #[test]
    fn adapter_choice_is_stable_across_runs() {
        // HashMap iteration order varies; without the sort a two-adapter
        // machine would show a different adapter's state each time.
        let mut objects = ManagedObjects::new();
        objects.insert(
            path("/org/bluez/hci1"),
            object(ADAPTER_INTERFACE, "Powered", true),
        );
        objects.insert(
            path("/org/bluez/hci0"),
            object(ADAPTER_INTERFACE, "Powered", false),
        );

        for _ in 0..8 {
            let Some(Event::Changed { adapter_path, .. }) = summarise(&objects) else {
                panic!("expected an adapter");
            };
            assert_eq!(adapter_path.as_str(), "/org/bluez/hci0");
        }
    }

    #[test]
    fn powering_off_zeroes_the_device_count() {
        let mut state = State::default();
        state.update(Event::Changed {
            powered: true,
            connected_devices: 2,
            adapter_path: path("/org/bluez/hci0"),
        });
        let _ = state.toggle();
        assert!(!state.powered);
        assert_eq!(state.connected_devices, 0);
    }

    #[test]
    fn toggling_without_an_adapter_does_nothing() {
        // Guards against unwrapping a missing adapter path.
        let mut state = State::default();
        assert!(state.toggle().is_none());
    }
}
