//! DNS provider switching, via NetworkManager.
//!
//! # Why not systemd-resolved
//!
//! The obvious backend is `org.freedesktop.resolve1.Manager.SetLinkDNS`, and it
//! is the one the original design called for. It cannot be used unprivileged.
//! `/usr/share/polkit-1/actions/org.freedesktop.resolve1.policy` grants
//! `org.freedesktop.resolve1.set-dns-servers` as:
//!
//! ```text
//! <allow_active>auth_admin_keep</allow_active>
//! ```
//!
//! so every switch would raise an admin password prompt — from a panel applet,
//! for a two-click action. NetworkManager grants
//! `settings.modify.own` as plain `yes`, so editing a connection the user owns
//! needs no prompt at all, and NetworkManager pushes the result into resolved
//! anyway on systems running both.
//!
//! # The remaining caveat
//!
//! Connections created by the *system* rather than by the user fall under
//! `settings.modify.system`, which is `auth_admin_keep`. Switching DNS on such
//! a connection will still prompt once per session. We cannot detect this
//! reliably in advance, so the failure is reported rather than pre-empted — see
//! [`Error::NeedsAuthorisation`].
//!
//! Only IPv4 DNS is set. IPv6 DNS is left alone deliberately: overriding it
//! without also handling it would leave the two stacks pointing at different
//! resolvers, which is worse than not touching either.

use cosmic::iced::Subscription;
use futures::StreamExt;
use std::collections::HashMap;
use std::net::Ipv4Addr;

use super::{dbus_subscription, Availability};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<OwnedObjectPath>;

    fn activate_connection(
        &self,
        connection: &zbus::zvariant::ObjectPath<'_>,
        device: &zbus::zvariant::ObjectPath<'_>,
        specific_object: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<OwnedObjectPath>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
trait ActiveConnection {
    #[zbus(property)]
    fn connection(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
trait SettingsConnection {
    fn get_settings(&self) -> zbus::Result<Settings>;
    fn update(&self, settings: Settings) -> zbus::Result<()>;
}

/// A named set of upstream resolvers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// Fluent key for built-ins, or a literal name for user-defined entries.
    pub name: ProviderName,
    /// Empty means "hand DNS back to DHCP".
    pub servers: Vec<Ipv4Addr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderName {
    /// Looked up through Fluent.
    Builtin(&'static str),
    /// Supplied by the user in `config.toml` or typed into the drill-down.
    Custom(String),
}

impl Provider {
    /// DHCP-provided DNS. Always first, and always the way back out.
    pub fn automatic() -> Self {
        Self {
            name: ProviderName::Builtin("dns-automatic"),
            servers: Vec::new(),
        }
    }

    pub fn builtins() -> Vec<Self> {
        vec![
            Self::automatic(),
            Self {
                name: ProviderName::Builtin("dns-cloudflare"),
                servers: vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(1, 0, 0, 1)],
            },
            Self {
                name: ProviderName::Builtin("dns-google"),
                servers: vec![Ipv4Addr::new(8, 8, 8, 8), Ipv4Addr::new(8, 8, 4, 4)],
            },
            Self {
                name: ProviderName::Builtin("dns-quad9"),
                servers: vec![Ipv4Addr::new(9, 9, 9, 9), Ipv4Addr::new(149, 112, 112, 112)],
            },
        ]
    }

    pub fn is_automatic(&self) -> bool {
        self.servers.is_empty()
    }
}

#[derive(Debug, Clone)]
pub enum Error {
    /// The connection is system-owned; NetworkManager wants an admin password.
    NeedsAuthorisation,
    Other(String),
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// Providers offered in the drill-down: built-ins plus anything from config.
    pub providers: Vec<Provider>,
    /// Resolvers currently configured on the primary connection.
    pub current: Vec<Ipv4Addr>,
    /// Name of the primary connection, shown in the drill-down for context.
    pub connection_name: Option<String>,
    /// Text in the manual-entry field.
    pub manual_input: String,
    pub last_error: Option<Error>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Current {
        servers: Vec<Ipv4Addr>,
        connection_name: String,
    },
    Unavailable,
    /// A write succeeded; the confirming read arrives via the subscription.
    Applied,
    Failed(Error),
}

impl State {
    pub fn new(custom: &[Vec<String>]) -> Self {
        let mut providers = Provider::builtins();
        providers.extend(
            custom
                .iter()
                .map(Vec::as_slice)
                .filter_map(parse_custom_provider),
        );
        Self {
            providers,
            ..Self::default()
        }
    }

    /// Which provider the current resolvers correspond to, if any.
    ///
    /// Order-insensitive: NetworkManager may hand the list back in a different
    /// order than we set it, and a reordered list is still the same provider.
    pub fn active(&self) -> Option<&Provider> {
        self.providers.iter().find(|provider| {
            provider.servers.len() == self.current.len()
                && provider
                    .servers
                    .iter()
                    .all(|server| self.current.contains(server))
        })
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Current {
                servers,
                connection_name,
            } => {
                self.availability = Availability::Available;
                self.current = servers;
                self.connection_name = Some(connection_name);
                self.last_error = None;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.current.clear();
                self.connection_name = None;
            }
            Event::Applied => self.last_error = None,
            Event::Failed(error) => self.last_error = Some(error),
        }
    }

    /// Parse whatever is in the manual field into a provider, if it is valid.
    ///
    /// Accepts a comma- or space-separated list so pasting `1.1.1.1, 1.0.0.1`
    /// works as-is.
    pub fn manual_provider(&self) -> Option<Provider> {
        let servers = parse_servers(&self.manual_input);
        (!servers.is_empty()).then(|| Provider {
            name: ProviderName::Custom(self.manual_input.trim().to_string()),
            servers,
        })
    }

    pub fn apply(&mut self, provider: Provider) -> impl std::future::Future<Output = Event> {
        // Optimistic, like the sliders: the drill-down highlights the new
        // provider immediately and the next NetworkManager signal confirms it.
        self.current = provider.servers.clone();
        self.last_error = None;
        async move {
            match write(provider.servers).await {
                Ok(()) => Event::Applied,
                Err(err) => Event::Failed(classify(&err)),
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        dbus_subscription("dns", || async {
            let connection = zbus::Connection::system().await?;
            let manager = NetworkManagerProxy::new(&connection).await?;

            let initial = read_current(&connection, &manager).await;
            // The primary connection changing covers joining a different
            // network, which is the case where the displayed DNS goes stale.
            let changes = manager.receive_primary_connection_changed().await;

            let updates = changes.filter_map(move |_| {
                let connection = connection.clone();
                let manager = manager.clone();
                async move { Some(read_current(&connection, &manager).await) }
            });

            Ok(futures::stream::once(async move { initial })
                .chain(updates)
                .boxed())
        })
    }
}

async fn read_current(connection: &zbus::Connection, manager: &NetworkManagerProxy<'_>) -> Event {
    match read_current_inner(connection, manager).await {
        Ok(event) => event,
        Err(err) => {
            tracing::debug!("could not read DNS settings: {err}");
            Event::Unavailable
        }
    }
}

async fn read_current_inner(
    connection: &zbus::Connection,
    manager: &NetworkManagerProxy<'_>,
) -> zbus::Result<Event> {
    let active_path = manager.primary_connection().await?;
    // NetworkManager uses "/" to mean "no primary connection" rather than
    // omitting the property, and building a proxy on it fails confusingly.
    if active_path.as_str() == "/" {
        return Ok(Event::Unavailable);
    }

    let active = ActiveConnectionProxy::builder(connection)
        .path(active_path)?
        .build()
        .await?;
    let settings_path = active.connection().await?;
    let name = active.id().await.unwrap_or_default();

    let settings = SettingsConnectionProxy::builder(connection)
        .path(settings_path)?
        .build()
        .await?
        .get_settings()
        .await?;

    Ok(Event::Current {
        servers: extract_dns(&settings),
        connection_name: name,
    })
}

async fn write(servers: Vec<Ipv4Addr>) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    let manager = NetworkManagerProxy::new(&connection).await?;

    let active_path = manager.primary_connection().await?;
    if active_path.as_str() == "/" {
        return Err(zbus::Error::Failure("no active connection".into()));
    }

    let active = ActiveConnectionProxy::builder(&connection)
        .path(active_path)?
        .build()
        .await?;
    let settings_path = active.connection().await?;
    let devices = active.devices().await.unwrap_or_default();

    let settings_proxy = SettingsConnectionProxy::builder(&connection)
        .path(settings_path.clone())?
        .build()
        .await?;

    let mut settings = settings_proxy.get_settings().await?;
    apply_dns(&mut settings, &servers)?;
    settings_proxy.update(settings).await?;

    // `Update` only writes the profile to disk; the running connection keeps
    // the old resolvers until it is reactivated. Without this the user sees the
    // tile change and nothing else happen.
    if let Some(device) = devices.first() {
        manager
            .activate_connection(
                &settings_path.as_ref(),
                &device.as_ref(),
                &zbus::zvariant::ObjectPath::try_from("/").expect("`/` is a valid object path"),
            )
            .await?;
    }

    Ok(())
}

/// Rewrite the `ipv4` section's DNS keys in place.
fn apply_dns(settings: &mut Settings, servers: &[Ipv4Addr]) -> zbus::Result<()> {
    let ipv4 = settings.entry("ipv4".to_string()).or_default();

    // NetworkManager rejects an Update that echoes back these read-only keys.
    ipv4.remove("addresses");
    ipv4.remove("routes");

    let encoded: Vec<u32> = servers.iter().copied().map(encode_address).collect();
    ipv4.insert(
        "dns".to_string(),
        OwnedValue::try_from(Value::from(encoded))
            .map_err(|err| zbus::Error::Failure(err.to_string()))?,
    );
    // Without this NetworkManager appends ours to the DHCP-provided list rather
    // than replacing it, and the "switch" silently does nothing.
    ipv4.insert(
        "ignore-auto-dns".to_string(),
        OwnedValue::from(!servers.is_empty()),
    );

    Ok(())
}

fn extract_dns(settings: &Settings) -> Vec<Ipv4Addr> {
    let Some(ipv4) = settings.get("ipv4") else {
        return Vec::new();
    };
    let Some(value) = ipv4.get("dns") else {
        return Vec::new();
    };
    let Ok(raw) = <Vec<u32>>::try_from(value.clone()) else {
        return Vec::new();
    };
    raw.into_iter().map(decode_address).collect()
}

/// NetworkManager stores `ipv4.dns` entries as network-byte-order integers.
///
/// That means the octets sit in memory first-to-last and are then read as a
/// native `u32` — so this is `from_ne_bytes`, not `from_be_bytes`. Getting it
/// wrong byte-reverses every address, and `1.1.1.1` is a palindrome, so the
/// most obvious test address would not catch it.
fn encode_address(address: Ipv4Addr) -> u32 {
    u32::from_ne_bytes(address.octets())
}

fn decode_address(raw: u32) -> Ipv4Addr {
    Ipv4Addr::from(raw.to_ne_bytes())
}

fn parse_servers(input: &str) -> Vec<Ipv4Addr> {
    input
        .split([',', ' ', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect()
}

fn parse_custom_provider(entry: &[String]) -> Option<Provider> {
    let (name, servers) = entry.split_first()?;
    let servers: Vec<Ipv4Addr> = servers.iter().filter_map(|s| s.parse().ok()).collect();
    // A named entry with no parseable address would render as a provider that
    // silently does nothing when selected.
    (!servers.is_empty()).then(|| Provider {
        name: ProviderName::Custom(name.clone()),
        servers,
    })
}

fn classify(err: &zbus::Error) -> Error {
    let text = err.to_string();
    if text.contains("not authorized")
        || text.contains("NotAuthorized")
        || text.contains("AuthFailed")
    {
        Error::NeedsAuthorisation
    } else {
        Error::Other(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_round_trip() {
        // 8.8.4.4 is deliberately not a palindrome: it catches the
        // from_ne_bytes/from_be_bytes confusion that 1.1.1.1 would hide.
        for address in [
            Ipv4Addr::new(1, 1, 1, 1),
            Ipv4Addr::new(8, 8, 4, 4),
            Ipv4Addr::new(149, 112, 112, 112),
        ] {
            assert_eq!(decode_address(encode_address(address)), address);
        }
    }

    #[test]
    fn manual_entry_accepts_commas_and_spaces() {
        assert_eq!(
            parse_servers("1.1.1.1, 1.0.0.1"),
            vec![Ipv4Addr::new(1, 1, 1, 1), Ipv4Addr::new(1, 0, 0, 1)]
        );
        assert_eq!(
            parse_servers("9.9.9.9 149.112.112.112"),
            vec![Ipv4Addr::new(9, 9, 9, 9), Ipv4Addr::new(149, 112, 112, 112)]
        );
    }

    #[test]
    fn manual_entry_drops_nonsense() {
        assert!(parse_servers("not-an-address").is_empty());
        assert!(parse_servers("").is_empty());
        // A partly-valid list keeps the valid part rather than failing whole.
        assert_eq!(
            parse_servers("1.1.1.1, nope"),
            vec![Ipv4Addr::new(1, 1, 1, 1)]
        );
    }

    #[test]
    fn ipv6_only_manual_entry_is_rejected_rather_than_half_applied() {
        // Only IPv4 is written, so accepting an IPv6 address would produce a
        // provider that appears selected but changes nothing.
        assert!(parse_servers("2606:4700:4700::1111").is_empty());
    }

    #[test]
    fn automatic_clears_the_override() {
        let mut settings = Settings::new();
        apply_dns(&mut settings, &[]).unwrap();
        let ipv4 = &settings["ipv4"];
        assert_eq!(
            <Vec<u32>>::try_from(ipv4["dns"].clone()).unwrap(),
            Vec::<u32>::new()
        );
        // The important half: leaving this true would keep DHCP's resolvers
        // suppressed while offering none of our own.
        assert!(!bool::try_from(ipv4["ignore-auto-dns"].clone()).unwrap());
    }

    #[test]
    fn setting_a_provider_suppresses_dhcp_resolvers() {
        let mut settings = Settings::new();
        apply_dns(&mut settings, &[Ipv4Addr::new(9, 9, 9, 9)]).unwrap();
        assert!(bool::try_from(settings["ipv4"]["ignore-auto-dns"].clone()).unwrap());
    }

    #[test]
    fn read_only_keys_are_stripped_before_update() {
        let mut settings = Settings::new();
        let ipv4 = settings.entry("ipv4".to_string()).or_default();
        ipv4.insert("addresses".to_string(), OwnedValue::from(0u32));
        ipv4.insert("routes".to_string(), OwnedValue::from(0u32));

        apply_dns(&mut settings, &[Ipv4Addr::new(1, 1, 1, 1)]).unwrap();

        assert!(!settings["ipv4"].contains_key("addresses"));
        assert!(!settings["ipv4"].contains_key("routes"));
    }

    #[test]
    fn active_provider_ignores_server_order() {
        let mut state = State::new(&[]);
        state.current = vec![Ipv4Addr::new(1, 0, 0, 1), Ipv4Addr::new(1, 1, 1, 1)];
        assert_eq!(
            state.active().map(|p| p.name.clone()),
            Some(ProviderName::Builtin("dns-cloudflare"))
        );
    }

    #[test]
    fn no_override_reads_as_automatic() {
        let state = State::new(&[]);
        assert!(state.active().is_some_and(Provider::is_automatic));
    }

    #[test]
    fn custom_providers_need_a_usable_address() {
        let good = vec!["Home".to_string(), "192.168.1.1".to_string()];
        let bad = vec!["Broken".to_string(), "nonsense".to_string()];
        assert!(parse_custom_provider(&good).is_some());
        assert!(parse_custom_provider(&bad).is_none());
    }
}
