//! The Settings window, reached by right-clicking the panel button.
//!
//! Runs from the same binary under `--settings`, as a normal window rather than
//! an applet. One binary keeps the config types, the icon resolver and the
//! translations in a single place; a second crate would have to either
//! duplicate them or export them, and there is not enough here to justify that.
//!
//! Changes are written to `config.toml` immediately. There is no Apply button:
//! every control here is a preference with a visible result, and the applet
//! re-reads its config each time the popup opens, so the effect is one click
//! away either way.

use cosmic::app::{Core, Task};
use cosmic::iced::{Alignment, Length, Subscription};
use cosmic::widget::{
    button, column, container, divider, icon, radio, row, scrollable, text, text_input, toggler,
};
use cosmic::{Application, Element};

use crate::config::{Config, PanelIcon, TileStyle};
use crate::fl;
use crate::ui::icons;

const WINDOW_WIDTH: f32 = 560.0;
// Tall enough that Controls and Tile style are both visible without
// scrolling on a 1080p screen; the icon section is one short scroll away.
const WINDOW_HEIGHT: f32 = 800.0;
const PREVIEW_ICON: u16 = 24;

/// Each toggleable module, paired with the label describing it.
///
/// A slice rather than a match arm per module so the list and the form cannot
/// drift apart: adding a module means adding one row here.
const MODULES: &[(&str, &str)] = &[
    ("wifi", "wifi"),
    ("bluetooth", "bluetooth"),
    ("battery", "battery"),
    ("dns", "dns"),
    ("volume", "volume"),
    ("brightness", "brightness"),
    ("dark_mode", "dark-mode"),
    ("tiling", "tiling"),
    ("gamemode", "game-mode"),
    ("microphone", "microphone"),
    ("keyboard_backlight", "keyboard-backlight"),
    ("media", "media"),
    ("vpn", "vpn"),
    ("do_not_disturb", "do-not-disturb"),
    ("keep_awake", "keep-awake"),
    ("charge_threshold", "charge-limit"),
];

/// Which of the three icon sources is in use.
///
/// Needed because `PanelIcon` carries data and so is not `Copy`, while `radio`
/// compares by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconKind {
    System,
    Preset,
    Custom,
}

impl IconKind {
    fn of(icon: &PanelIcon) -> Self {
        match icon {
            PanelIcon::System => IconKind::System,
            PanelIcon::Preset(_) => IconKind::Preset,
            PanelIcon::Custom(_) => IconKind::Custom,
        }
    }
}

pub struct Settings {
    core: Core,
    config: Config,
    /// Text in the custom-icon field, kept separate from the config so a
    /// half-typed path does not get written on every keystroke.
    custom_input: String,
    /// Set when a save fails, so the window can say so rather than appearing to
    /// have worked.
    error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleModule(&'static str, bool),
    ToggleCustom(usize, bool),
    SetStyle(TileStyle),
    SetIcon(PanelIcon),
    CustomInput(String),
    ApplyCustom,
    Close,
}

impl Settings {
    fn module_enabled(&self, key: &str) -> bool {
        let m = &self.config.modules;
        match key {
            "wifi" => m.wifi,
            "bluetooth" => m.bluetooth,
            "battery" => m.battery,
            "dns" => m.dns,
            "volume" => m.volume,
            "brightness" => m.brightness,
            "dark_mode" => m.dark_mode,
            "tiling" => m.tiling,
            "gamemode" => m.gamemode,
            "microphone" => m.microphone,
            "keyboard_backlight" => m.keyboard_backlight,
            "media" => m.media,
            "vpn" => m.vpn,
            "do_not_disturb" => m.do_not_disturb,
            "keep_awake" => m.keep_awake,
            "charge_threshold" => m.charge_threshold,
            _ => false,
        }
    }

    fn set_module(&mut self, key: &str, value: bool) {
        let m = &mut self.config.modules;
        match key {
            "wifi" => m.wifi = value,
            "bluetooth" => m.bluetooth = value,
            "battery" => m.battery = value,
            "dns" => m.dns = value,
            "volume" => m.volume = value,
            "brightness" => m.brightness = value,
            "dark_mode" => m.dark_mode = value,
            "tiling" => m.tiling = value,
            "gamemode" => m.gamemode = value,
            "microphone" => m.microphone = value,
            "keyboard_backlight" => m.keyboard_backlight = value,
            "media" => m.media = value,
            "vpn" => m.vpn = value,
            "do_not_disturb" => m.do_not_disturb = value,
            "keep_awake" => m.keep_awake = value,
            "charge_threshold" => m.charge_threshold = value,
            _ => {}
        }
    }

    /// Persist, recording any failure for the view to show.
    fn save(&mut self) {
        self.error = self.config.save().err();
        if let Some(err) = &self.error {
            tracing::error!("could not save settings: {err}");
        }
    }

    fn spacing(&self) -> u16 {
        self.core.system_theme().cosmic().spacing.space_xs
    }

    fn controls_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut section = column::with_capacity(MODULES.len() + 2)
            .spacing(spacing)
            .push(text::title4(fl!("settings-controls")))
            .push(text::caption(fl!("settings-controls-detail")));

        for &(key, label) in MODULES {
            section = section.push(
                row::with_capacity(2)
                    .align_y(Alignment::Center)
                    .push(text::body(crate::i18n::lookup(label, None)).width(Length::Fill))
                    .push(
                        toggler(self.module_enabled(key))
                            .on_toggle(move |value| Message::ToggleModule(key, value)),
                    ),
            );
        }

        // Tiles from `[[custom]]` in config.toml. They belong in the same list
        // as everything else: from the user's side a tile they added and a tile
        // that shipped are the same kind of thing, and having a switch for one
        // but not the other is why a Screenshot tile could be seen in the popup
        // and not found in here.
        if !self.config.custom.is_empty() {
            section = section
                .push(divider::horizontal::default())
                .push(text::caption(fl!("settings-custom-detail")));

            for (index, tile) in self.config.custom.iter().enumerate() {
                let mut labels = column::with_capacity(2)
                    .push(text::body(tile.name.clone()).width(Length::Fill));
                // Say what it runs. A tile called "Screenshot" is obvious; one
                // called "Work" is not, and the command is the only thing that
                // distinguishes two tiles with the same name.
                labels = labels.push(text::caption(tile.command.join(" ")));

                section = section.push(
                    row::with_capacity(2)
                        .align_y(Alignment::Center)
                        .push(labels.width(Length::Fill))
                        .push(
                            toggler(tile.enabled)
                                .on_toggle(move |value| Message::ToggleCustom(index, value)),
                        ),
                );
            }
        }

        section.into()
    }

    fn style_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut section = column::with_capacity(TileStyle::ALL.len() + 2)
            .spacing(spacing)
            .push(text::title4(fl!("settings-style")))
            .push(text::caption(fl!("settings-style-detail")));

        for style in TileStyle::ALL {
            section = section.push(
                column::with_capacity(2)
                    .push(radio(
                        text::body(crate::i18n::lookup(style.l10n_key(), None)),
                        style,
                        Some(self.config.appearance.style),
                        Message::SetStyle,
                    ))
                    .push(text::caption(crate::i18n::lookup(
                        style.description_key(),
                        None,
                    ))),
            );
        }

        section.into()
    }

    fn icon_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let current = &self.config.appearance.icon;

        let mut section = column::with_capacity(8)
            .spacing(spacing)
            .push(text::title4(fl!("settings-icon")))
            .push(text::caption(fl!("settings-icon-detail")))
            .push(radio(
                text::body(fl!("icon-system")),
                IconKind::System,
                Some(IconKind::of(current)),
                |_| Message::SetIcon(PanelIcon::System),
            ));

        // Presets shown as a row of previews: the glyph is the whole point, so
        // a name alone would make the choice guesswork.
        let mut previews = row::with_capacity(icons::PRESETS.len()).spacing(spacing);
        for (preset, label) in icons::PRESETS {
            let selected = matches!(current, PanelIcon::Preset(p) if p == preset);
            previews = previews.push(
                button::custom(
                    column::with_capacity(2)
                        .align_x(Alignment::Center)
                        .spacing(spacing / 2)
                        .push(icon::from_name(icons::preset_name(preset)).size(PREVIEW_ICON))
                        .push(text::caption(crate::i18n::lookup(label, None))),
                )
                .padding(spacing)
                .class(if selected {
                    button::ButtonClass::Suggested
                } else {
                    button::ButtonClass::Standard
                })
                .on_press(Message::SetIcon(PanelIcon::Preset(preset.to_string()))),
            );
        }
        section = section.push(previews);

        section = section.push(
            row::with_capacity(3)
                .align_y(Alignment::Center)
                .spacing(spacing)
                .push(
                    text_input::text_input(fl!("icon-custom-placeholder"), &self.custom_input)
                        .on_input(Message::CustomInput)
                        .on_submit(|_| Message::ApplyCustom)
                        .width(Length::Fill),
                )
                .push(button::text(fl!("apply")).on_press_maybe(
                    (!self.custom_input.trim().is_empty()).then_some(Message::ApplyCustom),
                )),
        );

        // Live preview of whatever is currently chosen, so a name that does not
        // resolve is visible immediately rather than after a restart.
        section = section.push(
            row::with_capacity(2)
                .align_y(Alignment::Center)
                .spacing(spacing)
                .push(text::caption(fl!("icon-preview")))
                .push(icon::icon(icons::panel_handle(current, PREVIEW_ICON)).size(PREVIEW_ICON)),
        );

        section.into()
    }
}

impl Application for Settings {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;

    const APP_ID: &'static str = "io.github.jjnuthuagen.ControlCenterSettings";

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: ()) -> (Self, Task<Message>) {
        let config = Config::load();
        let custom_input = match &config.appearance.icon {
            PanelIcon::Custom(value) => value.clone(),
            _ => String::new(),
        };
        (
            Self {
                core,
                config,
                custom_input,
                error: None,
            },
            Task::none(),
        )
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleModule(key, value) => {
                self.set_module(key, value);
                self.save();
            }
            Message::ToggleCustom(index, value) => {
                if let Some(tile) = self.config.custom.get_mut(index) {
                    tile.enabled = value;
                    self.save();
                }
            }
            Message::SetStyle(style) => {
                self.config.appearance.style = style;
                self.save();
            }
            Message::SetIcon(icon) => {
                if !matches!(icon, PanelIcon::Custom(_)) {
                    self.custom_input.clear();
                }
                self.config.appearance.icon = icon;
                self.save();
            }
            Message::CustomInput(value) => self.custom_input = value,
            Message::ApplyCustom => {
                let value = self.custom_input.trim().to_string();
                if !value.is_empty() {
                    self.config.appearance.icon = PanelIcon::Custom(value);
                    self.save();
                }
            }
            Message::Close => return cosmic::iced::exit(),
        }
        Task::none()
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::none()
    }

    fn view(&self) -> Element<'_, Message> {
        let spacing = self.spacing();

        let mut body = column::with_capacity(8)
            .spacing(spacing * 2)
            .padding(spacing * 2)
            .push(self.controls_section())
            .push(divider::horizontal::default())
            .push(self.style_section())
            .push(divider::horizontal::default())
            .push(self.icon_section());

        if let Some(error) = &self.error {
            body = body.push(text::caption(fl!(
                "settings-save-failed",
                reason = error.clone()
            )));
        } else {
            body = body.push(text::caption(fl!("settings-saved")));
        }

        body = body.push(
            row::with_capacity(2)
                .push(cosmic::widget::Space::new().width(Length::Fill))
                .push(button::suggested(fl!("close")).on_press(Message::Close)),
        );

        container(scrollable(body))
            .width(Length::Fixed(WINDOW_WIDTH))
            .height(Length::Fill)
            .into()
    }
}

/// Window settings for `--settings`.
pub fn window_settings() -> cosmic::app::Settings {
    cosmic::app::Settings::default()
        .size(cosmic::iced::Size::new(WINDOW_WIDTH, WINDOW_HEIGHT))
        .resizable(Some(8.0))
        .debug(false)
}
