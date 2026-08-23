//! The applet itself: panel button, popup, and the routing between the grid and
//! its drill-down pages.
//!
//! Drill-downs replace the popup's content rather than opening a second window.
//! A panel popup is a layer-shell surface anchored to the button; spawning
//! another one for a sub-page would put it somewhere unpredictable and leave two
//! surfaces to dismiss.

use cosmic::app::{Core, Task};
use cosmic::iced::window::{self, Id};
use cosmic::iced::{Alignment, Length, Limits, Subscription};
use cosmic::widget::{button, column, container, divider, row, text, text_input};
use cosmic::{Application, Element};

use crate::config::Config;
use crate::fl;
use crate::modules::{battery, bluetooth, brightness, dns, network, volume};
use crate::ui::{page_header, slider_row, Tile};

/// Wide enough for two tiles plus their detail lines, narrow enough not to
/// dominate the screen. COSMIC's own quick-settings popups sit around here.
const POPUP_WIDTH: f32 = 360.0;
const POPUP_MAX_HEIGHT: f32 = 720.0;

/// Which screen the popup is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Root,
    Battery,
    Dns,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Navigate(Page),

    ToggleWifi,
    ToggleBluetooth,
    ToggleDarkMode,

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

    /// A backend write finished. Nothing to do — state was updated
    /// optimistically — but the task has to resolve into something.
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
}

impl App {
    fn spacing(&self) -> u16 {
        self.core.system_theme().cosmic().spacing.space_xs
    }

    /// Tiles are only drawn when the module is both enabled in config and
    /// backed by something real. The two checks are separate on purpose:
    /// `config` is the user's preference, `availability` is the machine's
    /// answer, and a user should not have to turn off a tile that was never
    /// going to work anyway.
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

    fn is_dark(&self) -> bool {
        self.core.system_theme().theme_type.is_dark()
    }

    // -- Pages ---------------------------------------------------------------

    fn root_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(6).spacing(spacing);

        // Row 1: Wi-Fi and Bluetooth.
        let wifi_tile = self.show_wifi().then(|| {
            Tile::new("network-wireless-symbolic", fl!("wifi"))
                .detail(if !self.wifi.enabled {
                    fl!("wifi-off")
                } else {
                    self.wifi
                        .ssid
                        .clone()
                        .unwrap_or_else(|| fl!("wifi-disconnected"))
                })
                .active(self.wifi.enabled)
                .on_press(Message::ToggleWifi)
                .view(spacing)
        });

        let bluetooth_tile = self.show_bluetooth().then(|| {
            let detail = if !self.bluetooth.powered {
                fl!("bluetooth-off")
            } else if self.bluetooth.connected_devices == 0 {
                fl!("bluetooth-no-devices")
            } else {
                fl!(
                    "bluetooth-devices",
                    count = self.bluetooth.connected_devices as i64
                )
            };
            Tile::new("bluetooth-symbolic", fl!("bluetooth"))
                .detail(detail)
                .active(self.bluetooth.powered)
                .on_press(Message::ToggleBluetooth)
                .view(spacing)
        });

        // Row 2: Battery and DNS.
        let battery_tile = self.show_battery().then(|| {
            let detail = match (self.battery.percent, self.battery.active_profile) {
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
            };
            let mut tile = Tile::new("battery-symbolic", fl!("battery")).detail(detail);
            // Only offer the drill-down when there is something in it.
            if self.battery.profiles.is_shown() {
                tile = tile.on_drill_down(Message::Navigate(Page::Battery));
            }
            tile.view(spacing)
        });

        let dns_tile = self.show_dns().then(|| {
            let detail = self
                .dns
                .active()
                .map_or_else(|| fl!("dns-custom"), provider_label);
            Tile::new("network-server-symbolic", fl!("dns"))
                .detail(detail)
                .on_drill_down(Message::Navigate(Page::Dns))
                .view(spacing)
        });

        for pair in [(wifi_tile, bluetooth_tile), (battery_tile, dns_tile)] {
            if let Some(grid_row) = tile_row(pair, spacing) {
                content = content.push(grid_row);
            }
        }

        // Row 3: quick toggles.
        if self.config.modules.dark_mode {
            content = content.push(
                Tile::new("dark-mode-symbolic", fl!("dark-mode"))
                    .active(self.is_dark())
                    .on_press(Message::ToggleDarkMode)
                    .view(spacing),
            );
        }

        // Sliders.
        let has_tiles = self.show_wifi()
            || self.show_bluetooth()
            || self.show_battery()
            || self.show_dns()
            || self.config.modules.dark_mode;
        let has_sliders = self.show_volume() || self.show_brightness();

        if has_tiles && has_sliders {
            content = content.push(divider::horizontal::default());
        }

        if self.show_volume() {
            let percent = self.volume.percent.unwrap_or(0.0);
            content = content.push(slider_row(
                if self.volume.muted {
                    "audio-volume-muted-symbolic"
                } else {
                    "audio-volume-high-symbolic"
                },
                percent,
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

    fn battery_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(6).spacing(spacing).push(page_header(
            fl!("power-profile"),
            Message::Navigate(Page::Root),
            spacing,
        ));

        // Render what the daemon reported, not all three: hardware without a
        // platform profile driver has no `performance`, and offering a profile
        // that cannot be set would be a dead button.
        for profile in &self.battery.supported_profiles {
            let selected = self.battery.active_profile == Some(*profile);
            content = content.push(
                Tile::new(profile_icon(*profile), fl!(profile.l10n_key()))
                    .active(selected)
                    .on_press(Message::SetProfile(*profile))
                    .view(spacing),
            );
        }

        if let Some(reason) = &self.battery.performance_degraded {
            content = content.push(text::caption(fl!(
                "performance-degraded",
                reason = reason.clone()
            )));
        }

        content.into()
    }

    fn dns_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(8).spacing(spacing).push(page_header(
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
            let selected = active.as_ref() == Some(provider);
            content = content.push(
                Tile::new("network-server-symbolic", provider_label(provider))
                    .detail_maybe(servers_summary(provider))
                    .active(selected)
                    .on_press(Message::SelectDnsProvider(provider.clone()))
                    .view(spacing),
            );
        }

        content = content.push(
            row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(spacing)
                .push(
                    text_input::text_input(fl!("dns-manual-placeholder"), &self.dns.manual_input)
                        .on_input(Message::DnsManualInput)
                        .on_submit(|_| Message::ApplyDnsManual)
                        .width(Length::Fill),
                )
                .push(
                    button::text(fl!("dns-manual-apply"))
                        // Disabled until the field parses, so the button never
                        // silently does nothing.
                        .on_press_maybe(
                            self.dns.manual_provider().map(|_| Message::ApplyDnsManual),
                        ),
                ),
        );

        // Surface the polkit case explicitly. Without this the user clicks a
        // provider, sees no change and no error, and concludes the applet is
        // broken — when in fact NetworkManager refused a system-owned profile.
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

/// Lay out up to two tiles side by side, collapsing gracefully when only one is
/// present so a hidden module doesn't leave a hole in the grid.
fn tile_row<'a>(
    pair: (Option<Element<'a, Message>>, Option<Element<'a, Message>>),
    spacing: u16,
) -> Option<Element<'a, Message>> {
    match pair {
        (None, None) => None,
        (Some(only), None) | (None, Some(only)) => Some(only),
        (Some(left), Some(right)) => Some(
            row::with_capacity(2)
                .spacing(spacing)
                .push(container(left).width(Length::FillPortion(1)))
                .push(container(right).width(Length::FillPortion(1)))
                .into(),
        ),
    }
}

fn provider_label(provider: &dns::Provider) -> String {
    match &provider.name {
        dns::ProviderName::Builtin(key) => crate::i18n::lookup(key, None),
        dns::ProviderName::Custom(name) => name.clone(),
    }
}

fn servers_summary(provider: &dns::Provider) -> Option<String> {
    // "Automatic" has no servers to list; showing an empty second line would
    // make it a different height from the providers above and below it.
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
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }
                // Always reopen on the root page. Leaving the popup on a
                // drill-down from last time is disorienting — the user clicks
                // the panel button expecting the grid.
                self.page = Page::Root;

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
                }
                Task::none()
            }
            Message::Navigate(page) => {
                self.page = page;
                Task::none()
            }

            Message::ToggleWifi => run(self.wifi.toggle()),
            Message::ToggleBluetooth => match self.bluetooth.toggle() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::ToggleDarkMode => {
                toggle_dark_mode(self.is_dark());
                Task::none()
            }

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
            Message::Done => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = Vec::with_capacity(6);

        // A module switched off in config contributes no subscription, so it
        // never opens a bus connection at all. That is the whole point of the
        // config toggle — hiding the tile while leaving a D-Bus client running
        // would be a cosmetic fix to a resource problem.
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
        // The two polled modules only need sampling while the popup is open —
        // nothing else displays their value, and polling a closed popup is pure
        // idle wakeups on a laptop.
        if self.config.modules.volume && self.popup.is_some() {
            subscriptions.push(self.volume.subscription().map(Message::Volume));
        }
        if self.config.modules.brightness && self.popup.is_some() {
            subscriptions.push(self.brightness.subscription().map(Message::Brightness));
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
        let page = match self.page {
            Page::Root => self.root_page(),
            Page::Battery => self.battery_page(),
            Page::Dns => self.dns_page(),
        };

        self.core
            .applet
            .popup_container(
                container(page)
                    .padding(self.spacing())
                    .width(Length::Fixed(POPUP_WIDTH)),
            )
            .into()
    }
}

/// Run a backend write, discarding its (unit) result.
fn run(future: impl std::future::Future<Output = ()> + Send + 'static) -> Task<Message> {
    Task::perform(future, |()| cosmic::action::app(Message::Done))
}

/// Flip the system between light and dark.
///
/// COSMIC stores this in `cosmic-config` rather than exposing a bus method, and
/// the write is cheap and non-blocking, so it happens inline rather than as a
/// task.
fn toggle_dark_mode(currently_dark: bool) {
    use cosmic::cosmic_config::ConfigSet;
    use cosmic::cosmic_theme::ThemeMode;

    let config = match ThemeMode::config() {
        Ok(config) => config,
        Err(err) => {
            tracing::warn!("could not open the theme config: {err}");
            return;
        }
    };

    // `auto_switch` hands control to a day/night schedule. Leaving it on would
    // let the schedule undo the user's choice minutes later, which reads as the
    // toggle not working.
    if let Err(err) = config.set::<bool>("auto_switch", false) {
        tracing::warn!("could not disable automatic theme switching: {err}");
    }
    if let Err(err) = config.set::<bool>("is_dark", !currently_dark) {
        tracing::warn!("could not switch theme mode: {err}");
    }
}
