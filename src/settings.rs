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

use crate::config::{BadgeSide, BadgeTimeout, Config, PanelIcon, TileFinish, TileStyle};
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

/// Lay a red remove button over a tile's top-right corner when `show` is set.
///
/// The grid is what you edit and the palette is only ever a source, so removal
/// lives here rather than as a second mode on the palette: you take a tile off
/// the grid by pressing the − on that tile.
///
/// A real button rather than a `mouse_area` handler, so its press wins over
/// the drag gesture underneath instead of racing it.
fn with_remove_button(
    tile: Element<'_, Message>,
    show: bool,
    index: usize,
    spacing: u16,
) -> Element<'_, Message> {
    if !show {
        return tile;
    }

    let minus =
        cosmic::widget::button::icon(cosmic::widget::icon::from_name("list-remove-symbolic"))
            .icon_size(HANDLE_ICON)
            .class(cosmic::theme::Button::Destructive)
            .on_press(Message::RemoveInstance(index));

    let overlay = container(minus)
        .padding(spacing / 2)
        .width(Length::Fill)
        .align_x(cosmic::iced::Alignment::End)
        .align_y(cosmic::iced::Alignment::Start);

    cosmic::iced::widget::stack![tile, overlay].into()
}

/// The remove button's glyph size — smaller than a tile's own icon, because
/// it is an affordance on top of the content rather than part of it.
const HANDLE_ICON: u16 = 14;

/// A preview tile dressed for its state.
///
/// Three states, and they have to stay visually distinct:
///
/// * **Selected** — drawn plainly. It is in the grid; nothing to say.
/// * **Not selected** — a plain outline. Now that hidden tiles live in their
///   own "Not shown" grid the heading carries most of the meaning, but the
///   tiles still need to read as inert at a glance. Not accented, because
///   the accent already means "this control is on" inside the tile, and
///   reusing it would make a switched-off Wi-Fi tile and an excluded one
///   look the same. (iced draws no dashed borders; an earlier note here said
///   "dashed" and was wrong.)
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

    // A container's background paints *under* its child, not over it, so a
    // "wash" here cannot mute the tile — a 0.55 wash was tried and on a dark
    // theme the hidden tiles were indistinguishable from the shown ones.
    // What a container can do is outline, so the inert state is an outline
    // strong enough to see: half-strength foreground, two pixels.
    container(tile)
        .class(cosmic::theme::Container::Custom(Box::new(|theme| {
            let cosmic = theme.cosmic();
            let mut edge = cosmic.on_bg_color();
            edge.alpha = 0.5;
            cosmic::widget::container::Style {
                border: cosmic::iced::Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    width: 2.0,
                    color: cosmic::iced::Color::from(edge),
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
    /// The layout index of the instance being dragged, if the button is
    /// still held.
    picked: Option<usize>,
    /// The instance under the pointer, so it can show its remove button.
    hovered: Option<usize>,
    /// The cell the pointer is currently over while dragging.
    target: Option<(u16, u16)>,
    /// A cell that refused a drop, flashed so the refusal is visible.
    ///
    /// Collisions refuse rather than push: nothing the user did not drag ever
    /// moves. A refusal with no feedback just looks like a dropped gesture.
    refused: Option<(u16, u16)>,
    /// The layout being edited while a tile is held. Committed on a legal
    /// drop, discarded otherwise.
    working: Vec<crate::tile_layout::Instance>,
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
    /// Pointer down on a placed tile: start dragging that layout index.
    PickInstance(usize),
    /// The pointer is over this 0-based cell while a tile is held.
    DragToCell(u16, u16),
    /// Pointer up. Commits the move if the cell is free, else snaps back and
    /// flashes the cell that refused it.
    DropInstance,
    /// Remove this instance from the layout.
    RemoveInstance(usize),
    /// Add a control at the first free cell.
    AddControl(crate::tile_layout::Control, TileShape),
    /// Flip one of the non-tile controls on the Styling tab.
    ToggleExtra(&'static str, bool),
    /// The pointer entered (`Some`) or left (`None`) a placed tile.
    HoverInstance(Option<usize>),
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
    SetFinish(TileFinish),
    SetBatteryIndicator(bool),
    SetWifiIndicator(bool),
    SetWifiTimeout(BadgeTimeout),
    SetBadgeSide(BadgeSide),
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
    /// Write the edited layout back, validated, and save.
    ///
    /// `validate` here is belt-and-braces: every edit path already asks
    /// `free_at` first. It costs nothing and means no code path can put an
    /// overlap in the file.
    fn commit(&mut self) {
        let edited = std::mem::take(&mut self.working);
        self.config.appearance.layout = crate::tile_layout::validate(&edited);
        self.target = None;
        self.save();
    }

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
        let mut section = column::with_capacity(5)
            .spacing(spacing)
            .push(text::title4(fl!("settings-controls")))
            .push(text::caption(fl!("settings-preview-detail")));

        let layout = self.layout();

        // One element per instance, at the instance's own cell. A control
        // placed twice is drawn twice — there is no "the Battery tile" here
        // any more, only the instances of it.
        let mut tiles: Vec<(Element<'_, Message>, crate::tile_layout::Slot)> =
            Vec::with_capacity(layout.len());
        for (index, instance) in layout.iter().copied().enumerate() {
            let held = self.picked == Some(index);
            let tile = self.preview_tile(instance.control, true, theme_spacing);
            let tile = preview_frame(tile, true, held);
            // The remove affordance is on the tile, because the tile is what
            // you are removing. The palette only ever adds.
            let tile = with_remove_button(tile, self.hovered == Some(index), index, spacing);
            let area = cosmic::widget::mouse_area(tile)
                .on_press(Message::PickInstance(index))
                .on_release(Message::DropInstance)
                .on_enter(Message::HoverInstance(Some(index)))
                .on_exit(Message::HoverInstance(None))
                .interaction(if held {
                    cosmic::iced::mouse::Interaction::Grabbing
                } else {
                    cosmic::iced::mouse::Interaction::Grab
                });
            // Entering an occupied tile while dragging aims at *its* cell,
            // which is never free — so the drop is refused there rather than
            // silently landing somewhere else.
            let area = area.on_move(move |_| Message::DragToCell(instance.col, instance.row));
            tiles.push((area.into(), instance.slot()));
        }

        // Gaps are the drop targets, so Settings builds them itself.
        let refused = self.refused;
        let dragging = self.picked.is_some();
        let ghosts = crate::ui::Ghosts::Custom(Box::new(move |col, row| {
            let slot = crate::ui::ghost_slot(refused == Some((col, row)), theme_spacing);
            cosmic::widget::mouse_area(slot)
                .on_move(move |_| Message::DragToCell(col, row))
                .on_release(Message::DropInstance)
                .interaction(if dragging {
                    cosmic::iced::mouse::Interaction::Grabbing
                } else {
                    cosmic::iced::mouse::Interaction::default()
                })
                .into()
        }));

        section = section.push(tile_grid(tiles, ghosts, theme_spacing));

        // Anything not on the grid — a built-in control or one of the user's
        // own tiles — sits underneath, where tapping adds it at the first free
        // space. The palette replaces this wholesale later.
        let absent: Vec<(crate::tile_layout::Control, TileShape)> = self
            .placeable()
            .into_iter()
            .filter(|(control, _)| !layout.iter().any(|i| i.control == *control))
            .collect();
        if !absent.is_empty() {
            let mut available: Vec<(Element<'_, Message>, TileShape)> =
                Vec::with_capacity(absent.len());
            for (control, shape) in absent {
                let tile = preview_frame(
                    self.preview_tile(control, false, theme_spacing),
                    false,
                    false,
                );
                let area = cosmic::widget::mouse_area(tile)
                    .on_press(Message::AddControl(control, shape))
                    .interaction(cosmic::iced::mouse::Interaction::Pointer);
                available.push((area.into(), shape));
            }
            section = section
                .push(text::title4(fl!("settings-hidden")))
                .push(text::caption(fl!("settings-hidden-detail")))
                .push(tile_grid(
                    slotted(available),
                    crate::ui::Ghosts::Empty,
                    theme_spacing,
                ));
        }
        section.into()
    }

    /// The layout being drawn: the one mid-edit if a tile is held, else the
    /// saved one.
    ///
    /// A cancelled or refused move therefore leaves the file exactly as it
    /// was, rather than half-applied.
    fn layout(&self) -> Vec<crate::tile_layout::Instance> {
        if self.picked.is_some() && !self.working.is_empty() {
            return self.working.clone();
        }
        self.config.appearance.layout.clone()
    }

    /// Everything that can go on the grid, in the order it is offered: the
    /// built-in controls first, then the user's own tiles.
    ///
    /// Each carries the shape it would be added at — a control's default, and
    /// for a custom tile whatever its `[[custom]]` entry asks for, so adding a
    /// launcher back puts it back the size it was.
    fn placeable(&self) -> Vec<(crate::tile_layout::Control, TileShape)> {
        use crate::tile_layout::Control;

        let builtin = crate::tile_layout::DEFAULT_ORDER
            .iter()
            .copied()
            .filter(|&k| crate::tile_layout::is_placeable(k))
            .map(|k| (Control::Builtin(k), k.default_shape()));

        let custom = self
            .config
            .custom
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.enabled)
            .map(|(index, entry)| (Control::custom(index), entry.shape));

        builtin.chain(custom).collect()
    }

    fn preview_tile(
        &self,
        control: crate::tile_layout::Control,
        enabled: bool,
        spacing: Spacing,
    ) -> Element<'_, Message> {
        use crate::tile_layout::Control;

        let style = self.config.appearance.style;
        let finish = self.config.appearance.finish;
        let label = |ftl: &str| crate::i18n::lookup(ftl, None);

        // A user's own tile previews as itself: its icon, its name, and no
        // press, because in here it is a thing being arranged rather than a
        // command to run.
        let key = match control {
            Control::Builtin(key) => key,
            Control::Custom { custom } => {
                let Some(entry) = self.config.custom.get(custom) else {
                    return crate::ui::ghost_tile(spacing);
                };
                return Tile::new(
                    icons::resolve_owned(&entry.icon),
                    entry.name.clone(),
                    entry.detail.clone().unwrap_or_else(|| entry.name.clone()),
                )
                .style(style)
                .finish(finish)
                .compact(entry.shape == TileShape::Half)
                .wide(entry.shape == TileShape::Wide)
                .view(spacing);
            }
        };

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
                connectivity_tile(
                    rows,
                    crate::ui::tall_height(spacing),
                    style,
                    finish,
                    spacing,
                )
            }
            TileKey::Volume => wide_slider_tile(
                icons::volume(60.0, false),
                label("volume"),
                60.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                crate::ui::Look::new(self.config.appearance.finish, spacing),
            ),
            TileKey::Brightness => wide_slider_tile(
                icons::brightness(70.0, false),
                label("brightness"),
                70.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                crate::ui::Look::new(self.config.appearance.finish, spacing),
            ),
            TileKey::Microphone => wide_slider_tile(
                icons::microphone(50.0, false),
                label("microphone"),
                50.0,
                |_| Message::Noop,
                None,
                SliderMode::Inert,
                crate::ui::Look::new(self.config.appearance.finish, spacing),
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
                    .finish(finish)
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

        section = section.push(text::title4(fl!("settings-finish")));
        section = section.push(text::caption(fl!("settings-finish-detail")));
        for finish in TileFinish::ALL {
            section = section.push(
                column::with_capacity(2)
                    .push(radio(
                        text::body(crate::i18n::lookup(finish.l10n_key(), None)),
                        finish,
                        Some(self.config.appearance.finish),
                        Message::SetFinish,
                    ))
                    .push(text::caption(crate::i18n::lookup(
                        finish.description_key(),
                        None,
                    ))),
            );
        }

        section.into()
    }

    /// Badges beside the panel button.
    fn indicators_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let theme_spacing = Spacing::from_theme(self.core.system_theme());
        let indicators = &self.config.indicators;
        let mut section = column::with_capacity(10)
            .spacing(spacing)
            .push(text::title4(fl!("settings-indicators")))
            .push(text::caption(fl!("settings-indicators-detail")));

        section = section.push(crate::ui::toggle_row(
            "battery-low-symbolic",
            fl!("indicator-battery"),
            Some(fl!(
                "indicator-battery-detail",
                percent = i64::from(indicators.battery_low_percent)
            )),
            indicators.battery_low,
            Some(Message::SetBatteryIndicator(!indicators.battery_low)),
            theme_spacing,
        ));

        section = section.push(crate::ui::toggle_row(
            "network-wireless-offline-symbolic",
            fl!("indicator-wifi"),
            Some(fl!("indicator-wifi-detail")),
            indicators.wifi_disconnected,
            Some(Message::SetWifiIndicator(!indicators.wifi_disconnected)),
            theme_spacing,
        ));

        // Only worth asking how long it stays once something can put it up.
        if indicators.wifi_disconnected {
            section = section.push(text::caption(fl!("settings-indicator-timeout")));
            for timeout in BadgeTimeout::ALL {
                section = section.push(radio(
                    text::body(crate::i18n::lookup(timeout.l10n_key(), None)),
                    timeout,
                    Some(indicators.wifi_timeout),
                    Message::SetWifiTimeout,
                ));
            }
        }

        // And only worth asking which side once something can appear there.
        if indicators.battery_low || indicators.wifi_disconnected {
            section = section.push(text::caption(fl!("settings-indicator-side")));
            for side in BadgeSide::ALL {
                section = section.push(radio(
                    text::body(crate::i18n::lookup(side.l10n_key(), None)),
                    side,
                    Some(indicators.side),
                    Message::SetBadgeSide,
                ));
            }
        }

        section.into()
    }

    /// The controls that are not grid tiles, as plain switches.
    ///
    /// Derived selection replaced the `[modules]` switch list for everything
    /// that can be placed — you show a control by putting it on the grid. The
    /// three that have no tile still need somewhere to live, so they keep a
    /// switch, here rather than beside a grid they are not part of.
    fn extras_section(&self) -> Element<'_, Message> {
        let spacing = self.spacing();
        let theme_spacing = Spacing::from_theme(self.core.system_theme());
        let mut section = column::with_capacity(5)
            .spacing(spacing)
            .push(text::title4(fl!("settings-extras")))
            .push(text::caption(fl!("settings-extras-detail")));

        for (key, ftl, icon) in [
            ("media", "media", "multimedia-player-symbolic"),
            ("gamemode", "game-mode", "applications-games-symbolic"),
            ("charge_threshold", "charge-limit", "battery-symbolic"),
        ] {
            let on = self.module_enabled(key);
            section = section.push(crate::ui::toggle_row(
                icon,
                crate::i18n::lookup(ftl, None),
                None,
                on,
                Some(Message::ToggleExtra(key, !on)),
                theme_spacing,
            ));
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
            picked: None,
            hovered: None,
            target: None,
            refused: None,
            working: Vec::new(),
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
            Message::PickInstance(index) => {
                self.working = self.config.appearance.layout.clone();
                self.picked = Some(index);
                self.target = None;
                self.refused = None;
            }
            Message::HoverInstance(over) => self.hovered = over,
            Message::DragToCell(col, row) => {
                if self.picked.is_some() {
                    self.target = Some((col, row));
                    // Moving again clears a stale refusal, so the flash
                    // belongs to the cell the pointer is on now.
                    self.refused = None;
                }
            }
            Message::DropInstance => {
                let Some(index) = self.picked.take() else {
                    // A release with no pick behind it: the press began
                    // somewhere else, or was already resolved.
                    return Task::none();
                };
                let (Some((col, row)), Some(held)) =
                    (self.target.take(), self.working.get(index).copied())
                else {
                    self.working.clear();
                    return Task::none();
                };
                if (col, row) == (held.col, held.row) {
                    // Picked up and put back down. Not a move, not a refusal.
                    self.working.clear();
                    return Task::none();
                }
                // The held tile's own cells are free for it to move within,
                // so it is taken out of the layout before the question is
                // asked — otherwise a one-cell nudge would collide with itself.
                let mut rest = self.working.clone();
                rest.remove(index);
                if crate::tile_layout::free_at(&rest, held.shape, col, row) {
                    self.working[index].col = col;
                    self.working[index].row = row;
                    self.commit();
                } else {
                    self.refused = Some((col, row));
                    self.working.clear();
                }
            }
            Message::RemoveInstance(index) => {
                if index < self.config.appearance.layout.len() {
                    self.working = self.config.appearance.layout.clone();
                    self.working.remove(index);
                    self.hovered = None;
                    self.commit();
                }
            }
            Message::ToggleExtra(key, on) => {
                self.set_module(key, on);
                self.save();
            }
            Message::AddControl(control, shape) => {
                self.working = self.config.appearance.layout.clone();
                let (col, row) = crate::tile_layout::first_free(&self.working, shape);
                self.working
                    .push(crate::tile_layout::Instance::new(control, shape, col, row));
                self.commit();
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
            Message::SetFinish(finish) => {
                self.config.appearance.finish = finish;
                self.save();
            }
            Message::SetBatteryIndicator(on) => {
                self.config.indicators.battery_low = on;
                self.save();
            }
            Message::SetWifiIndicator(on) => {
                self.config.indicators.wifi_disconnected = on;
                self.save();
            }
            Message::SetWifiTimeout(timeout) => {
                self.config.indicators.wifi_timeout = timeout;
                self.save();
            }
            Message::SetBadgeSide(side) => {
                self.config.indicators.side = side;
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
            Some(Tab::Styling) => column::with_capacity(7)
                .spacing(spacing * 2)
                .push(self.style_section())
                .push(divider::horizontal::default())
                .push(self.icon_section())
                .push(divider::horizontal::default())
                .push(self.indicators_section())
                .push(divider::horizontal::default())
                .push(self.extras_section())
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

/// Bridge the preview's ordered (element, shape) list onto the position-driven
/// grid renderer, by packing it the way 0.1.6 did.
///
/// Temporary: this page is rebuilt around free placement next, at which point
/// the slots come from the layout itself and this goes away.
fn slotted<'a, Msg>(
    tiles: Vec<(Element<'a, Msg>, TileShape)>,
) -> Vec<(Element<'a, Msg>, crate::tile_layout::Slot)> {
    let shapes: Vec<TileShape> = tiles.iter().map(|(_, shape)| *shape).collect();
    let slots = crate::tile_layout::packed_slots(&shapes);
    tiles
        .into_iter()
        .map(|(element, _)| element)
        .zip(slots)
        .collect()
}
