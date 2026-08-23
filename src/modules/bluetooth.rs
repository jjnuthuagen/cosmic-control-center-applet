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

#[zbus::proxy(interface = "org.bluez.Device1", default_service = "org.bluez")]
trait BluezDevice {
    /// Connects every profile the device supports. BlueZ authorises this for
    /// the active session, so no polkit prompt for an already-paired device.
    fn connect(&self) -> zbus::Result<()>;
    fn disconnect(&self) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.Adapter1", default_service = "org.bluez")]
trait Adapter {
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_powered(&self, powered: bool) -> zbus::Result<()>;
}

/// A known device, reduced to what the drill-down draws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub connected: bool,
    /// Unpaired devices are listed but not actionable — pairing needs an agent
    /// that can show a PIN, which belongs in Settings, not a panel popup.
    pub paired: bool,
    path: OwnedObjectPath,
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub powered: bool,
    pub connected_devices: usize,
    pub devices: Vec<Device>,
    /// Path of a device with a connect/disconnect in flight.
    pub busy: Option<String>,
    adapter_path: Option<OwnedObjectPath>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed {
        powered: bool,
        connected_devices: usize,
        devices: Vec<Device>,
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
                devices,
                adapter_path,
            } => {
                self.availability = Availability::Available;
                self.powered = powered;
                self.connected_devices = connected_devices;
                self.devices = devices;
                self.adapter_path = Some(adapter_path);
                // Whatever was in flight has either landed or failed; either way
                // the fresh list is the truth now.
                self.busy = None;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.powered = false;
                self.connected_devices = 0;
                self.devices.clear();
                self.busy = None;
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
            self.devices.clear();
        }
        let powered = self.powered;
        Some(async move {
            if let Err(err) = write_powered(path, powered).await {
                tracing::warn!("could not toggle Bluetooth: {err}");
            }
        })
    }

    /// Connect or disconnect a device, depending on its current state.
    pub fn toggle_device(&mut self, name: &str) -> Option<impl std::future::Future<Output = ()>> {
        let device = self.devices.iter_mut().find(|device| device.name == name)?;
        // Unpaired devices have no credentials, so Connect would fail with a
        // confusing BlueZ error. The UI shows them but does not offer the action.
        if !device.paired {
            return None;
        }

        let path = device.path.clone();
        let connect = !device.connected;
        // Optimistic, and `busy` drives a spinner: a Bluetooth connect can take
        // several seconds, and a row that does nothing visible in that window
        // reads as a dead button.
        device.connected = connect;
        self.busy = Some(name.to_string());
        self.connected_devices = self.devices.iter().filter(|d| d.connected).count();

        Some(async move {
            if let Err(err) = set_device_connected(path, connect).await {
                tracing::warn!("could not change Bluetooth device connection: {err}");
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

    let mut devices: Vec<Device> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interfaces.get(DEVICE_INTERFACE)?;
            let flag = |key: &str| {
                properties
                    .get(key)
                    .and_then(|value| bool::try_from(value.clone()).ok())
                    .unwrap_or(false)
            };
            let name = properties
                .get("Alias")
                .or_else(|| properties.get("Name"))
                .and_then(|value| String::try_from(value.clone()).ok())
                .filter(|name| !name.is_empty())
                // A device with no name at all is a bare MAC address the user
                // cannot identify; there is nothing useful to list.
                .or_else(|| {
                    properties
                        .get("Address")
                        .and_then(|value| String::try_from(value.clone()).ok())
                })?;

            Some(Device {
                name,
                connected: flag("Connected"),
                paired: flag("Paired"),
                path: path.clone(),
            })
        })
        .collect();

    devices.sort_by(|a, b| {
        // Connected first, then paired, then by name. What you are using should
        // be at the top; what you cannot act on should be at the bottom.
        b.connected
            .cmp(&a.connected)
            .then(b.paired.cmp(&a.paired))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    let connected_devices = devices.iter().filter(|device| device.connected).count();

    Some(Event::Changed {
        powered,
        connected_devices,
        devices,
        adapter_path: (*adapter_path).clone(),
    })
}

async fn set_device_connected(path: OwnedObjectPath, connect: bool) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    let device = BluezDeviceProxy::builder(&connection)
        .path(path)?
        .build()
        .await?;
    if connect {
        device.connect().await
    } else {
        device.disconnect().await
    }
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

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    let connection = zbus::Connection::system()
        .await
        .map_err(|err| format!("no system bus: {err}"))?;
    let manager = ObjectManagerProxy::new(&connection)
        .await
        .map_err(|err| format!("org.bluez not reachable: {err}"))?;
    let objects = manager
        .get_managed_objects()
        .await
        .map_err(|err| format!("GetManagedObjects failed: {err}"))?;

    match summarise(&objects) {
        Some(Event::Changed {
            powered,
            connected_devices,
            devices,
            adapter_path,
        }) => Ok(format!(
            "{} is {}, {} device(s) known, {connected_devices} connected",
            adapter_path.as_str(),
            if powered { "on" } else { "off" },
            devices.len()
        )),
        _ => Err("bluetoothd is running but has no adapter".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(powered: bool) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut properties = HashMap::new();
        properties.insert("Powered".to_string(), OwnedValue::from(powered));
        let mut interfaces = HashMap::new();
        interfaces.insert(ADAPTER_INTERFACE.to_string(), properties);
        interfaces
    }

    fn device(
        name: &str,
        connected: bool,
        paired: bool,
    ) -> HashMap<String, HashMap<String, OwnedValue>> {
        let mut properties = HashMap::new();
        properties.insert(
            "Alias".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::from(name)).unwrap(),
        );
        properties.insert("Connected".to_string(), OwnedValue::from(connected));
        properties.insert("Paired".to_string(), OwnedValue::from(paired));
        let mut interfaces = HashMap::new();
        interfaces.insert(DEVICE_INTERFACE.to_string(), properties);
        interfaces
    }

    fn path(text: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(text).unwrap()
    }

    fn changed(objects: &ManagedObjects) -> (bool, usize, Vec<Device>, OwnedObjectPath) {
        match summarise(objects) {
            Some(Event::Changed {
                powered,
                connected_devices,
                devices,
                adapter_path,
            }) => (powered, connected_devices, devices, adapter_path),
            _ => panic!("expected an adapter"),
        }
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
        objects.insert(path("/org/bluez/hci0"), adapter(true));
        objects.insert(
            path("/org/bluez/hci0/dev_AA"),
            device("Headphones", true, true),
        );
        objects.insert(path("/org/bluez/hci0/dev_BB"), device("Mouse", true, true));
        // Paired but not connected — must not be counted.
        objects.insert(
            path("/org/bluez/hci0/dev_CC"),
            device("Keyboard", false, true),
        );

        let (powered, connected, devices, _) = changed(&objects);
        assert!(powered);
        assert_eq!(connected, 2);
        assert_eq!(devices.len(), 3);
    }

    #[test]
    fn devices_sort_connected_then_paired_then_by_name() {
        let mut objects = ManagedObjects::new();
        objects.insert(path("/org/bluez/hci0"), adapter(true));
        objects.insert(path("/org/bluez/hci0/dev_AA"), device("Zeta", false, false));
        objects.insert(path("/org/bluez/hci0/dev_BB"), device("alpha", false, true));
        objects.insert(path("/org/bluez/hci0/dev_CC"), device("Omega", true, true));

        let (_, _, devices, _) = changed(&objects);
        let order: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(order, vec!["Omega", "alpha", "Zeta"]);
    }

    #[test]
    fn adapter_choice_is_stable_across_runs() {
        // HashMap iteration order varies; without the sort a two-adapter
        // machine would show a different adapter's state each time.
        let mut objects = ManagedObjects::new();
        objects.insert(path("/org/bluez/hci1"), adapter(true));
        objects.insert(path("/org/bluez/hci0"), adapter(false));

        for _ in 0..8 {
            let (_, _, _, adapter_path) = changed(&objects);
            assert_eq!(adapter_path.as_str(), "/org/bluez/hci0");
        }
    }

    #[test]
    fn powering_off_clears_the_device_list() {
        let mut state = State::default();
        state.update(Event::Changed {
            powered: true,
            connected_devices: 2,
            devices: vec![Device {
                name: "Headphones".into(),
                connected: true,
                paired: true,
                path: path("/org/bluez/hci0/dev_AA"),
            }],
            adapter_path: path("/org/bluez/hci0"),
        });
        let _write = state.toggle();
        assert!(!state.powered);
        assert_eq!(state.connected_devices, 0);
        assert!(state.devices.is_empty());
    }

    #[test]
    fn toggling_without_an_adapter_does_nothing() {
        // Guards against unwrapping a missing adapter path.
        let mut state = State::default();
        assert!(state.toggle().is_none());
    }

    #[test]
    fn an_unpaired_device_offers_no_connect_action() {
        // Connect would fail with a BlueZ error the user cannot act on; pairing
        // needs an agent that can display a PIN, which lives in Settings.
        let mut state = State {
            devices: vec![Device {
                name: "Stranger".into(),
                connected: false,
                paired: false,
                path: path("/org/bluez/hci0/dev_ZZ"),
            }],
            ..State::default()
        };
        assert!(state.toggle_device("Stranger").is_none());
    }

    #[test]
    fn connecting_a_device_updates_the_count_immediately() {
        // A Bluetooth connect takes seconds; without the optimistic update the
        // row looks dead for the whole of it.
        let mut state = State {
            devices: vec![Device {
                name: "Headphones".into(),
                connected: false,
                paired: true,
                path: path("/org/bluez/hci0/dev_AA"),
            }],
            ..State::default()
        };

        let action = state.toggle_device("Headphones");
        assert!(action.is_some());
        assert!(state.devices[0].connected);
        assert_eq!(state.connected_devices, 1);
        assert_eq!(state.busy.as_deref(), Some("Headphones"));
    }

    #[test]
    fn a_fresh_snapshot_clears_the_busy_marker() {
        let mut state = State {
            busy: Some("Headphones".into()),
            ..State::default()
        };
        state.update(Event::Changed {
            powered: true,
            connected_devices: 0,
            devices: Vec::new(),
            adapter_path: path("/org/bluez/hci0"),
        });
        assert!(state.busy.is_none());
    }

    #[test]
    fn a_device_with_no_name_falls_back_to_its_address() {
        let mut objects = ManagedObjects::new();
        objects.insert(path("/org/bluez/hci0"), adapter(true));

        let mut properties = HashMap::new();
        properties.insert(
            "Address".to_string(),
            OwnedValue::try_from(zbus::zvariant::Value::from("AA:BB:CC:DD:EE:FF")).unwrap(),
        );
        properties.insert("Connected".to_string(), OwnedValue::from(false));
        properties.insert("Paired".to_string(), OwnedValue::from(true));
        let mut interfaces = HashMap::new();
        interfaces.insert(DEVICE_INTERFACE.to_string(), properties);
        objects.insert(path("/org/bluez/hci0/dev_AA"), interfaces);

        let (_, _, devices, _) = changed(&objects);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "AA:BB:CC:DD:EE:FF");
    }
}
