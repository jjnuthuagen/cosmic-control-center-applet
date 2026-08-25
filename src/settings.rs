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
    button, column, container, divider, icon, radio, row, scrollable, segmented_button, text,
    text_input, toggler,
};
use cosmic::ApplicationExt;
use cosmic::{Application, Element};

use crate::config::{Config, PanelIcon, TileStyle};
use crate::fl;
use crate::tile_layout::{TileKey, TileShape};
use crate::ui::{
    connectivity_tile, icons, tile_grid, wide_slider_tile, ConnectivityRow, SliderMode, Spacing,
    Tile,
};

const WINDOW_WIDTH: f32 = 560.0;
// Tall enough that Controls and Tile style are both visible without
// scrolling on a 1080p screen; the icon section is one short scroll away.
const WINDOW_HEIGHT: f32 = 800.0;
const PREVIEW_ICON: u16 = 24;

/// The application icon, shared with both desktop entries.
const APP_ICON: &str = "io.github.jjnuthuagen.ControlCenter";

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// The `[modules]` key that gates a tile, and the shape it packs as.
fn preview_module_key(key: TileKey) -> &'static str {
    match key {
        TileKey::Connectivity => "connectivity",
        TileKey::Wifi => "wifi",
        TileKey::Bluetooth => "bluetooth",
        TileKey::Vpn => "vpn",
        TileKey::Battery => "battery",
        TileKey::Dns => "dns",
        TileKey::DarkMode => "dark_mode",
        TileKey::Tiling => "tiling",
        TileKey::GameMode => "gamemode",
        TileKey::Media => "media",
        TileKey::DoNotDisturb => "do_not_disturb",
        TileKey::KeepAwake => "keep_awake",
        TileKey::ChargeThreshold => "charge_threshold",
        TileKey::KeyboardBacklight => "keyboard_backlight",
        TileKey::Volume => "volume",
        TileKey::Brightness => "brightness",
        TileKey::Microphone => "microphone",
    }
}

/// Lay a grab handle over a tile's top-right corner when `show` is set.
///
/// A `stack` rather than putting the handle inside the tile: the tiles here
/// are the popup's own widgets, drawn so the preview is honest about what the
/// grid looks like, and threading a Settings-only decoration through them
/// would put Settings' concerns inside the thing being previewed.
fn with_drag_handle(tile: Element<'_, Message>, show: bool, spacing: u16) -> Element<'_, Message> {
    if !show {
        return tile;
    }

    let handle = container(cosmic::widget::icon::from_name(icons::drag_handle()).size(HANDLE_ICON))
        .padding(spacing / 2)
        .width(Length::Fill)
        .align_x(cosmic::iced::Alignment::End)
        .align_y(cosmic::iced::Alignment::Start);

    cosmic::iced::widget::stack![tile, handle].into()
}

/// The grab handle's glyph size — smaller than a tile's own icon, because it
/// is an affordance on top of the content rather than part of it.
const HANDLE_ICON: u16 = 14;

/// A preview tile dressed for its state.
///
/// Three states, and they have to stay visually distinct:
///
/// * **Selected** — drawn plainly. It is in the grid; nothing to say.
/// * **Not selected** — dimmed behind a dashed outline. Dimming rather than
///   removal, because a tile that vanishes when switched off leaves nowhere
///   to switch it back on. Dashed rather than accented, because the accent
///   already means "this control is on" inside the tile itself, and reusing
///   it here would make a switched-off Wi-Fi tile and an excluded one look
///   the same.
/// * **Being dragged** — a solid accent outline, so the tile the grid is
///   shuffling around is unmistakable.
fn preview_frame(
    tile: Element<'_, Message>,
    selected: bool,
    dragging: bool,
) -> Element<'_, Message> {
    if dragging {
        return container(tile)
            .class(cosmic::theme::Container::Custom(Box::new(|theme| {
                let cosmic = theme.cosmic();
                cosmic::widget::container::Style {
                    border: cosmic::iced::Border {
                        radius: cosmic.corner_radii.radius_s.into(),
                        width: 2.0,
                        color: cosmic.accent_color().into(),
                    },
                    ..Default::default()
                }
            })))
            .into();
    }
    if selected {
        return tile;
    }

    // iced has no opacity on an arbitrary element, so "dimmed" is a wash of
    // the background colour laid over the tile by the container's own
    // background — the tile keeps its colours, the wash mutes them.
    container(tile)
        .class(cosmic::theme::Container::Custom(Box::new(|theme| {
            let cosmic = theme.cosmic();
            let mut wash = cosmic.bg_color();
            wash.alpha = 0.55;
            cosmic::widget::container::Style {
                background: Some(cosmic::iced::Background::Color(cosmic::iced::Color::from(
                    wash,
                ))),
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    width: 1.0,
                    color: cosmic::iced::Color::from({
                        let mut edge = cosmic.on_bg_color();
                        edge.alpha = 0.25;
                        edge
                    }),
                },
                ..Default::default()
            }
        })))
        .into()
}

/// Ask the desktop for an image file.
///
/// Goes through the XDG portal, so it is the file chooser the user already
/// knows and it works under a sandbox. Returns `None` when they cancel, and
/// also when no portal is running — a desktop without one is not an error
/// worth a dialog of its own.
async fn choose_icon() -> Option<std::path::PathBuf> {
    use cosmic::dialog::file_chooser::{open::Dialog, FileFilter};

    // ashpd's filter matches on glob patterns rather than bare extensions, and
    // its label is a plain `&str`, so this cannot take the translated string by
    // value the way the rest of the window does.
    let filter = FileFilter::new("Images")
        .glob("*.svg")
        .glob("*.png")
        .glob("*.jpg")
        .glob("*.jpeg");

    let response = Dialog::new()
        .title(fl!("icon-choose-title"))
        .filter(filter)
        .open_file()
        .await
        .ok()?;

    response.url().to_file_path().ok()
}

/// Copy a chosen icon into our own config directory, and return where it went.
///
/// The path is stored in `config.toml`, and a path into someone's Pictures
/// folder is a path they can move, rename or empty from the trash — at which
/// point the panel button silently falls back to a generic glyph and nothing
/// says why. Keeping our own copy means the icon survives whatever happens to
/// the original.
fn adopt_icon(source: &std::path::Path) -> Result<std::path::PathBuf, String> {
    let folder = Config::path()
        .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
        .ok_or("no config directory")?
        .join("icons");
    std::fs::create_dir_all(&folder)
        .map_err(|err| format!("could not create {}: {err}", folder.display()))?;

    let name = source
        .file_name()
        .ok_or_else(|| format!("{} has no file name", source.display()))?;
    let destination = folder.join(name);

    // Copying a file onto itself truncates it on some filesystems, so the
    // re-pick of an already-adopted icon has to be a no-op rather than a copy.
    if source == destination {
        return Ok(destination);
    }

    std::fs::copy(source, &destination).map_err(|err| format!("could not copy the icon: {err}"))?;
    Ok(destination)
}

/// Hand a URL to the desktop.
fn open_url(url: &str) {
    let mut command = std::process::Command::new("xdg-open");
    command.arg(url);
    if let Err(err) = crate::process::spawn_and_reap(command) {
        tracing::warn!("could not open {url}: {err}");
    }
}

/// Open the directory holding `config.toml` in the file manager.
///
/// The directory rather than the file: opening the file hands it to whatever
/// owns `.toml`, which on a fresh install is often nothing at all, and a dialog
/// asking which application to use is not an answer to "where do I put this?".
fn open_config_folder() {
    let Some(path) = Config::path() else {
        tracing::error!("no config directory to open");
        return;
    };
    let Some(folder) = path.parent() else {
        return;
    };

    // The folder may not exist yet: nothing is written until a setting changes,
    // and opening a path that is not there fails silently.
    if let Err(err) = std::fs::create_dir_all(folder) {
        tracing::warn!("could not create {}: {err}", folder.display());
        return;
    }

    let mut command = std::process::Command::new("xdg-open");
    command.arg(folder);
    if let Err(err) = crate::process::spawn_and_reap(command) {
        tracing::warn!("could not open {}: {err}", folder.display());
    }
}

/// The window's three pages.
///
/// The list of controls, the appearance settings and the about page were one
/// scrolling column, which meant the panel icon lived a screen and a half below
/// the switch you came in to flip. Tabs are how a settings window of this size
/// stays navigable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Tiles,
    Styling,
    About,
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
    /// The tab bar's model, which owns which tab is showing.
    tabs: segmented_button::SingleSelectModel,
    /// The tile the pointer went down on, if the button is still held.
    ///
    /// A press is only a *candidate* for either gesture. It becomes a drag the
    /// moment the pointer enters a different tile, and a tap otherwise — which
    /// is how one pointer button carries both selecting and reordering without
    /// a modifier key.
    pressed: Option<TileKey>,
    /// Whether the held press has turned into a drag.
    dragging: bool,
    /// The tile under the pointer, so it can show its grab handle.
    ///
    /// A drag gesture nobody knows about is a drag gesture nobody uses: the
    /// handle appearing under the pointer is what says the tiles move.
    hovered: Option<TileKey>,
    /// The order being edited while a tile is picked. Committed to config on
    /// drop, discarded on cancel — so a cancelled move leaves the file as it
    /// was rather than half-applied.
    working_order: Vec<TileKey>,
    /// Everything the About page displays. Built once and kept, because
    /// `widget::about` borrows from it for the lifetime of the view.
    about: cosmic::widget::about::About,
}

#[derive(Debug, Clone)]
pub enum Message {
    Tab(segmented_button::Entity),
    /// Preview tiles need a press handler to render as buttons; nothing
    /// happens on press, because there is no module behind them here.
    Noop,
    /// Pointer down on a preview tile. Not yet a decision — see
    /// [`Settings::pressed`].
    PressTile(TileKey),
    /// The pointer entered a preview tile while a press is held. Turns the
    /// press into a drag and moves the held tile to this one's slot, so the
    /// grid re-packs around it — that live shuffle is the placement cue.
    HoverTile(TileKey),
    /// Pointer up. A drag commits the new order; a plain press selects or
    /// deselects the tile that was pressed.
    ReleaseTile,
    /// The pointer left a preview tile.
    ExitTile,
    /// Another `--settings` invocation asked this window to come forward.
    Present,
    ToggleCustom(usize, bool),
    OpenConfigFolder,
    /// Ask the desktop's file chooser for an image.
    SelectIcon,
    /// One came back. The path is the user's own copy, not ours yet.
    IconSelected(std::path::PathBuf),
    OpenUrl(String),
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
            "connectivity" => m.connectivity,
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
            "connectivity" => m.connectivity = value,
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

    /// The Tiles page: the popup's own grid, drawn here, with a switch under
    /// every tile.
    ///
    /// Same packer, same tile widgets, same shapes and order as the popup, so
    /// what you see is what the panel button opens. Two honest limits:
    ///
    /// * **State is a placeholder.** Settings is its own process with no
    ///   D-Bus subscriptions — a module switched off is never constructed, and
    ///   the whole point of that design is that Settings must not start
    ///   sixteen bus clients just to draw a preview. So the Wi-Fi tile says
    ///   "Wi-Fi", not the SSID. Layout, shape and order are exact; the words
    ///   inside are not.
    /// * **A switched-off tile stays visible, dimmed.** Removing it from the
    ///   grid the instant it is switched off leaves nowhere to switch it back
    ///   on. It stays put with its switch off, and the popup — which *does*
    ///   honour the flag — drops it.
    fn preview_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let theme_spacing = Spacing::from_theme(self.core.system_theme());
        let mut section = column::with_capacity(3)
            .spacing(spacing)
            .push(text::title4(fl!("settings-controls")))
            .push(text::caption(fl!("settings-preview-detail")));

        // Two grids. The top one holds only what the popup draws, in the
        // order it draws it — so it *is* the popup's layout, not a superset of
        // it with some tiles dimmed. Everything switched off sits in its own
        // grid underneath, where tapping brings it back up.
        let mut shown: Vec<(Element<'_, Message>, TileShape)> = Vec::with_capacity(20);
        let mut hidden: Vec<(Element<'_, Message>, TileShape)> = Vec::with_capacity(20);

        for &key in self.preview_order().iter() {
            let module_key = preview_module_key(key);
            let shape = key.shape_with(&self.config.appearance.shapes);
            let enabled = self.module_enabled(module_key);
            let tile = self.preview_tile(key, enabled, theme_spacing);
            let dragging = self.dragging && self.pressed == Some(key);
            let tile = preview_frame(tile, enabled, dragging);
            // The handle appears under the pointer, which is what tells you
            // the tiles can be moved at all. Only on tiles that are in the
            // grid: the hidden ones are not part of the layout, so there is
            // nothing to drag them among.
            let tile = with_drag_handle(
                tile,
                enabled && (self.hovered == Some(key) || dragging),
                spacing,
            );
            // One pointer button, two gestures: a press that never leaves the
            // tile is a tap and toggles it; a press that wanders into another
            // shown tile is a drag and reorders. `mouse_area` rather than the
            // tile's own button so both live in one place.
            let mut area = cosmic::widget::mouse_area(tile)
                .on_press(Message::PressTile(key))
                .on_release(Message::ReleaseTile)
                .on_exit(Message::ExitTile);
            if enabled {
                area = area
                    .interaction(if dragging {
                        cosmic::iced::mouse::Interaction::Grabbing
                    } else {
                        cosmic::iced::mouse::Interaction::Grab
                    })
                    .on_enter(Message::HoverTile(key));
            } else {
                area = area.interaction(cosmic::iced::mouse::Interaction::Pointer);
            }
            let entry = (area.into(), shape);
            if enabled {
                shown.push(entry);
            } else {
                hidden.push(entry);
            }
        }

        section = section.push(tile_grid(shown, theme_spacing));

        if !hidden.is_empty() {
            section = section
                .push(text::title4(fl!("settings-hidden")))
                .push(text::caption(fl!("settings-hidden-detail")))
                .push(tile_grid(hidden, theme_spacing));
        }
        section.into()
    }

    /// The tiles to preview, in the order the popup will draw them.
    ///
    /// Mirrors the popup's rule: with the Connectivity group on, Wi-Fi,
    /// Bluetooth and VPN live inside it and do not appear on their own; with
    /// it off, the three come first as standalone tiles.
    /// Every tile key, in the user's order — including ones this page does
    /// not draw.
    ///
    /// This is what gets persisted. Saving only the *drawn* subset silently
    /// dropped every other key from the file, and `resolve_order` then
    /// re-appended them at the end — so one drag could shunt Connectivity to
    /// the bottom of a grid the user never touched. Reordering has to happen
    /// in the complete list and be written back complete.
    fn full_order(&self) -> Vec<TileKey> {
        // Mid-move, the order being edited is the one to use.
        if self.pressed.is_some() && !self.working_order.is_empty() {
            return self.working_order.clone();
        }
        crate::tile_layout::resolve_order(&self.config.appearance.order, |_| true)
    }

    /// The subset of [`Self::full_order`] this page draws.
    ///
    /// Every tile is previewed whether or not its switch is on — the switch is
    /// what you came here to find, and a tile that vanishes the moment you
    /// switch it off leaves nowhere to switch it back on. The group and the
    /// standalone tiles are independent, so all four are shown.
    fn preview_order(&self) -> Vec<TileKey> {
        self.full_order()
            .into_iter()
            .filter(|k| {
                // Game Mode and the charge limit live inside the Battery page,
                // and Media is a full-width row under the grid. None is a grid
                // tile, so none belongs in a preview of the grid.
                !matches!(
                    k,
                    TileKey::GameMode | TileKey::Media | TileKey::ChargeThreshold
                )
            })
            .collect()
    }

    /// One preview tile, using the real widgets with placeholder state.
    fn preview_tile(&self, key: TileKey, enabled: bool, spacing: Spacing) -> Element<'_, Message> {
        let style = self.config.appearance.style;
        let label = |ftl: &str| crate::i18n::lookup(ftl, None);

        match key {
            TileKey::Connectivity => {
                // Every row, always: the group's rows follow the hardware, not
                // the standalone tiles' switches, same as the popup.
                let rows = [
                    ("wifi", icons::wifi(false, false, true, true, 80)),
                    ("bluetooth", icons::bluetooth(true, 0)),
                    ("vpn", icons::vpn(false)),
                ]
                .into_iter()
                .map(|(ftl, icon)| ConnectivityRow {
                    icon_name: icon,
                    label: label(ftl),
                    state: None,
                    on: enabled,
                    on_press: None,
                })
                .collect();
                // The popup's own Tall height: with the switch rows gone, a
                // preview cell is exactly a tile again.
                connectivity_tile(rows, crate::ui::tall_height(spacing), style, spacing)
            }
            TileKey::Volume => wide_slider_tile(
                icons::volume(60.0, false),
                label("volume"),
                60.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                spacing,
            ),
            TileKey::Brightness => wide_slider_tile(
                icons::brightness(70.0, false),
                label("brightness"),
                70.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                spacing,
            ),
            TileKey::Microphone => wide_slider_tile(
                icons::microphone(50.0, false),
                label("microphone"),
                50.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                spacing,
            ),
            other => {
                let (icon, ftl) = match other {
                    TileKey::Wifi => (icons::wifi(false, false, true, true, 80), "wifi"),
                    TileKey::Bluetooth => (icons::bluetooth(true, 0), "bluetooth"),
                    TileKey::Vpn => (icons::vpn(false), "vpn"),
                    TileKey::Battery => (icons::battery(Some(80.0), false), "battery"),
                    TileKey::Dns => (icons::dns(), "dns"),
                    TileKey::DarkMode => (icons::dark_mode(true), "dark-mode"),
                    TileKey::Tiling => (icons::tiling(false), "tiling"),
                    TileKey::GameMode => (icons::game_mode(), "game-mode"),
                    TileKey::Media => (icons::media_play_pause(false), "media"),
                    TileKey::DoNotDisturb => (icons::do_not_disturb(false), "do-not-disturb"),
                    TileKey::KeepAwake => (icons::keep_awake(false), "keep-awake"),
                    TileKey::ChargeThreshold => (icons::battery(Some(80.0), true), "charge-limit"),
                    TileKey::KeyboardBacklight => (icons::keyboard(false), "keyboard-backlight"),
                    // Handled above; unreachable here but the match must be total.
                    TileKey::Connectivity
                    | TileKey::Volume
                    | TileKey::Brightness
                    | TileKey::Microphone => (icons::applet(), "applet-name"),
                };
                Tile::new(icon, label(ftl), label(ftl))
                    .active(enabled)
                    .style(style)
                    .compact(key.shape_with(&self.config.appearance.shapes) == TileShape::Half)
                    .view(spacing)
            }
        }
    }

    /// Switches for the tiles defined by `[[custom]]` in `config.toml`.
    ///
    /// Its own titled section rather than extra rows at the end of Controls.
    /// It was tried that way and could not be found: a `caption` heading below
    /// sixteen switches reads as a footnote on the list above it, not as a
    /// place to look. The headings are how you navigate this window, so
    /// anything you are meant to find needs one.
    ///
    /// Shown even with no tiles defined, because this is also where you find
    /// out the feature exists. Hiding it until you already have a tile means
    /// only people who read `config.toml` ever discover it.
    fn custom_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let mut section = column::with_capacity(self.config.custom.len() + 2)
            .spacing(spacing)
            .push(text::title4(fl!("settings-custom")))
            .push(text::caption(fl!("settings-custom-detail")));

        for (index, tile) in self.config.custom.iter().enumerate() {
            // Say what it runs. A tile called "Screenshot" is obvious; one
            // called "Work" is not, and the command is the only thing that
            // distinguishes two tiles with the same name.
            let labels = column::with_capacity(2)
                .push(text::body(tile.name.clone()))
                .push(text::caption(tile.command.join(" ")));

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

        // Straight to the file, since that is where tiles are added and typing
        // a path out for someone to paste is a poor substitute for opening it.
        section = section.push(
            row::with_capacity(2)
                .push(cosmic::widget::Space::new().width(Length::Fill))
                .push(
                    button::standard(fl!("open-config-folder")).on_press(Message::OpenConfigFolder),
                ),
        );

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

        let mut section = column::with_capacity(6)
            .spacing(spacing)
            .push(text::title4(fl!("settings-icon")))
            .push(text::caption(fl!("settings-icon-detail")));

        // Every choice is a tile showing the glyph it would put on the panel,
        // including "system" and "custom". A radio button next to a row of
        // previews made two of the six options look like a different kind of
        // thing, and the one that matters — what it actually looks like — was
        // the one a radio could not show.
        let mut tiles = row::with_capacity(icons::PRESETS.len() + 2).spacing(spacing);

        tiles = tiles.push(self.icon_tile(
            icons::panel_handle(&PanelIcon::System, PREVIEW_ICON),
            fl!("icon-system"),
            matches!(current, PanelIcon::System),
            Message::SetIcon(PanelIcon::System),
        ));

        for (preset, label) in icons::PRESETS {
            tiles = tiles.push(self.icon_tile(
                icon::from_name(icons::preset_name(preset)).into(),
                crate::i18n::lookup(label, None),
                matches!(current, PanelIcon::Preset(chosen) if chosen == preset),
                Message::SetIcon(PanelIcon::Preset(preset.to_string())),
            ));
        }

        // The custom tile previews whatever is chosen, so a path that no longer
        // resolves shows as the fallback glyph here rather than only on the
        // panel.
        let custom = PanelIcon::Custom(self.custom_input.clone());
        tiles = tiles.push(self.icon_tile(
            icons::panel_handle(&custom, PREVIEW_ICON),
            fl!("icon-custom"),
            matches!(current, PanelIcon::Custom(_)),
            Message::SelectIcon,
        ));

        section = section.push(tiles);

        // Choosing a file is the main path; the field stays for icon-theme
        // names, which no file chooser can offer.
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
                ))
                .push(button::standard(fl!("icon-select")).on_press(Message::SelectIcon)),
        );

        section = section.push(text::caption(fl!("icon-copied-detail")));

        section.into()
    }

    /// One choice in the panel-icon row: a preview, a label, and a pressed
    /// state that reads the same as the tiles in the popup itself.
    fn icon_tile(
        &self,
        handle: icon::Handle,
        label: String,
        selected: bool,
        message: Message,
    ) -> Element<'_, Message> {
        let spacing = self.spacing();
        button::custom(
            column::with_capacity(2)
                .align_x(Alignment::Center)
                .spacing(spacing / 2)
                .push(icon::icon(handle).size(PREVIEW_ICON))
                .push(text::caption(label)),
        )
        .padding(spacing)
        .class(if selected {
            button::ButtonClass::Suggested
        } else {
            button::ButtonClass::Standard
        })
        .on_press(message)
        .into()
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

        let tabs = segmented_button::Model::builder()
            .insert(|entry| entry.text(fl!("tab-tiles")).data(Tab::Tiles).activate())
            .insert(|entry| entry.text(fl!("tab-styling")).data(Tab::Styling))
            .insert(|entry| entry.text(fl!("tab-about")).data(Tab::About))
            .build();

        let about = cosmic::widget::about::About::default()
            .name(fl!("applet-name"))
            .icon(icon::from_name(APP_ICON))
            .version(env!("CARGO_PKG_VERSION"))
            .author(fl!("about-author"))
            .comments(fl!("about-comments"))
            .license("MIT OR Apache-2.0")
            .links([
                (fl!("about-source"), REPOSITORY),
                (
                    fl!("about-issues"),
                    concat!(env!("CARGO_PKG_REPOSITORY"), "/issues"),
                ),
            ]);

        let settings = Self {
            core,
            config,
            custom_input,
            error: None,
            tabs,
            about,
            pressed: None,
            dragging: false,
            hovered: None,
            working_order: Vec::new(),
        };

        // Set both titles right here, synchronously — the InitTitle Task path
        // never fired, and the async example still calls set_header_title in
        // init anyway.
        let mut settings = settings;
        settings.set_header_title(fl!("settings-window-title"));
        eprintln!(
            "[settings] init: header_title={:?}, main_window_id={:?}",
            settings.core.window.header_title,
            settings.core.main_window_id()
        );
        let title = match settings.core.main_window_id() {
            Some(id) => settings.set_window_title(fl!("settings-window-title"), id),
            None => Task::none(),
        };
        (settings, title)
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tab(entity) => self.tabs.activate(entity),
            Message::Noop => {}
            Message::PressTile(key) => {
                // The *full* order, not the drawn subset — see `full_order`.
                self.working_order = self.full_order();
                self.pressed = Some(key);
                self.dragging = false;
            }
            Message::ReleaseTile => {
                let Some(key) = self.pressed.take() else {
                    // A release with no press behind it: the press began
                    // somewhere else, or was already resolved.
                    return Task::none();
                };
                if self.dragging {
                    self.config.appearance.order = std::mem::take(&mut self.working_order);
                } else {
                    // A plain press selects or deselects.
                    let module = preview_module_key(key);
                    let enabled = self.module_enabled(module);
                    self.set_module(module, !enabled);
                    self.working_order.clear();
                }
                self.dragging = false;
                self.save();
            }
            Message::ExitTile => self.hovered = None,
            Message::HoverTile(over) => {
                self.hovered = Some(over);
                // Only a shown tile can be dragged, and only onto another shown
                // tile — the hidden grid is not part of the layout being
                // edited. `on_enter` is only wired on shown tiles, but the
                // pressed one could have been hidden by a tap a moment ago.
                let shown = |k: TileKey| self.module_enabled(preview_module_key(k));
                if let Some(picked) = self.pressed {
                    if picked != over && shown(picked) && shown(over) {
                        // Entering a different tile is what makes this a drag
                        // rather than a tap.
                        self.dragging = true;
                        let order = &mut self.working_order;
                        if let (Some(from), Some(to)) = (
                            order.iter().position(|&k| k == picked),
                            order.iter().position(|&k| k == over),
                        ) {
                            let moved = order.remove(from);
                            order.insert(to, moved);
                        }
                    }
                }
            }
            Message::Present => {
                // Raise and focus, rather than opening a second window. The
                // window may be behind something or on another workspace, so
                // doing nothing here would look like the right-click was
                // ignored.
                if let Some(id) = self.core.main_window_id() {
                    return cosmic::iced::window::gain_focus(id);
                }
            }
            Message::OpenConfigFolder => open_config_folder(),
            Message::OpenUrl(url) => open_url(&url),
            Message::SelectIcon => {
                return Task::perform(choose_icon(), |path| match path {
                    Some(path) => cosmic::action::app(Message::IconSelected(path)),
                    // Cancelled, or no portal on this desktop. Neither is worth
                    // interrupting the user over.
                    None => cosmic::action::none(),
                });
            }
            Message::IconSelected(path) => match adopt_icon(&path) {
                Ok(stored) => {
                    self.custom_input = stored.to_string_lossy().into_owned();
                    self.config.appearance.icon = PanelIcon::Custom(self.custom_input.clone());
                    self.save();
                }
                Err(err) => self.error = Some(err),
            },
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
        // A later `--settings` invocation asking this window to come forward.
        let present = Subscription::run_with((), |()| {
            futures::stream::unfold(crate::single_instance::requests(), |requests| async move {
                let mut receiver = requests?;
                // `None` ends the stream: the sender is gone, which cannot
                // happen while this process lives, but ending is the right
                // answer if it ever does.
                receiver
                    .recv()
                    .await
                    .map(|()| (Message::Present, Some(receiver)))
            })
        });

        present
    }

    fn view(&self) -> Element<'_, Message> {
        let spacing = self.spacing();

        let page: Element<'_, Message> = match self.tabs.active_data::<Tab>() {
            Some(Tab::Styling) => column::with_capacity(3)
                .spacing(spacing * 2)
                .push(self.style_section())
                .push(divider::horizontal::default())
                .push(self.icon_section())
                .into(),
            Some(Tab::About) => {
                cosmic::widget::about::about(&self.about, |url| Message::OpenUrl(url.to_string()))
            }
            // Tiles, and the fallback: an empty model would otherwise show a
            // blank window rather than the page people open this for.
            _ => column::with_capacity(3)
                .spacing(spacing * 2)
                .push(self.preview_section())
                .push(divider::horizontal::default())
                .push(self.custom_section())
                .into(),
        };

        let mut body = column::with_capacity(6)
            .spacing(spacing * 2)
            .padding(spacing * 2)
            .push(
                // `TabBar` is the underline-under-the-active-tab style. The
                // default `Control` shows a tick next to the active label,
                // which is right for a pick-one set and wrong for tabs.
                cosmic::widget::segmented_control::horizontal(&self.tabs)
                    .style(cosmic::theme::SegmentedButton::TabBar)
                    .on_activate(Message::Tab),
            )
            .push(page);

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

        // Fill, not the fixed opening width: the window can be resized, and a
        // fixed-width container inside a wider window left the extra space
        // empty on one side and clipped the scrollable on the other.
        container(scrollable(body))
            .width(Length::Fill)
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
