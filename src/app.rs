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
use cosmic::widget::{button, column, container, divider, mouse_area, row, text, text_input};
use cosmic::{Application, Element};

use crate::config::Config;
use crate::fl;
use crate::modules::{
    battery, bluetooth, brightness, caffeine, custom, dns, gamemode, keyboard, media, network,
    system, tiling, volume, vpn,
};
use crate::tile_layout::{TileKey, TileShape};
use crate::ui::{
    connectivity_tile, icons, list_row, page_header, scrollable_page, tile_grid, toggle_row,
    wide_slider_tile, ConnectivityRow, SliderMode, Spacing, Tile,
};

/// Wide enough for two tiles plus their state text, narrow enough not to
/// dominate the screen. Public because `ui` sizes its text elision against it.
pub const POPUP_WIDTH: f32 = 360.0;
const POPUP_MAX_HEIGHT: f32 = 720.0;

/// Cap on the Bluetooth device list.
///
/// Past a dozen the list is scrolling anyway and the ones that matter are at the
/// top, since it sorts connected-and-paired first.
const MAX_LIST_ROWS: usize = 12;

/// How many characters of the media title are visible at once.
///
/// Fixed rather than measured: it is what keeps the transport buttons in the
/// same place regardless of what is playing. Sized for the caption font in the
/// space left by the player icon and the three buttons, so the row holds its
/// shape whatever is playing.
const MEDIA_TITLE_CHARS: usize = 30;

/// Networks shown before "Show more".
///
/// A busy building can see thirty access points, and all but a handful are
/// noise: the list sorts connected, then known, then by strength, so the ones
/// worth seeing are always in the first few. Showing everything by default
/// turns a two-tap action into a scroll.
const WIFI_INITIAL_ROWS: usize = 5;
/// How many more each press of "Show more" reveals.
const WIFI_ROW_STEP: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Root,
    Wifi,
    /// Password entry for one network, reached from the Wi-Fi list.
    WifiConnect,
    Bluetooth,
    Battery,
    Dns,
    Vpn,
}

#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Navigate(Page),
    OpenSettings,

    WifiToggleRadio,
    WifiToggleAirplane,
    WifiSelect(String),
    WifiPasswordInput(String),
    WifiSubmitPassword,
    WifiCancelPassword,
    WifiShowMore,

    BluetoothTogglePower,
    BluetoothToggleDevice(String),

    ToggleDark,
    ToggleTiling,
    ToggleDoNotDisturb,
    ToggleKeepAwake,
    CycleKeyboard,
    ToggleChargeThreshold,
    RunCustom(usize),

    SetMicrophone(f64),
    ToggleMicrophoneMute,

    MediaPlayPause,
    MediaNext,
    MediaPrevious,

    ToggleVpn(String),

    SetVolume(f64),
    ToggleMute,
    SetBrightness(f64),
    ToggleDim,
    ToggleGameMode,

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
    GameMode(gamemode::Event),
    Tiling(tiling::Event),
    Microphone(volume::Event),
    Keyboard(keyboard::Event),
    Media(media::Event),
    Vpn(vpn::Event),
    Caffeine(caffeine::Event),

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
    gamemode: gamemode::State,
    tiling: tiling::State,
    microphone: volume::State,
    keyboard: keyboard::State,
    media: media::State,
    vpn: vpn::State,
    caffeine: caffeine::State,
    /// Validated at load so a broken entry is reported once, not silently
    /// drawn as a tile that does nothing.
    custom: Vec<custom::Tile>,
    /// How many networks the Wi-Fi list is currently showing.
    wifi_rows: usize,
}

impl App {
    fn spacing(&self) -> Spacing {
        Spacing::from_theme(self.core.system_theme())
    }

    // Each tile needs the module enabled in config *and* a working backend.
    // The two are separate: config is the user's preference, availability is
    // the machine's answer, and nobody should have to switch off a tile that
    // was never going to work.
    /// Whether the grouped Connectivity tile is on and has at least one row.
    ///
    /// The group needs the user's flag *and* something to put in it: a
    /// machine with no Wi-Fi, no Bluetooth and no VPN gets no group tile,
    /// not an empty card.
    fn show_connectivity(&self) -> bool {
        // The group's own flag, and nothing else. It always has at least the
        // VPN row to show, so there is no "empty card" case to guard against,
        // and it must not look at the standalone tiles' flags: the group and
        // the standalone tiles are independent choices — have both, either,
        // or neither.
        self.placed(TileKey::Connectivity)
    }

    // Rows inside the group gate on the hardware alone. The `wifi` flag is
    // about the *standalone tile*, so switching that off must not empty the
    // group's Wi-Fi row — those are two different questions.
    fn wifi_available(&self) -> bool {
        self.wifi.availability.is_shown()
    }

    fn bluetooth_available(&self) -> bool {
        self.bluetooth.availability.is_shown()
    }

    fn vpn_available(&self) -> bool {
        self.vpn.availability.is_shown()
    }

    // The standalone tiles are independent of the group: on, off, or both.
    fn show_wifi(&self) -> bool {
        self.placed(TileKey::Wifi) && self.wifi_available()
    }

    fn show_bluetooth(&self) -> bool {
        self.placed(TileKey::Bluetooth) && self.bluetooth_available()
    }

    fn show_battery(&self) -> bool {
        self.placed(TileKey::Battery) && self.battery.is_shown()
    }

    fn show_dns(&self) -> bool {
        self.placed(TileKey::Dns) && self.dns.availability.is_shown()
    }

    fn show_volume(&self) -> bool {
        self.placed(TileKey::Volume) && self.volume.availability.is_shown()
    }

    fn show_brightness(&self) -> bool {
        self.placed(TileKey::Brightness) && self.brightness.availability.is_shown()
    }

    fn show_dark_mode(&self) -> bool {
        self.placed(TileKey::DarkMode) && self.system.availability.is_shown()
    }

    fn show_microphone(&self) -> bool {
        self.placed(TileKey::Microphone) && self.microphone.availability.is_shown()
    }

    fn show_keyboard(&self) -> bool {
        self.placed(TileKey::KeyboardBacklight) && self.keyboard.availability.is_shown()
    }

    fn show_media(&self) -> bool {
        self.config.modules.media && self.media.availability.is_shown()
    }

    fn show_vpn(&self) -> bool {
        self.placed(TileKey::Vpn) && self.vpn_available()
    }

    fn show_keep_awake(&self) -> bool {
        self.placed(TileKey::KeepAwake) && self.caffeine.availability.is_shown()
    }

    fn show_do_not_disturb(&self) -> bool {
        self.placed(TileKey::DoNotDisturb) && self.system.dnd_available
    }

    fn show_tiling(&self) -> bool {
        self.placed(TileKey::Tiling) && self.tiling.availability.is_shown()
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
        // Percentage alone when there is a battery. "17% · Balanced" is too
        // long for a tile — it wrapped to two lines, and truncating it to
        // "17% · Balan…" is worse than not showing the profile at all. The
        // profile is one tap away on the page, and the charge is the thing you
        // glance at the grid for.
        match (self.battery.percent, self.battery.active_profile) {
            (Some(percent), _) if self.battery.charging => {
                fl!("battery-charging", percent = percent.round() as i64)
            }
            (Some(percent), _) => fl!("battery-charge", percent = percent.round() as i64),
            // No battery: the profile is all there is to show, and on a desktop
            // it is the reason the tile exists at all.
            (None, Some(profile)) => fl!(profile.l10n_key()),
            (None, None) => fl!("battery-no-battery"),
        }
    }

    /// Every placed instance of this control, in layout order.
    ///
    /// A control can appear several times, at several sizes — that is the
    /// point of the instance model — so callers that need "is it shown at
    /// all" want [`Self::placed`] and callers that draw want this.
    fn instances_of(&self, key: TileKey) -> impl Iterator<Item = &crate::tile_layout::Instance> {
        self.config
            .appearance
            .layout
            .iter()
            .filter(move |i| i.control == key)
    }

    /// Whether this control is in the layout at all.
    ///
    /// **Derived selection**: this replaced the `[modules]` on/off switch for
    /// every control that is a tile. A control is shown because the user put
    /// it on the grid, not because a second switch elsewhere also agrees.
    fn placed(&self, key: TileKey) -> bool {
        self.instances_of(key).next().is_some()
    }

    /// Whether this control's backend has a consumer.
    ///
    /// Same as [`Self::placed`], except that the Connectivity group draws
    /// Wi-Fi, Bluetooth and VPN rows of its own — so a placed group keeps
    /// those three modules alive even with no standalone tile. This one rule
    /// is what the old `show_connectivity` / `|| connectivity` guards
    /// collapsed into.
    fn wanted(&self, key: TileKey) -> bool {
        let via_group = matches!(key, TileKey::Wifi | TileKey::Bluetooth | TileKey::Vpn)
            && self.placed(TileKey::Connectivity);
        self.placed(key) || via_group
    }

    /// The Wide Connectivity tile, with one row per available module.
    fn connectivity_tile(&self, spacing: Spacing) -> Element<'_, Message> {
        let mut rows = Vec::with_capacity(3);

        if self.wifi_available() {
            let killed = self.wifi.hardware_killed || self.wifi.airplane_mode;
            rows.push(ConnectivityRow {
                icon_name: wifi_icon(&self.wifi),
                label: fl!("wifi"),
                state: Some(self.wifi_state_text()),
                on: self.wifi.enabled && !killed,
                on_press: Some(Message::Navigate(Page::Wifi)),
            });
        }

        if self.bluetooth_available() {
            rows.push(ConnectivityRow {
                icon_name: icons::bluetooth(
                    self.bluetooth.powered,
                    self.bluetooth.connected_devices,
                ),
                label: fl!("bluetooth"),
                state: Some(self.bluetooth_state_text()),
                on: self.bluetooth.powered,
                on_press: Some(Message::Navigate(Page::Bluetooth)),
            });
        }

        // Always present, and gated on nothing — not the hardware, and not
        // the standalone VPN tile's flag, which is what it used to check and
        // which made the group's row vanish when that tile was switched off.
        // "No VPN profiles are saved" is a thing the user needs told, and a
        // row that is simply absent tells them nothing. The page explains.
        {
            let active = self.vpn.active_name().is_some();
            rows.push(ConnectivityRow {
                icon_name: icons::vpn(active),
                label: fl!("vpn"),
                state: Some(
                    self.vpn
                        .active_name()
                        .map_or_else(|| fl!("vpn-off"), str::to_string),
                ),
                on: active,
                on_press: Some(Message::Navigate(Page::Vpn)),
            });
        }

        connectivity_tile(
            rows,
            crate::ui::tall_height(spacing),
            self.config.appearance.style,
            self.config.appearance.finish,
            spacing,
        )
    }

    /// The element for one placed control, at the shape that instance asks
    /// for.
    ///
    /// Built per instance rather than once per control. Elements cannot be
    /// cloned, so the previous "build each control once, then hand it to the
    /// first instance that wants it" drew a control placed twice exactly
    /// once — the second instance silently found nothing left and was
    /// skipped. Asking here also means the shape reaches the builder, which
    /// is what lets a control draw differently at Half and at Wide.
    ///
    /// `None` is a control this machine has no hardware for; its cells are
    /// left as gaps rather than closed up.
    fn control_tile(
        &self,
        control: TileKey,
        shape: TileShape,
        spacing: Spacing,
    ) -> Option<Element<'_, Message>> {
        match control {
            TileKey::Connectivity if self.show_connectivity() => {
                Some(self.connectivity_tile(spacing))
            }
            TileKey::Wifi if self.show_wifi() => Some(
                Tile::new(wifi_icon(&self.wifi), fl!("wifi"), self.wifi_state_text())
                    .active(self.wifi.enabled && !self.wifi.airplane_mode)
                    .on_press(Message::Navigate(Page::Wifi))
                    .style(self.config.appearance.style)
                    .finish(self.config.appearance.finish)
                    .compact(shape == TileShape::Half)
                    .view(spacing),
            ),
            TileKey::Bluetooth if self.show_bluetooth() => Some(
                Tile::new(
                    icons::bluetooth(self.bluetooth.powered, self.bluetooth.connected_devices),
                    fl!("bluetooth"),
                    self.bluetooth_state_text(),
                )
                .active(self.bluetooth.powered)
                .on_press(Message::Navigate(Page::Bluetooth))
                .style(self.config.appearance.style)
                .finish(self.config.appearance.finish)
                .compact(shape == TileShape::Half)
                .view(spacing),
            ),
            TileKey::Battery if self.show_battery() => {
                let mut tile = Tile::new(
                    icons::battery(self.battery.percent, self.battery.charging),
                    fl!("battery"),
                    self.battery_state_text(),
                );
                // Only offer the page when there is something on it.
                if self.battery.profiles.is_shown() {
                    tile = tile.on_press(Message::Navigate(Page::Battery));
                }

                // Wide has room for the power profile beside the charge — the
                // thing the tile deliberately drops at Small, where
                // "17% · Balan…" was worse than showing no profile at all.
                if shape == TileShape::Wide {
                    tile = tile.wide(true);
                    if let Some(profile) = self.battery.active_profile {
                        tile = tile.detail(fl!(profile.l10n_key()));
                    }
                }
                Some(
                    tile.style(self.config.appearance.style)
                        .finish(self.config.appearance.finish)
                        .compact(shape == TileShape::Half)
                        .view(spacing),
                )
            }
            TileKey::Dns if self.show_dns() => {
                let state = self
                    .dns
                    .active()
                    .map_or_else(|| fl!("dns-custom"), provider_label);

                let mut tile = Tile::new(icons::dns(), fl!("dns"), state);
                // The provider names the choice; the servers are what it
                // actually resolves through, which is otherwise a page away.
                if shape == TileShape::Wide && !self.dns.current.is_empty() {
                    tile = tile.wide(true).detail(
                        self.dns
                            .current
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                }
                Some(
                    tile.on_press(Message::Navigate(Page::Dns))
                        .style(self.config.appearance.style)
                        .finish(self.config.appearance.finish)
                        .compact(shape == TileShape::Half)
                        .view(spacing),
                )
            }
            TileKey::DarkMode if self.show_dark_mode() => {
                let state = if self.system.dark {
                    fl!("mode-dark")
                } else {
                    fl!("mode-light")
                };

                Some(
                    Tile::new(icons::dark_mode(self.system.dark), fl!("dark-mode"), state)
                        .active(self.system.dark)
                        .on_press(Message::ToggleDark)
                        .style(self.config.appearance.style)
                        .finish(self.config.appearance.finish)
                        .compact(shape == TileShape::Half)
                        .view(spacing),
                )
            }
            TileKey::Tiling if self.show_tiling() => {
                let state = if self.tiling.tiled {
                    fl!("tiling-on")
                } else {
                    fl!("tiling-off")
                };

                Some(
                    Tile::new(icons::tiling(self.tiling.tiled), fl!("tiling"), state)
                        .active(self.tiling.tiled)
                        .on_press(Message::ToggleTiling)
                        .style(self.config.appearance.style)
                        .finish(self.config.appearance.finish)
                        .compact(shape == TileShape::Half)
                        .view(spacing),
                )
            }
            TileKey::Vpn if self.show_vpn() => {
                let state = self
                    .vpn
                    .active_name()
                    .map_or_else(|| fl!("vpn-off"), str::to_string);

                Some(
                    Tile::new(
                        icons::vpn(self.vpn.active_name().is_some()),
                        fl!("vpn"),
                        state,
                    )
                    .active(self.vpn.active_name().is_some())
                    .style(self.config.appearance.style)
                    .finish(self.config.appearance.finish)
                    .on_press(Message::Navigate(Page::Vpn))
                    .compact(shape == TileShape::Half)
                    .view(spacing),
                )
            }
            TileKey::KeyboardBacklight if self.show_keyboard() => Some(
                Tile::new(
                    icons::keyboard(self.keyboard.is_on()),
                    fl!("keyboard-backlight"),
                    crate::i18n::lookup(self.keyboard.level_key(), None),
                )
                .active(self.keyboard.is_on())
                .style(self.config.appearance.style)
                .finish(self.config.appearance.finish)
                .on_press(Message::CycleKeyboard)
                .wide(shape == TileShape::Wide)
                .compact(shape == TileShape::Half)
                .view(spacing),
            ),
            TileKey::DoNotDisturb if self.show_do_not_disturb() => {
                let state = if self.system.do_not_disturb {
                    fl!("on")
                } else {
                    fl!("off")
                };

                Some(
                    Tile::new(
                        icons::do_not_disturb(self.system.do_not_disturb),
                        fl!("do-not-disturb"),
                        state,
                    )
                    .active(self.system.do_not_disturb)
                    .style(self.config.appearance.style)
                    .finish(self.config.appearance.finish)
                    .on_press(Message::ToggleDoNotDisturb)
                    .compact(shape == TileShape::Half)
                    .view(spacing),
                )
            }
            TileKey::KeepAwake if self.show_keep_awake() => {
                // Name whoever is holding it, rather than a bare "On" that leaves
                // the user wondering why the screen will not sleep.
                let state = match (&self.caffeine.held_by, self.caffeine.is_on()) {
                    (Some(who), _) => fl!("keep-awake-held", who = who.clone()),
                    (None, true) => fl!("on"),
                    (None, false) => fl!("off"),
                };

                Some(
                    Tile::new(
                        icons::keep_awake(self.caffeine.is_on()),
                        fl!("keep-awake"),
                        state,
                    )
                    .active(self.caffeine.is_on())
                    .style(self.config.appearance.style)
                    .finish(self.config.appearance.finish)
                    // No press while another program holds the lock — we cannot
                    // release someone else's inhibitor, so the button would do
                    // nothing. Same rule as Game Mode.
                    .on_press_maybe(
                        self.caffeine
                            .can_toggle()
                            .then_some(Message::ToggleKeepAwake),
                    )
                    .wide(shape == TileShape::Wide)
                    .compact(shape == TileShape::Half)
                    .view(spacing),
                )
            }
            TileKey::Volume if self.show_volume() => Some(wide_slider_tile(
                icons::volume(self.volume.percent.unwrap_or(0.0), self.volume.muted),
                fl!("volume"),
                self.volume.percent.unwrap_or(0.0),
                Message::SetVolume,
                Some(Message::ToggleMute),
                if self.volume.muted {
                    SliderMode::Held
                } else {
                    SliderMode::Live
                },
                crate::ui::Look::new(self.config.appearance.finish, spacing),
            )),
            TileKey::Brightness if self.show_brightness() => Some(wide_slider_tile(
                icons::brightness(
                    self.brightness.percent.unwrap_or(0.0),
                    self.brightness.dimmed,
                ),
                fl!("brightness"),
                self.brightness.percent.unwrap_or(0.0),
                Message::SetBrightness,
                Some(Message::ToggleDim),
                if self.brightness.dimmed {
                    SliderMode::Held
                } else {
                    SliderMode::Live
                },
                crate::ui::Look::new(self.config.appearance.finish, spacing),
            )),
            TileKey::Microphone if self.show_microphone() => Some(wide_slider_tile(
                icons::microphone(
                    self.microphone.percent.unwrap_or(0.0),
                    self.microphone.muted,
                ),
                fl!("microphone"),
                self.microphone.percent.unwrap_or(0.0),
                Message::SetMicrophone,
                Some(Message::ToggleMicrophoneMute),
                if self.microphone.muted {
                    SliderMode::Held
                } else {
                    SliderMode::Live
                },
                crate::ui::Look::new(self.config.appearance.finish, spacing),
            )),
            _ => None,
        }
    }

    fn root_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        // Each entry pairs its element with the footprint the packer
        // should give it: Small unless said otherwise (sliders are Tall,
        // Connectivity — when it lands — is Wide).
        // Built keyed, then emitted in the user's order. `keyed` holds every
        // tile this machine can show; `resolve_order` decides the sequence and
        // drops nothing that is here — a key absent from `keyed` is simply a
        // module this machine has no hardware for.
        // Keyed by tile, without a shape: the shape belongs to the key, and
        // writing it at each push meant the popup and `default_shape` could
        // disagree — which they did, silently, leaving the sliders narrow and
        // Connectivity wide after both were reshaped.
        let mut tiles: Vec<(Element<'_, Message>, crate::tile_layout::Slot)> =
            Vec::with_capacity(16);

        // User-defined tiles go last, after everything built in, so adding one
        // never reshuffles the controls someone is used to.
        let mut custom_tiles: Vec<(Element<'_, Message>, TileShape)> =
            Vec::with_capacity(self.custom.len());
        for (index, entry) in self.custom.iter().enumerate() {
            custom_tiles.push((
                Tile::new(
                    icons::resolve_owned(&entry.icon),
                    entry.name.clone(),
                    entry.detail.clone().unwrap_or_else(|| entry.name.clone()),
                )
                .style(self.config.appearance.style)
                .finish(self.config.appearance.finish)
                .on_press(Message::RunCustom(index))
                // Half is the icon-only form: the glyph is the whole tile and
                // the name moves into the tooltip.
                .compact(entry.shape == TileShape::Half)
                .view(spacing),
                entry.shape,
            ));
        }

        // Sliders are packed into the same grid rather than laid out as
        // separate full-width rows underneath.
        // One element per instance, built at that instance's own shape, so a
        // control placed twice draws twice and a control placed Wide draws
        // its Wide form.
        for instance in &self.config.appearance.layout {
            if let Some(element) = self.control_tile(instance.control, instance.shape, spacing) {
                tiles.push((element, instance.slot()));
            }
        }
        // Custom tiles have no TileKey, so they are not yet part of the
        // layout: they keep their index-addressed model and land in the first
        // free cells after everything placed. They join the palette proper
        // later in this series.
        for (element, shape) in custom_tiles {
            let placed: Vec<crate::tile_layout::Instance> = tiles
                .iter()
                .map(|(_, s)| {
                    crate::tile_layout::Instance::new(TileKey::Media, s.shape, s.col, s.row)
                })
                .collect();
            let (col, row) = crate::tile_layout::first_free(&placed, shape);
            tiles.push((element, crate::tile_layout::Slot::new(shape, col, row)));
        }

        let mut content = column::with_capacity(4).spacing(spacing.section);
        if !tiles.is_empty() {
            content = content.push(tile_grid(tiles, crate::ui::Ghosts::Empty, spacing));
        }

        // Media stays a full-width row underneath: it has three buttons and a
        // scrolling title, not a value to nudge.
        if self.show_media() {
            content = content.push(divider::horizontal::default());
            content = content.push(self.media_row(spacing));
        }

        content.into()
    }

    /// Now playing, with transport controls.
    ///
    /// A row rather than a tile: the track name needs the full width, and three
    /// buttons will not fit in half of one.
    fn media_row(&self, spacing: crate::ui::Spacing) -> Element<'_, Message> {
        row::with_capacity(5)
            .align_y(Alignment::Center)
            .spacing(spacing.gap)
            // Whose media this is. A row that says only "Everything In Its
            // Right Place" does not tell you which of three open players will
            // answer the next button press.
            .push(
                cosmic::widget::icon::from_name(self.media.icon.clone()).size(crate::ui::ICON_SIZE),
            )
            .push(
                column::with_capacity(2)
                    .width(Length::Fill)
                    // The player named on top, the track under it. The player's
                    // name is short and fixed and never needs to move; the track
                    // is neither.
                    .push(
                        text::body(self.media.player_name.clone())
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    )
                    .push(
                        // `Fill` so the buttons keep their place, `Wrapping::None`
                        // so a long title cannot push the row taller, and the
                        // marquee so what will not fit still gets read. All three
                        // are needed: any one alone clips, wraps or overflows.
                        text::caption(self.media.marquee(MEDIA_TITLE_CHARS))
                            .width(Length::Fill)
                            .wrapping(cosmic::iced::widget::text::Wrapping::None),
                    ),
            )
            .push(
                button::icon(
                    cosmic::widget::icon::from_name(icons::media_previous())
                        .size(crate::ui::ICON_SIZE),
                )
                .padding(spacing.pad_y)
                .on_press_maybe(self.media.can_previous.then_some(Message::MediaPrevious)),
            )
            .push(
                button::icon(
                    cosmic::widget::icon::from_name(icons::media_play_pause(self.media.playing))
                        .size(crate::ui::ICON_SIZE),
                )
                .padding(spacing.pad_y)
                .on_press(Message::MediaPlayPause),
            )
            .push(
                button::icon(
                    cosmic::widget::icon::from_name(icons::media_next()).size(crate::ui::ICON_SIZE),
                )
                .padding(spacing.pad_y)
                .on_press_maybe(self.media.can_next.then_some(Message::MediaNext)),
            )
            .into()
    }

    // -- VPN ------------------------------------------------------------------

    fn vpn_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut content = column::with_capacity(8)
            .spacing(spacing.section)
            .push(page_header(
                fl!("vpn"),
                Message::Navigate(Page::Root),
                spacing,
            ));

        for profile in &self.vpn.profiles {
            let detail = if self.vpn.busy.as_deref() == Some(profile.uuid.as_str()) {
                Some(fl!("connecting"))
            } else if profile.active {
                Some(fl!("connected"))
            } else {
                None
            };

            content = content.push(list_row(
                icons::vpn(profile.active),
                profile.name.clone(),
                detail,
                profile.active,
                Some(Message::ToggleVpn(profile.uuid.clone())),
                spacing,
            ));
        }

        // With no profiles at all the page is otherwise just a heading and a
        // footnote, which reads as something that failed to load. Say plainly
        // that there is nothing to connect to.
        if self.vpn.profiles.is_empty() {
            content = content.push(text::body(fl!("vpn-none-saved")));
        }

        content = content.push(text::caption(fl!("vpn-add-in-settings")));
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

        content = content.push(toggle_row(
            icons::airplane(),
            fl!("airplane-mode"),
            None,
            self.wifi.airplane_mode,
            Some(Message::WifiToggleAirplane),
            spacing,
        ));

        // The radio switch is pointless while airplane mode holds it down, and
        // offering it would be a button that visibly does nothing.
        if !self.wifi.airplane_mode {
            content = content.push(toggle_row(
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

            for net in self.wifi.networks.iter().take(self.wifi_rows) {
                let detail = match net.join_kind() {
                    network::JoinKind::AlreadyConnected => Some(fl!("connected")),
                    network::JoinKind::UnsupportedEnterprise => Some(fl!("enterprise-in-settings")),
                    _ if self.wifi.connecting.as_deref() == Some(net.ssid.as_str()) => {
                        Some(fl!("connecting"))
                    }
                    _ => Some(fl!("signal-strength", percent = i64::from(net.strength))),
                };

                content = content.push(list_row(
                    icons::signal(net.strength, net.secured),
                    net.ssid.clone(),
                    detail,
                    net.connected,
                    // Enterprise networks with no profile get no action rather
                    // than a password box that cannot possibly work.
                    (net.join_kind() != network::JoinKind::UnsupportedEnterprise)
                        .then(|| Message::WifiSelect(net.ssid.clone())),
                    spacing,
                ));
            }

            // Reveal the rest a few at a time rather than all at once, and say
            // how many are hidden so the button is not a mystery.
            let hidden = self.wifi.networks.len().saturating_sub(self.wifi_rows);
            if hidden > 0 {
                // Label what the press actually reveals, not what is hidden.
                // "Show 17 more" that reveals five is a button that lies.
                let reveals = hidden.min(WIFI_ROW_STEP);
                content = content.push(
                    button::text(fl!("show-more", count = reveals as i64))
                        .width(Length::Fill)
                        .on_press(Message::WifiShowMore),
                );
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

    /// Password entry for one network.
    ///
    /// Its own page rather than a field spliced into the list: the list shifts
    /// under the cursor as scan results come in every few seconds, which moved
    /// an inline field while it was being typed into. A page also gives the
    /// failure message room to be a sentence rather than a fragment.
    fn wifi_connect_page(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let ssid = self.wifi.password_for.clone().unwrap_or_default();

        let mut content = column::with_capacity(8)
            .spacing(spacing.section)
            .push(page_header(
                ssid.clone(),
                // Back returns to the list and abandons the attempt, which is
                // what a back button should do.
                Message::WifiCancelPassword,
                spacing,
            ))
            .push(text::caption(fl!(
                "enter-password-for",
                ssid = ssid.clone()
            )));

        content = content.push(
            text_input::secure_input(
                fl!("enter-password"),
                &self.wifi.password_input,
                None,
                // Characters stay hidden: this popup sits over whatever is on
                // screen and dismisses on focus loss, which is a poor place to
                // expose a key.
                true,
            )
            .on_input(Message::WifiPasswordInput)
            .on_submit(|_| Message::WifiSubmitPassword)
            .width(Length::Fill),
        );

        if let Some(error) = wifi_error_for(&self.wifi, &ssid) {
            content = content.push(text::caption(error));
        }

        let connecting = self.wifi.connecting.as_deref() == Some(ssid.as_str());
        content = content.push(
            row::with_capacity(2)
                .spacing(spacing.gap)
                .push(
                    button::standard(fl!("cancel"))
                        .width(Length::Fill)
                        .on_press(Message::WifiCancelPassword),
                )
                .push(
                    button::suggested(if connecting {
                        fl!("connecting")
                    } else {
                        fl!("connect")
                    })
                    .width(Length::Fill)
                    // Disabled while empty or in flight, so the button never
                    // silently does nothing.
                    .on_press_maybe(
                        (!self.wifi.password_input.is_empty() && !connecting)
                            .then_some(Message::WifiSubmitPassword),
                    ),
                ),
        );

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
            .push(toggle_row(
                icons::bluetooth(self.bluetooth.powered, self.bluetooth.connected_devices),
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
                    icons::bluetooth(true, usize::from(device.connected)),
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

        // How long is left, when UPower has an estimate. Above the profiles
        // because it is the thing you opened this page to read.
        if let Some(seconds) = self.battery.time_remaining {
            let remaining = battery::format_duration(seconds);
            content = content.push(text::body(if self.battery.charging {
                fl!("battery-until-full", time = remaining)
            } else {
                fl!("battery-remaining", time = remaining)
            }));
        }

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

        // Game Mode sits directly after Performance, as the last entry in the
        // same list. It is worth being clear that it is not a fourth profile:
        // it is a separate daemon and it stacks with whichever profile is
        // selected, so its subtitle says so rather than letting the position
        // imply mutual exclusivity.
        if self.config.modules.gamemode && self.gamemode.availability.is_shown() {
            // A switch, not a selectable row. The profiles above it are a
            // pick-one list where selecting one deselects another; Game Mode
            // stacks with whichever is chosen, so it needs the affordance that
            // says "independent on/off" rather than the one that says "one of
            // these".
            content = content.push(toggle_row(
                icons::game_mode(),
                fl!("game-mode"),
                Some(if self.gamemode.can_toggle() {
                    fl!("game-mode-detail")
                } else {
                    // A game is holding it on; say so rather than showing a row
                    // that refuses to respond.
                    fl!("game-mode-held")
                }),
                self.gamemode.active,
                self.gamemode
                    .can_toggle()
                    .then_some(Message::ToggleGameMode),
                spacing,
            ));
        }

        if let Some(reason) = &self.battery.performance_degraded {
            content = content.push(text::caption(fl!(
                "performance-degraded",
                reason = reason.clone()
            )));
        }

        if self.config.modules.charge_threshold && self.battery.charge_threshold_supported {
            content = content.push(divider::horizontal::default());
            content = content.push(toggle_row(
                icons::battery(self.battery.percent, self.battery.charging),
                fl!("charge-limit"),
                Some(fl!("charge-limit-detail")),
                self.battery.charge_threshold_enabled,
                Some(Message::ToggleChargeThreshold),
                spacing,
            ));
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
                icons::dns(),
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

/// The last Wi-Fi error, but only when it is about `about`.
///
/// Errors carry the network they belong to because more than one join can be
/// in flight. Without the filter, a failure for one network renders under
/// another's password field — telling the user the password they have not yet
/// tried is wrong.
fn wifi_error_for(wifi: &network::State, about: &str) -> Option<String> {
    let (ssid, _) = wifi.last_error.as_ref()?;
    if ssid != about {
        return None;
    }
    wifi_error_text(wifi)
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

/// The Wi-Fi tile icon, derived from the module's whole state.
fn wifi_icon(wifi: &network::State) -> &'static str {
    let strength = wifi
        .connected_ssid
        .as_ref()
        .and_then(|ssid| wifi.networks.iter().find(|n| &n.ssid == ssid))
        .map_or(0, |n| n.strength);

    icons::wifi(
        wifi.airplane_mode,
        wifi.hardware_killed,
        wifi.enabled,
        wifi.connected_ssid.is_some(),
        strength,
    )
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
    icons::power_profile(match profile {
        battery::Profile::PowerSaver => icons::PowerProfile::PowerSaver,
        battery::Profile::Balanced => icons::PowerProfile::Balanced,
        battery::Profile::Performance => icons::PowerProfile::Performance,
    })
}

impl Application for App {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.ControlCenter";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let config = Config::load();
        let dns = dns::State::new(&config.dns.custom_providers);
        let custom = custom::usable(&config.custom);
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
                gamemode: gamemode::State::default(),
                tiling: tiling::State::default(),
                microphone: volume::State::new(volume::Direction::Input),
                keyboard: keyboard::State::default(),
                media: media::State::default(),
                vpn: vpn::State::default(),
                caffeine: caffeine::State::default(),
                custom,
                wifi_rows: WIFI_INITIAL_ROWS,
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
                self.wifi_rows = WIFI_INITIAL_ROWS;

                // Pick up anything the Settings window changed. Doing it here
                // rather than watching the file keeps the applet free of a file
                // watcher for something the user only sees on opening the popup
                // anyway.
                let config = Config::load();
                if config.dns.custom_providers != self.config.dns.custom_providers {
                    // Rebuilding drops the manual-entry text, so only do it when
                    // the provider list actually changed.
                    self.dns = dns::State::new(&config.dns.custom_providers);
                }
                self.custom = custom::usable(&config.custom);
                self.config = config;

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
                let popup =
                    cosmic::iced::platform_specific::shell::commands::popup::get_popup(settings);

                // Ask for the blur ourselves. libcosmic issues `enable_blur`
                // only for surfaces it tracks in `surface_views`, and a popup
                // made with `get_popup` is not one of those — `Core::blur`
                // takes the untracked branch, which for an applet is a flat
                // `false`. So the theme said frosted_applets, the popup drew
                // its translucent background, and nothing behind it was ever
                // blurred: transparency without frost, which reads as a film
                // over the wallpaper rather than glass.
                //
                // Gated on the same question libcosmic would have asked, so
                // turning frosted styling off in Settings still turns it off
                // here.
                if self.core.frosted(self.core.system_theme().cosmic()) {
                    Task::batch([popup, cosmic::iced::window::enable_blur(id)])
                } else {
                    popup
                }
            }
            Message::PopupClosed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                    self.wifi.scanning = false;
                }
                Task::none()
            }
            Message::OpenSettings => {
                open_settings_window();
                // Opening a window over the popup leaves the popup orphaned
                // under it, so close it.
                if let Some(id) = self.popup.take() {
                    self.wifi.scanning = false;
                    return cosmic::iced::platform_specific::shell::commands::popup::destroy_popup(
                        id,
                    );
                }
                Task::none()
            }
            Message::Navigate(page) => {
                self.page = page;
                // Scanning is the expensive part of the Wi-Fi module, so it runs
                // only while its page is actually on screen.
                // Keep scanning on the connect page too: it is reached from the
                // list and returns to it, and stopping would leave stale results
                // waiting on the way back.
                self.wifi.scanning = matches!(page, Page::Wifi | Page::WifiConnect);
                if page == Page::Wifi {
                    // Fresh visit, fresh list length.
                    self.wifi_rows = WIFI_INITIAL_ROWS;
                }
                if !self.wifi.scanning {
                    self.wifi.cancel_password();
                }
                Task::none()
            }

            Message::WifiToggleRadio => run(self.wifi.toggle_radio()),
            Message::WifiToggleAirplane => run(self.wifi.toggle_airplane_mode()),
            Message::WifiSelect(ssid) => {
                let action = self.wifi.select(&ssid);
                // `select` opens the password state rather than returning work
                // when a credential is needed; that is the cue to change page.
                if self.wifi.password_for.is_some() {
                    self.page = Page::WifiConnect;
                }
                match action {
                    Some(future) => {
                        Task::perform(future, |event| cosmic::action::app(Message::Wifi(event)))
                    }
                    None => Task::none(),
                }
            }
            Message::WifiShowMore => {
                self.wifi_rows = self.wifi_rows.saturating_add(WIFI_ROW_STEP);
                Task::none()
            }
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
                if self.page == Page::WifiConnect {
                    self.page = Page::Wifi;
                }
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
            Message::ToggleDoNotDisturb => run(self.system.toggle_do_not_disturb()),
            Message::ToggleKeepAwake => match self.caffeine.toggle() {
                Some(future) => Task::perform(future, |event| {
                    cosmic::action::app(Message::Caffeine(event))
                }),
                // Switching off is just closing the descriptor, which already
                // happened inside `toggle`.
                None => Task::none(),
            },
            Message::CycleKeyboard => match self.keyboard.cycle() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::ToggleChargeThreshold => match self.battery.toggle_charge_threshold() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::RunCustom(index) => {
                if let Some(entry) = self.custom.get(index) {
                    entry.run();
                }
                Task::none()
            }

            Message::SetMicrophone(percent) => run(self.microphone.set(percent)),
            Message::ToggleMicrophoneMute => run(self.microphone.toggle_mute()),

            Message::MediaPlayPause => match self.media.play_pause() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::MediaNext => match self.media.next() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::MediaPrevious => match self.media.previous() {
                Some(future) => run(future),
                None => Task::none(),
            },

            Message::ToggleVpn(uuid) => match self.vpn.toggle(&uuid) {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::ToggleTiling => run(self.tiling.toggle()),

            Message::SetVolume(percent) => run(self.volume.set(percent)),
            Message::ToggleMute => run(self.volume.toggle_mute()),
            Message::SetBrightness(percent) => match self.brightness.set(percent) {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::ToggleDim => match self.brightness.toggle_dim() {
                Some(future) => run(future),
                None => Task::none(),
            },
            Message::ToggleGameMode => match self.gamemode.toggle() {
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
                // The connect page is defined by there being a network awaiting
                // a password. Once that clears — joined, or failed in a way that
                // a retype cannot fix — the page has no subject and its header
                // would be blank, so fall back to the list. A wrong password
                // deliberately keeps `password_for` set, which keeps us here
                // with the field ready to correct.
                if self.page == Page::WifiConnect && self.wifi.password_for.is_none() {
                    self.page = Page::Wifi;
                }
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
            Message::GameMode(event) => {
                self.gamemode.update(event);
                Task::none()
            }
            Message::Tiling(event) => {
                self.tiling.update(event);
                Task::none()
            }
            Message::Microphone(event) => {
                self.microphone.update(event);
                Task::none()
            }
            Message::Keyboard(event) => {
                self.keyboard.update(event);
                Task::none()
            }
            Message::Media(event) => {
                self.media.update(event);
                Task::none()
            }
            Message::Vpn(event) => {
                self.vpn.update(event);
                Task::none()
            }
            Message::Caffeine(event) => {
                self.caffeine.update(event);
                Task::none()
            }
            Message::Done => Task::none(),
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let mut subscriptions = Vec::with_capacity(15);
        let open = self.popup.is_some();

        // A module switched off in config contributes no subscription, so it
        // never opens a bus connection at all. That is the point of the config
        // toggle: hiding a tile while leaving a D-Bus client running would be a
        // cosmetic fix to a resource problem.
        // Either consumer keeps the module alive: the standalone tile or
        // a row inside the Connectivity group.
        if self.wanted(TileKey::Wifi) {
            subscriptions.push(self.wifi.subscription().map(Message::Wifi));
        }
        // Either consumer keeps the module alive: the standalone tile or
        // a row inside the Connectivity group.
        if self.wanted(TileKey::Bluetooth) {
            subscriptions.push(self.bluetooth.subscription().map(Message::Bluetooth));
        }
        if self.wanted(TileKey::Battery) {
            subscriptions.push(self.battery.subscription().map(Message::Battery));
        }
        if self.wanted(TileKey::Dns) {
            subscriptions.push(self.dns.subscription().map(Message::Dns));
        }
        // The polled modules only need sampling while the popup is open —
        // nothing else displays their value, and polling a closed popup is pure
        // idle wakeups on a laptop.
        if self.wanted(TileKey::Volume) && open {
            subscriptions.push(self.volume.subscription().map(Message::Volume));
        }
        if self.wanted(TileKey::Brightness) && open {
            subscriptions.push(self.brightness.subscription().map(Message::Brightness));
        }
        if self.wanted(TileKey::DarkMode) && open {
            subscriptions.push(self.system.subscription().map(Message::System));
        }
        if self.wanted(TileKey::Tiling) && open {
            subscriptions.push(self.tiling.subscription().map(Message::Tiling));
        }
        if self.config.modules.gamemode && open {
            subscriptions.push(self.gamemode.subscription().map(Message::GameMode));
        }
        if self.wanted(TileKey::Microphone) && open {
            subscriptions.push(self.microphone.subscription().map(Message::Microphone));
        }
        if self.wanted(TileKey::KeyboardBacklight) && open {
            subscriptions.push(self.keyboard.subscription().map(Message::Keyboard));
        }
        if self.config.modules.media && open {
            subscriptions.push(
                self.media
                    .subscription(MEDIA_TITLE_CHARS)
                    .map(Message::Media),
            );
        }
        if self.wanted(TileKey::KeepAwake) && open {
            subscriptions.push(self.caffeine.subscription().map(Message::Caffeine));
        }
        // VPN state is signal-free, so it polls — but unlike the others it must
        // keep running while the popup is closed, or the tile would show a
        // stale connection the moment it reopens.
        // Either consumer keeps the module alive: the standalone tile or
        // a row inside the Connectivity group.
        if self.wanted(TileKey::Vpn) {
            subscriptions.push(self.vpn.subscription().map(Message::Vpn));
        }

        Subscription::batch(subscriptions)
    }

    fn view(&self) -> Element<'_, Message> {
        let size = self.core.applet.suggested_size(true).0;
        let button = self
            .core
            .applet
            .icon_button_from_handle(icons::panel_handle(&self.config.appearance.icon, size))
            .on_press(Message::TogglePopup);

        // Right-click opens Settings, matching how panel items behave
        // elsewhere. Left-click stays the popup, so the common action is
        // unchanged.
        mouse_area(button)
            .on_right_press(Message::OpenSettings)
            .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Message> {
        // Every page scrolls. The root grid rarely needs it, but a Wi-Fi page
        // in a busy building or a Bluetooth page with a dozen paired devices
        // will exceed the popup height cap, and overflow is simply not drawn.
        let page = scrollable_page(match self.page {
            Page::Root => self.root_page(),
            Page::Wifi => self.wifi_page(),
            Page::WifiConnect => self.wifi_connect_page(),
            Page::Bluetooth => self.bluetooth_page(),
            Page::Battery => self.battery_page(),
            Page::Dns => self.dns_page(),
            Page::Vpn => self.vpn_page(),
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

/// Launch the Settings window as a separate process.
///
/// A second process rather than a second surface in this one: an applet is a
/// layer-shell client, and mixing an ordinary toplevel into the same event loop
/// is more trouble than spawning the binary again with a flag.
fn open_settings_window() {
    let Ok(executable) = std::env::current_exe() else {
        tracing::error!("could not determine our own path; cannot open Settings");
        return;
    };

    let mut command = std::process::Command::new(executable);
    command.arg("--settings");
    // Reaped rather than plain `spawn`: the applet outlives every Settings
    // window it opens, so an unreaped child would sit in the process table as a
    // zombie for the rest of the session.
    match crate::process::spawn_and_reap(command) {
        Ok(pid) => tracing::debug!("opened Settings as pid {pid}"),
        Err(err) => tracing::error!("could not open Settings: {err}"),
    }
}

/// Run a backend write, discarding its (unit) result.
fn run(future: impl std::future::Future<Output = ()> + Send + 'static) -> Task<Message> {
    Task::perform(future, |()| cosmic::action::app(Message::Done))
}
