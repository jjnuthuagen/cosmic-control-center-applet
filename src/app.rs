//! The applet itself: panel button, popup, and routing between the tile grid
//! and its drill-down pages.
//!
//! Drill-downs replace the popup's content rather than opening a second window.
//! A panel popup is a layer-shell surface anchored to the button; spawning
//! another for a sub-page would put it somewhere unpredictable and leave two
//! surfaces to dismiss.
//!
//! Every tile behaves the same way: pressing it opens that thing's page, where
//! the on/off switch lives alongside whatever else it can do. The two quick
//! toggles (dark mode, tiling) are the exception — they have nothing to drill
//! into, so they flip in place.

use cosmic::app::{Core, Task};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::widget::{button, column, container, divider, row, text, text_input};
use cosmic::{Application, Element};

use crate::config::Config;
use crate::fl;
use crate::modules::{battery, bluetooth, brightness, dns, network, system, volume};
use crate::ui::{list_row, page_header, scrollable_page, slider_row, tile_grid, Spacing, Tile};

/// Wide enough for two tiles plus their state text, narrow enough not to
/// dominate the screen. Public because `ui` sizes its text elision against it.
pub const POPUP_WIDTH: f32 = 360.0;
const POPUP_MAX_HEIGHT: f32 = 720.0;

/// Cap on the network and device lists.
///
/// A busy flat can see thirty access points. Past a dozen the list is scrolling
/// anyway and the ones that matter are at the top, since both lists sort
/// connected-and-known first.
const MAX_LIST_ROWS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Root,
    Wifi,
    Bluetooth,
    Battery,
    Dns,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Navigate(Page),

    WifiToggleRadio,
    WifiToggleAirplane,
    WifiSelect(String),
    WifiPasswordInput(String),
    WifiSubmitPassword,
    WifiCancelPassword,

    BluetoothTogglePower,
    BluetoothToggleDevice(String),

    ToggleDark,
    ToggleTiling,

    SetVolume(f64),
    ToggleMute,
    SetBrightness(f64),

    SetProfile(battery::Profile),
    SelectDnsProvider(dns::Provider),
    DnsManualInput(String),
    ApplyDnsManual,

    Wifi(network::Event),
    Bluetooth(bluetooth::Event),
    Battery(battery::Event),
    Dns(dns::Event),
    Volume(volume::Event),
    Brightness(brightness::Event),
    System(system::Event),

    /// A backend write finished. State was updated optimistically, so there is
    /// nothing to do — but the task has to resolve into something.
    Done,
}

pub struct App {
    core: Core,
    config: Config,
    popup: Option<Id>,
    page: Page,

    wifi: network::State,
    bluetooth: bluetooth::State,
    battery: battery::State,
    dns: dns::State,
    volume: volume::State,
    brightness: brightness::State,
    system: system::State,
}

impl App {
    fn spacing(&self) -> Spacing {
        Spacing::from_theme(self.core.system_theme())
    }

    // Each tile needs the module enabled in config *and* a working backend.
    // The two are separate: config is the user's preference, availability is
    // the machine's answer, and nobody should have to switch off a tile that
    // was never going to work.
    fn show_wifi(&self) -> bool {
        self.config.modules.wifi && self.wifi.availability.is_shown()
    }

    fn show_bluetooth(&self) -> bool {
        self.config.modules.bluetooth && self.bluetooth.availability.is_shown()
    }

    fn show_battery(&self) -> bool {
        self.config.modules.battery && self.battery.is_shown()
    }

    fn show_dns(&self) -> bool {
        self.config.modules.dns && self.dns.availability.is_shown()
    }

    fn show_volume(&self) -> bool {
        self.config.modules.volume && self.volume.availability.is_shown()
    }

    fn show_brightness(&self) -> bool {
        self.config.modules.brightness && self.brightness.availability.is_shown()
    }

    fn show_dark_mode(&self) -> bool {
        self.config.modules.dark_mode && self.system.availability.is_shown()
    }

    fn show_tiling(&self) -> bool {
        self.config.modules.tiling && self.system.tiling_available
    }

    // -- Root -----------------------------------------------------------------

    fn wifi_state_text(&self) -> String {
        match self.wifi.summary_key() {
            "wifi-connected" => self
                .wifi
                .connected_ssid
                .clone()
                .unwrap_or_else(|| fl!("wifi-disconnected")),
            key => crate::i18n::lookup(key, None),
        }
    }

    fn bluetooth_state_text(&self) -> String {
        if !self.bluetooth.powered {
            fl!("bluetooth-off")
        } else if self.bluetooth.connected_devices == 0 {
            fl!("bluetooth-no-devices")
        } else {
            fl!(
                "bluetooth-devices",
                count = self.bluetooth.connected_devices as i64
            )
        }
    }

    fn battery_state_text(&self) -> String {
        match (self.battery.percent, self.battery.active_profile) {
            (Some(percent), Some(profile)) => format!(
                "{} · {}",
                fl!("battery-charge", percent = percent.round() as i64),
                fl!(profile.l10n_key())
            ),
            (Some(percent), None) if self.battery.charging => {
                fl!("battery-charging", percent = percent.round() as i64)
            }
            (Some(percent), None) => fl!("battery-charge", percent = percent.round() as i64),
            // The desktop case: no battery, but power profiles still work.
            (None, Some(profile)) => fl!(profile.l10n_key()),
            (None, None) => fl!("battery-no-battery"),
        }
    }

    fn root_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut tiles: Vec<Element<'_, Message>> = Vec::with_capacity(6);

        if self.show_wifi() {
            tiles.push(
                Tile::new(wifi_icon(&self.wifi), fl!("wifi"), self.wifi_state_text())
                    .active(self.wifi.enabled && !self.wifi.airplane_mode)
                    .on_press(Message::Navigate(Page::Wifi))
                    .view(spacing),
            );
        }
        if self.show_bluetooth() {
            tiles.push(
                Tile::new(
                    "bluetooth-symbolic",
                    fl!("bluetooth"),
                    self.bluetooth_state_text(),
                )
                .active(self.bluetooth.powered)
                .on_press(Message::Navigate(Page::Bluetooth))
                .view(spacing),
            );
        }
        if self.show_battery() {
            let mut tile = Tile::new(
                "battery-symbolic",
                fl!("battery"),
                self.battery_state_text(),
            );
            // Only offer the page when there is something on it.
            if self.battery.profiles.is_shown() {
                tile = tile.on_press(Message::Navigate(Page::Battery));
            }
            tiles.push(tile.view(spacing));
        }
        if self.show_dns() {
            let state = self
                .dns
                .active()
                .map_or_else(|| fl!("dns-custom"), provider_label);
            tiles.push(
                Tile::new("network-server-symbolic", fl!("dns"), state)
                    .on_press(Message::Navigate(Page::Dns))
                    .view(spacing),
            );
        }
        if self.show_dark_mode() {
            let state = if self.system.dark {
                fl!("mode-dark")
            } else {
                fl!("mode-light")
            };
            tiles.push(
                Tile::new(
                    if self.system.dark {
                        "weather-clear-night-symbolic"
                    } else {
                        "weather-clear-symbolic"
                    },
                    fl!("dark-mode"),
                    state,
                )
                .active(self.system.dark)
                .on_press(Message::ToggleDark)
                .view(spacing),
            );
        }
        if self.show_tiling() {
            let state = if self.system.tiling {
                fl!("tiling-on")
            } else {
                fl!("tiling-off")
            };
            tiles.push(
                Tile::new("view-grid-symbolic", fl!("tiling"), state)
                    .active(self.system.tiling)
                    .on_press(Message::ToggleTiling)
                    .view(spacing),
            );
        }

        let has_tiles = !tiles.is_empty();
        let mut content = column::with_capacity(8).spacing(spacing.section);
        for grid_row in tile_grid(tiles, spacing) {
            content = content.push(grid_row);
        }

        let has_sliders = self.show_volume() || self.show_brightness();
        if has_tiles && has_sliders {
            content = content.push(divider::horizontal::default());
        }

        if self.show_volume() {
            content = content.push(slider_row(
                if self.volume.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                },
                self.volume.percent.unwrap_or(0.0),
                Message::SetVolume,
                Some(Message::ToggleMute),
                spacing,
            ));
        }
        if self.show_brightness() {
            content = content.push(slider_row(
                "display-brightness-symbolic",
                self.brightness.percent.unwrap_or(0.0),
                Message::SetBrightness,
                None,
                spacing,
            ));
        }

        content.into()
    }

    // -- Wi-Fi ----------------------------------------------------------------

    fn wifi_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(12)
            .spacing(spacing.section)
            .push(page_header(
                fl!("wifi"),
                Message::Navigate(Page::Root),
                spacing,
            ));

        content = content.push(list_row(
            "airplane-mode-symbolic",
            fl!("airplane-mode"),
            None,
            self.wifi.airplane_mode,
            Some(Message::WifiToggleAirplane),
            spacing,
        ));

        // The radio switch is pointless while airplane mode holds it down, and
        // offering it would be a button that visibly does nothing.
        if !self.wifi.airplane_mode {
            content = content.push(list_row(
                wifi_icon(&self.wifi),
                fl!("wifi"),
                self.wifi.hardware_killed.then(|| fl!("wifi-hardware-off")),
                self.wifi.enabled && !self.wifi.hardware_killed,
                (!self.wifi.hardware_killed).then_some(Message::WifiToggleRadio),
                spacing,
            ));
        }

        if self.wifi.enabled && !self.wifi.airplane_mode && !self.wifi.hardware_killed {
            content = content.push(divider::horizontal::default());
            content = content.push(text::caption(fl!("visible-networks")));

            for net in self.wifi.networks.iter().take(MAX_LIST_ROWS) {
                let detail = match net.join_kind() {
                    network::JoinKind::AlreadyConnected => Some(fl!("connected")),
                    network::JoinKind::UnsupportedEnterprise => Some(fl!("enterprise-in-settings")),
                    _ if self.wifi.connecting.as_deref() == Some(net.ssid.as_str()) => {
                        Some(fl!("connecting"))
                    }
                    _ => Some(fl!("signal-strength", percent = i64::from(net.strength))),
                };

                content = content.push(list_row(
                    signal_icon(net.strength, net.secured),
                    net.ssid.clone(),
                    detail,
                    net.connected,
                    // Enterprise networks with no profile get no action rather
                    // than a password box that cannot possibly work.
                    (net.join_kind() != network::JoinKind::UnsupportedEnterprise)
                        .then(|| Message::WifiSelect(net.ssid.clone())),
                    spacing,
                ));

                // The password field appears inline under the network it
                // belongs to, so it is obvious which one is being joined.
                if self.wifi.password_for.as_deref() == Some(net.ssid.as_str()) {
                    content = content.push(password_field(&self.wifi, spacing));
                }
            }

            if self.wifi.networks.is_empty() {
                content = content.push(text::caption(fl!("no-networks")));
            }
        }

        if let Some(error) = wifi_error_text(&self.wifi) {
            content = content.push(text::caption(error));
        }

        if !self.wifi.details.is_empty() {
            content = content.push(divider::horizontal::default());
            content = content.push(details_view(&self.wifi.details));
        }

        content.into()
    }

    // -- Bluetooth ------------------------------------------------------------

    fn bluetooth_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(10)
            .spacing(spacing.section)
            .push(page_header(
                fl!("bluetooth"),
                Message::Navigate(Page::Root),
                spacing,
            ))
            .push(list_row(
                "bluetooth-symbolic",
                fl!("bluetooth"),
                None,
                self.bluetooth.powered,
                Some(Message::BluetoothTogglePower),
                spacing,
            ));

        if self.bluetooth.powered {
            content = content.push(divider::horizontal::default());

            if self.bluetooth.devices.is_empty() {
                content = content.push(text::caption(fl!("no-devices")));
            }

            for device in self.bluetooth.devices.iter().take(MAX_LIST_ROWS) {
                let detail = if self.bluetooth.busy.as_deref() == Some(device.name.as_str()) {
                    Some(fl!("connecting"))
                } else if device.connected {
                    Some(fl!("connected"))
                } else if device.paired {
                    Some(fl!("paired"))
                } else {
                    // Unpaired devices are listed so you can see they are there,
                    // but pairing needs an agent that can show a PIN.
                    Some(fl!("pair-in-settings"))
                };

                content = content.push(list_row(
                    "bluetooth-symbolic",
                    device.name.clone(),
                    detail,
                    device.connected,
                    device
                        .paired
                        .then(|| Message::BluetoothToggleDevice(device.name.clone())),
                    spacing,
                ));
            }
        }

        content.into()
    }

    // -- Battery --------------------------------------------------------------

    fn battery_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(6)
            .spacing(spacing.section)
            .push(page_header(
                fl!("power-profile"),
                Message::Navigate(Page::Root),
                spacing,
            ));

        // Render what the daemon reported, not all three: hardware without a
        // platform profile driver has no `performance`, and offering a profile
        // that cannot be set would be a dead button.
        for profile in &self.battery.supported_profiles {
            content = content.push(list_row(
                profile_icon(*profile),
                fl!(profile.l10n_key()),
                None,
                self.battery.active_profile == Some(*profile),
                Some(Message::SetProfile(*profile)),
                spacing,
            ));
        }

        if let Some(reason) = &self.battery.performance_degraded {
            content = content.push(text::caption(fl!(
                "performance-degraded",
                reason = reason.clone()
            )));
        }

        content.into()
    }

    // -- DNS ------------------------------------------------------------------

    fn dns_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(10)
            .spacing(spacing.section)
            .push(page_header(
                fl!("dns"),
                Message::Navigate(Page::Root),
                spacing,
            ));

        if let Some(connection) = &self.dns.connection_name {
            content = content.push(text::caption(fl!(
                "dns-on-connection",
                connection = connection.clone()
            )));
        }

        let active = self.dns.active().cloned();
        for provider in &self.dns.providers {
            content = content.push(list_row(
                "network-server-symbolic",
                provider_label(provider),
                servers_summary(provider),
                active.as_ref() == Some(provider),
                Some(Message::SelectDnsProvider(provider.clone())),
                spacing,
            ));
        }

        content = content.push(
            row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(spacing.gap)
                .push(
                    text_input::text_input(fl!("dns-manual-placeholder"), &self.dns.manual_input)
                        .on_input(Message::DnsManualInput)
                        .on_submit(|_| Message::ApplyDnsManual)
                        .width(Length::Fill),
                )
                .push(
                    button::text(fl!("apply"))
                        // Disabled until the field parses, so the button never
                        // silently does nothing.
                        .on_press_maybe(
                            self.dns.manual_provider().map(|_| Message::ApplyDnsManual),
                        ),
                ),
        );

        // Surface the polkit case explicitly. Without this the user picks a
        // provider, sees no change and no error, and concludes it is broken —
        // when NetworkManager refused a system-owned profile.
        match &self.dns.last_error {
            Some(dns::Error::NeedsAuthorisation) => {
                content = content.push(text::caption(fl!("dns-needs-authorisation")));
            }
            Some(dns::Error::Other(reason)) if !reason.is_empty() => {
                content = content.push(text::caption(fl!("dns-failed", reason = reason.clone())));
            }
            _ => {}
        }

        content.into()
    }
}

fn password_field<'a>(wifi: &'a network::State, spacing: Spacing) -> Element<'a, Message> {
    row::with_capacity(3)
        .align_y(Alignment::Center)
        .spacing(spacing.gap)
        .push(
            text_input::secure_input(
                fl!("enter-password"),
                &wifi.password_input,
                None,
                // `true` keeps the characters hidden. There is no reveal toggle:
                // this popup sits over whatever is on screen and dismisses on
                // focus loss, which is a poor place to expose a key.
                true,
            )
            .on_input(Message::WifiPasswordInput)
            .on_submit(|_| Message::WifiSubmitPassword)
            .width(Length::Fill),
        )
        .push(button::text(fl!("connect")).on_press_maybe(
            (!wifi.password_input.is_empty()).then_some(Message::WifiSubmitPassword),
        ))
        .push(button::text(fl!("cancel")).on_press(Message::WifiCancelPassword))
        .into()
}

fn details_view(details: &network::Details) -> Element<'_, Message> {
    let mut rows = column::with_capacity(3);
    for (key, value) in [
        ("ipv4", details.ipv4.as_ref()),
        ("ipv6", details.ipv6.as_ref()),
        ("mac", details.mac.as_ref()),
    ] {
        if let Some(value) = value {
            rows = rows.push(text::caption(format!(
                "{}: {}",
                crate::i18n::lookup(key, None),
                value
            )));
        }
    }
    rows.into()
}

fn wifi_error_text(wifi: &network::State) -> Option<String> {
    let (ssid, error) = wifi.last_error.as_ref()?;
    Some(match error {
        network::Error::AuthFailed => fl!("wifi-auth-failed", ssid = ssid.clone()),
        network::Error::NeedsAuthorisation => fl!("wifi-needs-authorisation"),
        network::Error::Timeout => fl!("wifi-timeout", ssid = ssid.clone()),
        network::Error::Other(reason) if !reason.is_empty() => {
            fl!("wifi-failed", reason = reason.clone())
        }
        network::Error::Other(_) => return None,
    })
}

fn wifi_icon(wifi: &network::State) -> &'static str {
    if wifi.airplane_mode {
        "airplane-mode-symbolic"
    } else if !wifi.enabled || wifi.hardware_killed {
        "network-wireless-disabled-symbolic"
    } else {
        "network-wireless-symbolic"
    }
}

/// Pick the signal-strength icon, matching the names the icon theme ships.
fn signal_icon(strength: u8, secured: bool) -> &'static str {
    match (strength, secured) {
        (80.., true) => "network-wireless-signal-excellent-secure-symbolic",
        (55..80, true) => "network-wireless-signal-good-secure-symbolic",
        (30..55, true) => "network-wireless-signal-ok-secure-symbolic",
        (_, true) => "network-wireless-signal-weak-secure-symbolic",
        (80.., false) => "network-wireless-signal-excellent-symbolic",
        (55..80, false) => "network-wireless-signal-good-symbolic",
        (30..55, false) => "network-wireless-signal-ok-symbolic",
        (_, false) => "network-wireless-signal-weak-symbolic",
    }
}

fn provider_label(provider: &dns::Provider) -> String {
    match &provider.name {
        dns::ProviderName::Builtin(key) => crate::i18n::lookup(key, None),
        dns::ProviderName::Custom(name) => name.clone(),
    }
}

fn servers_summary(provider: &dns::Provider) -> Option<String> {
    // "Automatic" has no servers to list; an empty second line would make it a
    // different height from the providers around it.
    (!provider.is_automatic()).then(|| {
        provider
            .servers
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    })
}

fn profile_icon(profile: battery::Profile) -> &'static str {
    match profile {
        battery::Profile::PowerSaver => "power-profile-power-saver-symbolic",
        battery::Profile::Balanced => "power-profile-balanced-symbolic",
        battery::Profile::Performance => "power-profile-performance-symbolic",
    }
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "dev.jamesjohn.CosmicControlCenter";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let config = Config::load();
        let dns = dns::State::new(&config.dns.custom_providers);
        (
            Self {
                core,
                config,
                popup: None,
                page: Page::Root,
                wifi: network::State::default(),
                bluetooth: bluetooth::State::default(),
                battery: battery::State::default(),
                dns,
                volume: volume::State::default(),
                brightness: brightness::State::default(),
                system: system::State::default(),
            },
            Task::none(),
        )
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::TogglePopup => {
                if let Some(id) = self.popup.take() {
                    self.wifi.scanning = false;
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }
                // Always reopen on the root page. Leaving the popup on a
                // drill-down from last time is disorienting — the panel button
                // should show the grid.
                self.page = Page::Root;
                self.wifi.cancel_password();

                let id = window::Id::unique();
                self.popup = Some(id);
                let mut settings = self.core.applet.get_popup_settings(
                    self.core.main_window_id().unwrap_or(id),
                    id,
                    None,
                    None,
                    None,
                );
                settings.positioner.size_limits = Limits::NONE
                    .max_width(POPUP_WIDTH)
                    .min_width(POPUP_WIDTH)
                    .max_height(POPUP_MAX_HEIGHT);
                cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings)
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.wifi.scanning = false;
                }
                Task::none()
            }
            Message::Navigate(page) => {
                self.page = page;
                // Scanning is the expensive part of the Wi-Fi module, so it runs
                // only while its page is actually on screen.
                self.wifi.scanning = page == Page::Wifi;
                if page != Page::Wifi {
                    self.wifi.cancel_password();
                }
                Task::none()
            }

            Message::WifiToggleRadio => run(self.wifi.toggle_radio()),
            Message::WifiToggleAirplane => run(self.wifi.toggle_airplane_mode()),
            Message::WifiSelect(ssid) => match self.wifi.select(&ssid) {
                Some(future) => {
                    Task::perform(future, |event| cosmic::action::app(Message::Wifi(event)))
                }
                None => Task::none(),
            },
            Message::WifiPasswordInput(value) => {
                self.wifi.password_input = value;
                Task::none()
            }
            Message::WifiSubmitPassword => match self.wifi.submit_password() {
                Some(future) => {
                    Task::perform(future, |event| cosmic::action::app(Message::Wifi(event)))
                }
                None => Task::none(),
            },
            Message::WifiCancelPassword => {
                self.wifi.cancel_password();
                Task::none()
            }

            Message::BluetoothTogglePower => match self.bluetooth.toggle() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::BluetoothToggleDevice(name) => match self.bluetooth.toggle_device(&name) {
                Some(future) => run(future),
                None => Task::none(),
            },

            Message::ToggleDark => run(self.system.toggle_dark()),
            Message::ToggleTiling => run(self.system.toggle_tiling()),

            Message::SetVolume(percent) => run(self.volume.set(percent)),
            Message::ToggleMute => run(self.volume.toggle_mute()),
            Message::SetBrightness(percent) => match self.brightness.set(percent) {
                Some(future) => run(future),
                None => Task::none(),
            },

            Message::SetProfile(profile) => run(self.battery.set_profile(profile)),
            Message::SelectDnsProvider(provider) => {
                let future = self.dns.apply(provider);
                Task::perform(future, |event| cosmic::action::app(Message::Dns(event)))
            }
            Message::DnsManualInput(value) => {
                self.dns.manual_input = value;
                Task::none()
            }
            Message::ApplyDnsManual => match self.dns.manual_provider() {
                Some(provider) => {
                    let future = self.dns.apply(provider);
                    Task::perform(future, |event| cosmic::action::app(Message::Dns(event)))
                }
                None => Task::none(),
            },

            Message::Wifi(event) => {
                self.wifi.update(event);
                Task::none()
            }
            Message::Bluetooth(event) => {
                self.bluetooth.update(event);
                Task::none()
            }
            Message::Battery(event) => {
                self.battery.update(event);
                Task::none()
            }
            Message::Dns(event) => {
                self.dns.update(event);
                Task::none()
            }
            Message::Volume(event) => {
                self.volume.update(event);
                Task::none()
            }
            Message::Brightness(event) => {
                self.brightness.update(event);
                Task::none()
            }
            Message::System(event) => {
                self.system.update(event);
                Task::none()
            }
            Message::Done => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = Vec::with_capacity(7);
        let open = self.popup.is_some();

        // A module switched off in config contributes no subscription, so it
        // never opens a bus connection at all. That is the point of the config
        // toggle: hiding a tile while leaving a D-Bus client running would be a
        // cosmetic fix to a resource problem.
        if self.config.modules.wifi {
            subscriptions.push(self.wifi.subscription().map(Message::Wifi));
        }
        if self.config.modules.bluetooth {
            subscriptions.push(self.bluetooth.subscription().map(Message::Bluetooth));
        }
        if self.config.modules.battery {
            subscriptions.push(self.battery.subscription().map(Message::Battery));
        }
        if self.config.modules.dns {
            subscriptions.push(self.dns.subscription().map(Message::Dns));
        }
        // The polled modules only need sampling while the popup is open —
        // nothing else displays their value, and polling a closed popup is pure
        // idle wakeups on a laptop.
        if self.config.modules.volume && open {
            subscriptions.push(self.volume.subscription().map(Message::Volume));
        }
        if self.config.modules.brightness && open {
            subscriptions.push(self.brightness.subscription().map(Message::Brightness));
        }
        if (self.config.modules.dark_mode || self.config.modules.tiling) && open {
            subscriptions.push(self.system.subscription().map(Message::System));
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        self.core
            .applet
            .icon_button("preferences-system-symbolic")
            .on_press(Message::TogglePopup)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        // Every page scrolls. The root grid rarely needs it, but a Wi-Fi page
        // in a busy building or a Bluetooth page with a dozen paired devices
        // will exceed the popup height cap, and overflow is simply not drawn.
        let page = scrollable_page(match self.page {
            Page::Root => self.root_page(),
            Page::Wifi => self.wifi_page(),
            Page::Bluetooth => self.bluetooth_page(),
            Page::Battery => self.battery_page(),
            Page::Dns => self.dns_page(),
        });

        self.core
            .applet
            .popup_container(
                container(page)
                    .padding(self.spacing().section)
                    .width(Length::Fixed(POPUP_WIDTH)),
            )
            .into()
    }
}

/// Run a backend write, discarding its (unit) result.
fn run(future: impl std::future::Future<Output = ()> + Send + 'static) -> Task<Message> {
    Task::perform(future, |()| cosmic::action::app(Message::Done))
}
