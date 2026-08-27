//! Shared widgets and the popup's spacing policy.
//!
//! Nothing here picks a colour, radius or font size by hand — everything comes
//! from the active COSMIC theme, so the popup follows the system light/dark
//! setting and accent colour without knowing which is in use.
//!
//! # One place sets padding
//!
//! Every interactive widget in this file sets its padding **explicitly**, and
//! nothing wraps a padded container inside a padded button. That rule exists
//! because breaking it produced a real bug: a tile had `padding(space_xs)` on an
//! inner container while the button kept its own default `Padding::new(5)`, so a
//! 52px tile spent 34px on padding and left 18px for a 20px icon. The icon was
//! clipped, and nothing in the code said 52 had to be bigger than anything.
//!
//! So heights are *derived* from what they contain ([`tile_height`]), never
//! written down as a number that can quietly stop being big enough.

pub mod icons;

use cosmic::iced::{Alignment, Background, Border, Color, Length, Padding};
use cosmic::widget::{
    button, container, icon, progress_bar, row, scrollable, slider, text, toggler, tooltip,
};
use cosmic::{theme, Element};

use crate::config::{TileFinish, TileStyle};
use crate::tile_layout::Slot;

/// Icon size inside a tile and a list row.
pub const ICON_SIZE: u16 = 20;

/// The popup's spacing scale, read once from the theme.
///
/// COSMIC ships three densities (compact / standard / spacious) with different
/// values for the same token, so these are looked up rather than assumed.
#[derive(Debug, Clone, Copy)]
pub struct Spacing {
    /// Gap between items in a row or column.
    pub gap: u16,
    /// Inside a tile or list row, vertically.
    pub pad_y: u16,
    /// Inside a tile or list row, horizontally.
    pub pad_x: u16,
    /// Between sections of a page.
    pub section: u16,
}

impl Spacing {
    pub fn from_theme(theme: &cosmic::Theme) -> Self {
        let spacing = theme.cosmic().spacing;
        Self {
            gap: spacing.space_xxs,
            pad_y: spacing.space_xxs,
            pad_x: spacing.space_xs,
            section: spacing.space_xs,
        }
    }

    fn padding(self) -> Padding {
        Padding::from([self.pad_y, self.pad_x])
    }
}

/// Height of a single-line tile: its icon plus its own padding, plus room for
/// the icon base that [`TileStyle::Medium`] draws around it.
///
/// Derived rather than hardcoded — see the note at the top of this module. The
/// base is included unconditionally so that switching tile style does not
/// change the grid's geometry: the rows stay put and only the colouring moves.
pub fn tile_height(spacing: Spacing) -> f32 {
    // A single line of icon-plus-text — the floor.
    let one_line =
        f32::from(ICON_SIZE) + f32::from(spacing.pad_y) * 2.0 + icon_base_padding(spacing) * 2.0;

    // But a Tall tile is exactly two of these plus a gap, and the Connectivity
    // tile has to fit three rows inside that. Solving for the Small height that
    // makes `tall_height` hold three rows at their natural size is what sets
    // the grid's row height — which is why every tile got a little taller when
    // the group went from two columns to one. The alternative was cramming the
    // rows, and a row shorter than its own icon is not a row.
    let three_rows = connectivity_row_height(spacing) * 3.0
        + connectivity_row_gap(spacing) * 2.0
        + f32::from(spacing.pad_y) * 2.0;
    let from_rows = (three_rows - f32::from(spacing.gap)) / 2.0;

    one_line.max(from_rows)
}

/// Height of one row inside the Connectivity tile: its icon on its base.
pub fn connectivity_row_height(spacing: Spacing) -> f32 {
    f32::from(ICON_SIZE) + icon_base_padding(spacing) * 2.0
}

/// Vertical gap between Connectivity rows — tighter than the grid's, because
/// three rows are one control, not three neighbours.
pub fn connectivity_row_gap(spacing: Spacing) -> f32 {
    f32::from(spacing.gap) / 2.0
}

/// Padding inside the medium style's icon base.
fn icon_base_padding(spacing: Spacing) -> f32 {
    f32::from(spacing.pad_y / 2)
}

/// Characters of state text a tile shows before eliding.
///
/// Sized against the real tile width: the popup is [`POPUP_WIDTH`] wide with
/// page padding either side, split into two columns with a gap, then each tile
/// spends its own horizontal padding and the icon plus a gap. What is left is
/// roughly 14 characters at the body font.
///
/// [`POPUP_WIDTH`]: crate::app::POPUP_WIDTH
const MAX_STATE_CHARS: usize = 14;

/// What a Wide tile can show instead. Four sub-columns rather than two, so
/// roughly twice the room before anything has to be cut.
const MAX_WIDE_STATE_CHARS: usize = 30;

/// A grid tile: an icon and the thing's current state.
pub struct Tile<'a, Msg> {
    /// Icon only — the Half shape. Name and state move into the tooltip,
    /// because fourteen characters do not fit in half a tile and a clipped
    /// "Balan" is worse than nothing.
    compact: bool,
    icon_name: std::borrow::Cow<'a, str>,
    state: String,
    /// Full name of the control, shown on hover. The tile deliberately has no
    /// visible name label — this is where the discoverability goes instead.
    name: String,
    active: bool,
    style: TileStyle,
    finish: TileFinish,
    /// The Wide shape: the same one line, with room to say more.
    wide: bool,
    /// A second, quieter fact shown at the trailing edge of a Wide tile —
    /// the power profile beside the charge, the servers beside the provider.
    /// Ignored unless `wide`, because a Small tile has nowhere to put it.
    detail: Option<String>,
    on_press: Option<Msg>,
}

impl<'a, Msg: Clone + 'static> Tile<'a, Msg> {
    pub fn new(
        icon_name: impl Into<std::borrow::Cow<'a, str>>,
        name: impl Into<String>,
        state: impl Into<String>,
    ) -> Self {
        Self {
            icon_name: icon_name.into(),
            state: state.into(),
            name: name.into(),
            active: false,
            style: TileStyle::default(),
            finish: TileFinish::default(),
            wide: false,
            detail: None,
            on_press: None,
            compact: false,
        }
    }

    /// How strongly to signal the on state. See [`TileStyle`].
    pub fn style(mut self, style: TileStyle) -> Self {
        self.style = style;
        self
    }

    /// Draw the Wide form: one line still, but with room for a longer state
    /// and a trailing detail.
    pub fn wide(mut self, wide: bool) -> Self {
        self.wide = wide;
        self
    }

    /// The quieter fact at the trailing edge. Only drawn when [`Self::wide`].
    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// How the tile's own surface is painted. See [`TileFinish`].
    pub fn finish(mut self, finish: TileFinish) -> Self {
        self.finish = finish;
        self
    }

    /// Whether the tile reads as "on" — drives the accent fill.
    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn on_press(mut self, msg: Msg) -> Self {
        self.on_press = Some(msg);
        self
    }

    /// Draw icon-only, for the Half shape.
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Set the press action, or leave the tile inert.
    ///
    /// For controls that exist but cannot be acted on right now — Game Mode
    /// held by a running game, Keep Awake held by another program.
    pub fn on_press_maybe(mut self, msg: Option<Msg>) -> Self {
        self.on_press = msg;
        self
    }

    pub fn view(self, spacing: Spacing) -> Element<'a, Msg> {
        // `Low` never shows an on state on the grid at all. Selection lives
        // inside the drill-down pages instead, which is how Battery has always
        // behaved: you learn which power profile is active by opening it.
        let signalled = self.active && self.style != TileStyle::Low;

        let icon_name = self.icon_name.into_owned();
        // Always the base box, visible only under Medium — see `icon_base`.
        let glyph: Element<'a, Msg> = icon_base(
            icon_name,
            signalled,
            self.style == TileStyle::Medium,
            spacing,
        );

        let content: Element<'a, Msg> = if self.compact {
            // Centred glyph, nothing else. The name and state are in the
            // tooltip below.
            container(glyph)
                .width(Length::Fill)
                .align_x(Alignment::Center)
                .into()
        } else {
            let limit = if self.wide {
                MAX_WIDE_STATE_CHARS
            } else {
                MAX_STATE_CHARS
            };
            let mut line = row::with_capacity(3)
                .align_y(Alignment::Center)
                .spacing(spacing.gap)
                .push(glyph)
                .push(
                    text::body(truncate(&self.state, limit))
                        .width(Length::Fill)
                        // Tiles are a fixed height sized for one line. Without
                        // this a long state string wraps, and the tile's
                        // content is squeezed and sits out of line with its
                        // neighbour. Character truncation alone is not enough:
                        // it counts characters, and what overflows is pixels.
                        .wrapping(cosmic::iced::widget::text::Wrapping::None),
                );
            // The detail sits at the trailing edge, in caption weight: it is
            // the thing you read second. `Fill` is already spent on the state,
            // so this takes only what it needs and the state gives way first.
            if let Some(detail) = self.detail.filter(|_| self.wide) {
                line = line.push(
                    text::caption(truncate(&detail, MAX_STATE_CHARS))
                        .wrapping(cosmic::iced::widget::text::Wrapping::None),
                );
            }
            line.into()
        };

        // Only `High` tints the whole tile. `Medium` carries the signal on the
        // icon's base instead, so the tile itself must stay neutral or the two
        // would compete.
        let filled = signalled && self.style == TileStyle::High;

        let tile = button::custom(content)
            // Set once, here. Never also on a child.
            .padding(spacing.padding())
            .class(if filled {
                button::ButtonClass::Suggested
            } else {
                tile_button_class(self.finish)
            })
            .width(Length::Fill)
            .height(Length::Fixed(tile_height(spacing)));

        let tile = match self.on_press {
            Some(msg) => tile.on_press(msg),
            None => tile,
        };

        // The name lives in the tooltip because the tile shows state, not name.
        // Without this, a user who does not recognise an icon has no way to
        // find out what it is. A compact tile shows neither, so its tooltip
        // carries both.
        let hint = if self.compact && !self.state.is_empty() && self.state != self.name {
            format!("{} — {}", self.name, self.state)
        } else {
            self.name
        };
        tooltip(tile, text::body(hint), tooltip::Position::Top).into()
    }
}

/// The colour a tile's card is painted in.
///
/// This is COSMIC's *component on the background layer* — what the system
/// paints any card sitting on a background with. Two properties matter here:
///
/// * It is **frost-aware**: alpha follows the `frosted_*` theme flags (1.0
///   with frost off, ~0.78 with it on), so a tile sits on the popup's glass
///   the way every other COSMIC surface does. `cosmic.button.base`, which
///   this replaced, is a flat 25% wash whatever the frost setting — and a
///   popup tiled edge to edge with it read as an extra film over the blur
///   rather than as cards on glass.
/// * It is **not** `theme::Container::Primary`. That is the *layer* colour,
///   which under frosted glass is the popup's own background, so tiles drawn
///   in it read as holes punched in the popup.
///
/// Both tile kinds go through here — `Tile` is a button, the slider and
/// Connectivity tiles are containers — because the moment the two ask
/// different questions they drift, which is exactly how the sliders once
/// ended up looking like a different kind of tile.
fn tile_component(theme: &cosmic::Theme) -> cosmic::cosmic_theme::Component {
    theme
        .cosmic()
        .background(theme.transparent)
        .component
        .clone()
}

/// How much of the card colour a finish actually paints, and whether it
/// draws an edge instead.
///
/// One place, so a button tile and a container tile cannot disagree about
/// what "frosted" means.
fn finish_paint(
    _theme: &cosmic::Theme,
    finish: TileFinish,
    fill: cosmic::cosmic_theme::palette::Srgba,
) -> (Option<Color>, f32) {
    let mut fill = fill;
    match finish {
        TileFinish::Solid => (Some(Color::from(fill)), 0.0),
        TileFinish::Frosted => {
            // Denser than the popup's own glass (~0.78 of it) so the tile is
            // unmistakably a surface, but thin enough that the blur behind is
            // still the thing you see.
            fill.alpha *= FROSTED_TILE_ALPHA;
            (Some(Color::from(fill)), 0.0)
        }
        // No fill: the popup's frost runs unbroken behind every tile, and the
        // edge alone says where one tile stops and the next starts.
        TileFinish::Outline => (None, 1.0),
    }
}

/// How much of the card colour a `Frosted` tile keeps.
///
/// Chosen against the popup background, which is itself ~0.78 alpha under
/// frost: much lower and the tiles stop reading as tiles, much higher and
/// there is no visible difference from `Solid`. Raised from 0.55 once the
/// popup's blur actually worked — against a genuinely blurred backdrop the
/// thinner value left the tiles reading as smudges rather than surfaces.
const FROSTED_TILE_ALPHA: f32 = 0.68;

/// The edge a tile draws, for the finishes that have one.
fn tile_border(theme: &cosmic::Theme, width: f32) -> Border {
    let cosmic = theme.cosmic();
    let mut edge = cosmic.background(theme.transparent).component.divider;
    // The divider colour is already a faint on-surface wash; half of it again
    // keeps an outlined grid quiet enough not to look like a table.
    edge.alpha *= 0.7;
    Border {
        radius: cosmic.corner_radii.radius_s.into(),
        width,
        color: if width > 0.0 {
            Color::from(edge)
        } else {
            Color::TRANSPARENT
        },
    }
}

/// The surface every non-button tile draws on: the same fill a tile button
/// gets from [`tile_button_class`].
fn tile_surface<'a>(finish: TileFinish) -> theme::Container<'a> {
    theme::Container::Custom(Box::new(move |theme| {
        let component = tile_component(theme);
        let (fill, edge) = finish_paint(theme, finish, component.base);
        container::Style {
            background: fill.map(Background::Color),
            border: tile_border(theme, edge),
            ..Default::default()
        }
    }))
}

/// A tile button painted in the same frost-aware card colour as
/// [`tile_surface`], with the hover and pressed states from that component.
///
/// Text and icon colours are left unset so they inherit, exactly as
/// `ButtonClass::Standard` leaves them — see the note on [`quiet_button`] for
/// what happens to a class that sets them instead.
fn tile_button_class(finish: TileFinish) -> button::ButtonClass {
    fn style(
        theme: &cosmic::Theme,
        finish: TileFinish,
        fill: cosmic::cosmic_theme::palette::Srgba,
    ) -> button::Style {
        let (fill, edge) = finish_paint(theme, finish, fill);
        let border = tile_border(theme, edge);
        button::Style {
            background: fill.map(Background::Color),
            border_radius: border.radius,
            border_width: border.width,
            border_color: border.color,
            text_color: None,
            icon_color: None,
            ..button::Style::new()
        }
    }

    // An outlined tile still has to answer the pointer, so hover and pressed
    // paint their wash whatever the finish — a tile that does nothing under
    // the cursor reads as a label, not a control.
    button::ButtonClass::Custom {
        active: Box::new(move |_focused, theme| style(theme, finish, tile_component(theme).base)),
        disabled: Box::new(move |theme| style(theme, finish, tile_component(theme).disabled)),
        hovered: Box::new(move |_focused, theme| {
            style(theme, TileFinish::Solid, tile_component(theme).hover)
        }),
        pressed: Box::new(move |_focused, theme| {
            style(theme, TileFinish::Solid, tile_component(theme).pressed)
        }),
    }
}

/// The icon sitting on its own base shape, used by [`TileStyle::Medium`].
///
/// The base is always drawn, so the tile does not change shape when the control
/// turns on — only its colour. Its corner radius comes from the theme, so it
/// follows whatever roundness the system is set to.
///
/// The accent is applied as a tint rather than at full strength on purpose:
/// libcosmic exposes no way to recolour a named icon, so the glyph keeps the
/// popup's default foreground colour. A fully saturated accent behind it reads
/// poorly in both themes — near-white on light blue in dark mode, near-black on
/// blue in light mode. A tint is unmistakably the accent and keeps the glyph
/// legible.
/// `visible` is false for the High and Low styles: the base is still laid out
/// — so the icon sits in the same 24px box and the text beside it lands on
/// the same baseline whatever the style — it just draws nothing. Before this,
/// only Medium got the box and the other two drew a bare 20px icon, which
/// shifted every tile's content by a few pixels the moment the style changed.
fn icon_base<'a, Msg: 'static>(
    icon_name: String,
    active: bool,
    visible: bool,
    spacing: Spacing,
) -> Element<'a, Msg> {
    container(icon::from_name(icon_name).size(ICON_SIZE))
        .padding(spacing.pad_y / 2)
        .class(theme::Container::Custom(Box::new(move |theme| {
            let cosmic = theme.cosmic();
            let fill = if !visible {
                None
            } else if active {
                let mut accent = cosmic.accent_color();
                accent.alpha = 0.35;
                Some(accent)
            } else {
                let mut neutral = cosmic.on_bg_color();
                neutral.alpha = 0.08;
                Some(neutral)
            };

            container::Style {
                background: fill.map(|c| Background::Color(Color::from(c))),
                border: Border {
                    radius: cosmic.corner_radii.radius_xs.into(),
                    ..Default::default()
                },
                ..Default::default()
            }
        })))
        .into()
}

/// A symbolic icon in the destructive colour, for a panel badge.
///
/// libcosmic offers no way to recolour a *named* icon — the note on
/// [`icon_base`] is about exactly that — but an `Icon` does take an svg class,
/// and a symbolic glyph is a one-colour svg. So the colour goes on as a filter
/// rather than by asking the icon theme for a red variant that no theme ships.
pub fn alert_icon<'a, Msg: 'a>(name: &'a str, size: u16) -> Element<'a, Msg> {
    icon::from_name(name)
        .size(size)
        .icon()
        .class(theme::Svg::custom(|theme| cosmic::widget::svg::Style {
            color: Some(theme.cosmic().destructive_color().into()),
        }))
        .into()
}

/// A ghost cell, optionally flashing because it just refused a drop.
///
/// The refusal has to be seen: a tile that snaps back with no other feedback
/// reads as a dropped gesture rather than as "not there". A destructive-red
/// wash on the cell that said no is the whole message.
pub fn ghost_slot<'a, Msg: 'a>(refused: bool, spacing: Spacing) -> Element<'a, Msg> {
    if !refused {
        return ghost_tile(spacing);
    }
    container(cosmic::widget::Space::new().width(Length::Fill))
        .height(Length::Fixed(tile_height(spacing)))
        .width(Length::Fill)
        .class(theme::Container::Custom(Box::new(|theme| {
            let cosmic = theme.cosmic();
            let mut fill = cosmic.destructive_color();
            fill.alpha = 0.20;
            let edge = cosmic.destructive_color();
            container::Style {
                background: Some(Background::Color(Color::from(fill))),
                border: Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    width: 1.0,
                    color: Color::from(edge),
                },
                ..Default::default()
            }
        })))
        .into()
}

/// A placeholder occupying a grid cell that has no tile in it.
///
/// Used only to keep a row square when an odd number of tiles is showing. It is
/// deliberately not interactive and carries no label: it says "the grid is this
/// wide" and nothing more. A ghost per hidden module would leave permanent holes
/// on machines that simply lack the hardware.
pub fn ghost_tile<'a, Msg: 'a>(spacing: Spacing) -> Element<'a, Msg> {
    container(cosmic::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(tile_height(spacing)))
        .class(theme::Container::Custom(Box::new(|theme| {
            let cosmic = theme.cosmic();
            // A faint wash of the foreground colour: visible enough to read as a
            // slot, far too faint to compete with a real tile.
            let mut fill = cosmic.on_bg_color();
            fill.alpha = 0.04;
            let mut edge = cosmic.on_bg_color();
            edge.alpha = 0.10;

            container::Style {
                background: Some(Background::Color(Color::from(fill))),
                border: Border {
                    radius: cosmic.corner_radii.radius_s.into(),
                    width: 1.0,
                    color: Color::from(edge),
                },
                ..Default::default()
            }
        })))
        .into()
}

/// What to draw in a cell no instance covers.
///
/// The gap is the same hole in both surfaces; only its treatment differs, so
/// this is a flag on one renderer rather than two renderers that can drift.
pub enum Ghosts<'a, Msg> {
    /// The popup: a gap the user left is plain background.
    Empty,
    /// A faint slot, so the gap reads as a place a tile could go.
    ///
    /// Kept for a read-only grid; the editable one builds its own gaps.
    #[allow(dead_code)]
    Visible,
    /// Settings while editing: the caller builds each empty cell itself,
    /// from its 0-based (col, row), so a gap can be a drop target rather
    /// than only a decoration.
    Custom(Box<dyn Fn(u16, u16) -> Element<'a, Msg> + 'a>),
}

impl<'a, Msg: 'static> Ghosts<'a, Msg> {
    fn draw(&self, col: u16, row: u16, spacing: Spacing) -> Element<'a, Msg> {
        match self {
            Ghosts::Empty => cosmic::widget::Space::new()
                .width(Length::Fill)
                .height(Length::Fill)
                .into(),
            Ghosts::Visible => ghost_tile(spacing),
            Ghosts::Custom(build) => build(col, row),
        }
    }
}

/// Draw placed tiles as a grid of bands.
///
/// Returns a single `Element` — a column of bands — that the popup's page
/// pushes into whatever column is above it. No grid widget: libcosmic's
/// `Grid` is taffy, and taffy gives a spanning item's width to the first
/// track it spans, which made a Wide tile eat column two. Plain rows and
/// columns express the placements exactly; see [`crate::tile_layout::bands`]
/// for the split.
///
/// Rules the caller relies on:
///
///  * Each tile is drawn at its instance's own cell. Nothing reorders and
///    nothing is packed — a gap between two tiles is drawn as a gap.
///  * Cells no instance covers become ghosts, drawn per `ghosts`.
///  * The layout must be validated ([`crate::tile_layout::validate`]);
///    overlapping instances are not a grid the band cutter can express.
pub fn tile_grid<'a, Msg: Clone + 'static>(
    tiles: Vec<(Element<'a, Msg>, Slot)>,
    ghosts: Ghosts<'a, Msg>,
    spacing: Spacing,
) -> Element<'a, Msg> {
    use crate::tile_layout::{bands, place, Entry};

    let slots: Vec<Slot> = tiles.iter().map(|(_, slot)| *slot).collect();
    let packed = place(&slots);
    let layout = bands(&packed);

    // Take elements out by index as the bands ask for them. `Option` so each
    // is moved exactly once; the band tests pin that every index is asked for
    // exactly once, so a `None` here is a packer/bands mismatch, not a user
    // state — draw a ghost rather than panic inside `view`.
    let mut slots: Vec<Option<Element<'a, Msg>>> =
        tiles.into_iter().map(|(e, _)| Some(e)).collect();
    let mut take = |entry: Entry| -> Element<'a, Msg> {
        match entry {
            Entry::Tile(i) => slots
                .get_mut(i)
                .and_then(Option::take)
                .unwrap_or_else(|| ghost_tile(spacing)),
            Entry::Ghost(col, row) => ghosts.draw(col, row, spacing),
        }
    };

    let mut column = cosmic::widget::column::with_capacity(layout.len()).spacing(spacing.gap);

    for band in layout {
        let mut band_row = row::with_capacity(band.strips.len()).spacing(spacing.gap);
        for strip in band.strips {
            // A strip is a column of rows. A row with nothing in it is the
            // lower half of a tile spanning two rows, which already occupies
            // that space — skipping it keeps the column's spacing honest.
            let mut strip_col =
                cosmic::widget::column::with_capacity(strip.rows.len()).spacing(spacing.gap);
            for entries in strip.rows {
                if entries.is_empty() {
                    continue;
                }
                let mut r = row::with_capacity(entries.len()).spacing(spacing.gap);
                for (entry, width) in entries {
                    r = r.push(container(take(entry)).width(Length::FillPortion(width)));
                }
                strip_col = strip_col.push(r);
            }
            band_row = band_row.push(container(strip_col).width(Length::FillPortion(strip.width)));
        }
        column = column.push(band_row);
    }

    column.into()
}

/// One row inside the [`connectivity_tile`].
///
/// Deliberately not a reuse of [`toggle_row`]: that is a page-level row with
/// its own padding, and three of them stacked inside a tile would be taller
/// than the Wide footprint allows. This is the compact form.
pub struct ConnectivityRow<'a, Msg> {
    pub icon_name: &'a str,
    pub label: String,
    /// The one line of state under the label — an SSID, "2 devices", a
    /// VPN profile name. `None` when there is nothing to say.
    pub state: Option<String>,
    /// Whether this thing is on, for the accent the tile style applies.
    pub on: bool,
    /// Pressing the row body, to drill into that module's page.
    ///
    /// `None` makes the row inert. The Settings preview needs that: a live
    /// button inside a tile captures the pointer, so the tile above it never
    /// sees the press and can be neither selected nor dragged.
    pub on_press: Option<Msg>,
}

/// The Wide tile grouping Wi-Fi, Bluetooth and VPN.
///
/// Modelled on the macOS Control Centre's connectivity block: one card,
/// three rows, each row a switch and a line of state, with the row body
/// itself a button into the module's page. Rows come and go with what the
/// machine actually has — a desktop without a Wi-Fi adapter gets two rows,
/// not a greyed-out third.
///
/// Occupies [`TileShape::Wide`] — two columns, one row — so it packs at
/// the top of the grid without being split around. Its height is that of a
/// Small tile stretched to fit however many rows are present, so the packer
/// treats it as one row-unit and the grid does not gain a ragged edge.
/// `height` is the cell it has to fill. The popup's Tall cell is two tile
/// rows plus the gap between them ([`tall_height`]); the Settings preview's
/// is taller, because every tile there carries a switch underneath and a Tall
/// tile spans two of those. Passing it in keeps both callers honest instead of
/// baking one caller's geometry into the widget.
pub fn connectivity_tile<'a, Msg: Clone + 'static>(
    rows: Vec<ConnectivityRow<'a, Msg>>,
    height: f32,
    style: TileStyle,
    finish: TileFinish,
    spacing: Spacing,
) -> Element<'a, Msg> {
    // Each row is exactly `connectivity_row_height`, spaced by
    // `connectivity_row_gap` — the same numbers `tile_height` was solved
    // from, so three of them fit the Tall footprint by construction.
    let mut column = cosmic::widget::column::with_capacity(rows.len())
        .spacing(connectivity_row_gap(spacing) as u16);

    for row_data in rows {
        // Name only. The state line was dropped when the tile went from two
        // columns to one: "a real SSID" does not fit beside a switch in half
        // the popup's width, and a truncated SSID tells you less than the
        // icon already does.
        let labels = cosmic::widget::column::with_capacity(1).push(
            text::body(truncate(&row_data.label, CONNECTIVITY_LABEL_CHARS))
                .wrapping(cosmic::iced::widget::text::Wrapping::None),
        );
        let _ = &row_data.state;

        // The same glyph treatment the standalone tiles get, so a row here
        // and a tile out there read as the same kind of thing: `Medium` puts
        // the accent on the icon's base, `High` fills the row, `Low` shows no
        // on-state at all.
        let signalled = row_data.on && style != TileStyle::Low;
        let glyph: Element<'a, Msg> = icon_base(
            row_data.icon_name.to_string(),
            signalled,
            style == TileStyle::Medium,
            spacing,
        );

        let inner = row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(spacing.gap)
            .push(glyph)
            .push(labels.width(Length::Fill));

        // The whole row is the button — press it to open that thing's page,
        // exactly as pressing a tile does. With no message it is a plain
        // container instead: see the note on `on_press`.
        // No padding of its own in either axis. Vertically, the row's height
        // is the icon on its base and any padding would come out of the budget
        // `tile_height` already spent. Horizontally, the tile's own inset is
        // the same `pad_x` a Small tile uses, so a row inset on top of it put
        // this icon 4px further from the edge than every other tile's — which
        // is exactly enough to look wrong without looking like a bug.
        let padding = Padding::ZERO;
        let height = Length::Fixed(connectivity_row_height(spacing));
        let body: Element<'a, Msg> = match row_data.on_press {
            Some(msg) => button::custom(inner)
                .padding(padding)
                .height(height)
                .class(if signalled && style == TileStyle::High {
                    button::ButtonClass::Suggested
                } else {
                    quiet_button()
                })
                .width(Length::Fill)
                .on_press(msg)
                .into(),
            None => container(inner)
                .padding(padding)
                .height(height)
                .width(Length::Fill)
                .into(),
        };

        column = column.push(body);
    }

    container(column)
        .padding(spacing.padding())
        .width(Length::Fill)
        .height(Length::Fixed(height))
        .class(tile_surface(finish))
        .into()
}

/// A button that behaves like one but is coloured like the surface it sits on.
///
/// The Connectivity rows are buttons inside a card that is already a tile.
/// `ButtonClass::Text` was the obvious fit and is wrong: libcosmic paints it
/// from `cosmic.text_button`, which is the **accent** colour, so the rows'
/// labels and glyphs came out accented while every other tile — a `Standard`
/// button, which leaves `text_color` and `icon_color` unset so they inherit —
/// stayed neutral. `Transparent` is no good either: it zeroes the text colour
/// rather than leaving it alone, so the labels disappear.
///
/// So: no fill at rest — the tile's card is already the surface — with the
/// same hover and pressed washes a tile itself uses, and text and icon left
/// to inherit exactly as `Standard` leaves them.
fn quiet_button() -> button::ButtonClass {
    fn base(
        theme: &cosmic::Theme,
        fill: Option<cosmic::cosmic_theme::palette::Srgba>,
    ) -> button::Style {
        let cosmic = theme.cosmic();
        button::Style {
            background: fill.map(|c| Background::Color(Color::from(c))),
            border_radius: cosmic.corner_radii.radius_s.into(),
            // Left as None on purpose — that is what makes the row inherit
            // the tile's text and icon colour instead of being told one.
            text_color: None,
            icon_color: None,
            ..button::Style::new()
        }
    }

    // The same washes a tile uses, from the same component — see
    // `tile_component`. These were `cosmic.button.hover`/`.pressed`, the
    // *button* component, so a Connectivity row lit up a different colour
    // from the tile sitting next to it under the same pointer.
    button::ButtonClass::Custom {
        active: Box::new(|_focused, theme| base(theme, None)),
        disabled: Box::new(|theme| base(theme, None)),
        hovered: Box::new(|_focused, theme| base(theme, Some(tile_component(theme).hover))),
        pressed: Box::new(|_focused, theme| base(theme, Some(tile_component(theme).pressed))),
    }
}

/// Height of a Tall cell in the popup: two tile rows plus the gap between
/// them, so the packer's arithmetic and the drawn height agree.
pub fn tall_height(spacing: Spacing) -> f32 {
    tile_height(spacing) * 2.0 + f32::from(spacing.gap)
}

/// Characters of a row's name the narrow Connectivity tile can show.
const CONNECTIVITY_LABEL_CHARS: usize = 10;

/// The Wide slider tile: icon, horizontal track, percentage, in a card.
///
/// Occupies [`TileShape::Wide`] — two columns, one row. A slider reads
/// left-to-right; the vertical form this replaces had shorter travel and put
/// the value somewhere the eye does not look for it.
///
/// The icon is a button — pressing it mutes or dims, the gesture the slider
/// row always had. When `enabled` is false the track is replaced by an inert
/// bar at the level it was, so a muted tile still says "you were at 45%" and
/// dragging cannot silently set a new value on a muted device.
/// How much of a slider tile is live.
///
/// Three states, not two booleans: "muted" and "a picture of a slider" look
/// alike but behave differently, and the pair `(enabled, interactive)` had a
/// fourth combination that means nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SliderMode {
    /// In the popup, device on: the track drags, the icon mutes.
    Live,
    /// In the popup, device muted or dimmed: a bar at the level it was, and
    /// the icon still works so you can bring it back.
    Held,
    /// In the Settings preview: a picture. Nothing inside responds, because a
    /// live slider captures every drag before the tile above it can be picked
    /// up and moved.
    Inert,
}

/// How a tile is drawn: its surface, and the metrics it is drawn to.
///
/// The two always travel together — every widget that paints a tile needs
/// both, and they both come from the same place — so they are one value
/// rather than two more parameters on every signature.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub finish: TileFinish,
    pub spacing: Spacing,
}

impl Look {
    pub fn new(finish: TileFinish, spacing: Spacing) -> Self {
        Self { finish, spacing }
    }
}

pub fn wide_slider_tile<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    label: impl Into<String>,
    value: f64,
    on_change: impl Fn(f64) -> Msg + 'a,
    on_icon_press: Option<Msg>,
    mode: SliderMode,
    look: Look,
) -> Element<'a, Msg> {
    let Look { finish, spacing } = look;
    let leading: Element<'a, Msg> = match on_icon_press.filter(|_| mode != SliderMode::Inert) {
        Some(msg) => button::icon(icon::from_name(icon_name).size(ICON_SIZE))
            .padding(spacing.pad_y / 2)
            .on_press(msg)
            .into(),
        None => container(icon::from_name(icon_name).size(ICON_SIZE))
            .padding(spacing.pad_y / 2)
            .into(),
    };

    let track: Element<'a, Msg> = if mode == SliderMode::Live {
        slider(0.0..=100.0, value, on_change)
            .step(1.0)
            .width(Length::Fill)
            .into()
    } else {
        inert_track(value, spacing)
    };

    let content = row::with_capacity(3)
        .align_y(Alignment::Center)
        .spacing(spacing.gap)
        .push(leading)
        .push(track)
        .push(text::caption(format!("{}%", value.round() as u32)));

    let tile: Element<'a, Msg> = container(content)
        .padding(spacing.padding())
        .width(Length::Fill)
        .height(Length::Fixed(tile_height(spacing)))
        .align_y(Alignment::Center)
        .class(tile_surface(finish))
        .into();

    tooltip(tile, text::body(label.into()), tooltip::Position::Top).into()
}

/// A read-only bar where a disabled slider's track would be.
///
/// Padded to the slider's own geometry rather than drawn edge to edge: a
/// slider's rail is inset by half a handle at each end, so a bare progress bar
/// visibly changes the track's length when a control is muted, and the row
/// twitches every time you press the icon.
fn inert_track<'a, Msg: Clone + 'a>(percent: f64, spacing: Spacing) -> Element<'a, Msg> {
    let _ = spacing;
    container(progress_bar::determinate_linear((percent / 100.0) as f32).width(Length::Fill))
        .padding(Padding::from([0, HANDLE_RADIUS]))
        .width(Length::Fill)
        .into()
}

/// Half of COSMIC's slider handle, which is how far its rail is inset at each
/// end. libcosmic exposes no way to ask, so it is written down here.
const HANDLE_RADIUS: u16 = 13;

/// A row whose control is an actual switch.
///
/// Used for the on/off at the top of a page. A tinted full-width button reads as
/// "an item that is selected", which is right for picking one of several power
/// profiles and wrong for a thing that is simply on or off — and it looked
/// identical to the list rows underneath it.
pub fn toggle_row<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    label: impl Into<String>,
    detail: Option<String>,
    value: bool,
    on_toggle: Option<Msg>,
    spacing: Spacing,
) -> Element<'a, Msg> {
    let mut labels = cosmic::widget::column::with_capacity(2).push(text::body(label.into()));
    if let Some(detail) = detail {
        labels = labels.push(text::caption(detail));
    }

    let mut switch = toggler(value);
    if let Some(msg) = on_toggle {
        switch = switch.on_toggle(move |_| msg.clone());
    }

    container(
        row::with_capacity(3)
            .align_y(Alignment::Center)
            .spacing(spacing.gap)
            .push(icon::from_name(icon_name).size(ICON_SIZE))
            .push(labels.width(Length::Fill))
            .push(switch),
    )
    .padding(spacing.padding())
    .width(Length::Fill)
    .into()
}

/// Header for a drill-down page: a back button and the page title.
pub fn page_header<'a, Msg: Clone + 'static>(
    title: impl Into<String>,
    back: Msg,
    spacing: Spacing,
) -> Element<'a, Msg> {
    row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(spacing.gap)
        .push(
            button::icon(icon::from_name(icons::back()).size(ICON_SIZE))
                .padding(spacing.pad_y)
                .on_press(back),
        )
        .push(text::heading(title.into()))
        .into()
}

/// A full-width row inside a drill-down list.
///
/// Grows to fit two lines when `detail` is present, so it is not given a fixed
/// height — the clipping trap only applies to the single-line tiles.
pub fn list_row<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    label: impl Into<String>,
    detail: Option<String>,
    selected: bool,
    on_press: Option<Msg>,
    spacing: Spacing,
) -> Element<'a, Msg> {
    let mut labels = cosmic::widget::column::with_capacity(2).push(text::body(label.into()));
    if let Some(detail) = detail {
        labels = labels.push(text::caption(detail));
    }

    let content = row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(spacing.gap)
        .push(icon::from_name(icon_name).size(ICON_SIZE))
        .push(labels.width(Length::Fill));

    let widget = button::custom(content)
        .padding(spacing.padding())
        .class(if selected {
            button::ButtonClass::Suggested
        } else {
            button::ButtonClass::Standard
        })
        .width(Length::Fill);

    match on_press {
        Some(msg) => widget.on_press(msg).into(),
        None => widget.into(),
    }
}

/// Wrap a page so a long list scrolls instead of being cut off.
///
/// The popup has a hard height cap set by the positioner. Without this, a flat
/// with thirty access points in range renders a column taller than the cap and
/// the overflow is simply not drawn — including, on the Wi-Fi page, the password
/// field and the error text.
pub fn scrollable_page<'a, Msg: 'a>(content: Element<'a, Msg>) -> Element<'a, Msg> {
    scrollable(content)
        .width(Length::Fill)
        .height(Length::Shrink)
        .into()
}

/// Shorten `text` to `max` characters, appending an ellipsis.
///
/// Counts characters rather than bytes so a multi-byte SSID is not cut mid-
/// codepoint, which would panic on the slice.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    // Leave room for the ellipsis so the result is never wider than `max`.
    let keep = max.saturating_sub(1);
    text.chars().take(keep).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tile_is_painted_in_a_frost_aware_colour() {
        // The bug this pins: tiles were `cosmic.button.base`, a flat 25% wash
        // whatever the frost setting, so a popup tiled edge to edge read as an
        // extra film over the blur instead of cards on glass. The card colour
        // has to follow the frosted_* flags like every other COSMIC surface.
        use cosmic::widget::button::Catalog;

        let mut theme = cosmic::Theme::dark();

        theme.transparent = false;
        let solid = tile_component(&theme).base.alpha;
        theme.transparent = true;
        let frosted = tile_component(&theme).base.alpha;

        assert!(
            frosted < solid,
            "a tile must go translucent under frost: {frosted} !< {solid}"
        );

        // The flat wash it replaced does not move, which is the whole point.
        let button_base = theme.cosmic().button.base.alpha;
        theme.transparent = false;
        assert!((theme.cosmic().button.base.alpha - button_base).abs() < f32::EPSILON);

        // And not the layer colour, which under frost *is* the popup's own
        // background — tiles drawn in it read as holes.
        theme.transparent = true;
        assert_ne!(
            tile_component(&theme).base,
            theme.cosmic().background(true).base,
            "a tile must not be painted in the layer colour underneath it"
        );

        // Both tile kinds ask the same question, so they cannot drift.
        let class = tile_button_class(TileFinish::Solid);
        let button_fill = match theme.active(false, false, &class).background {
            Some(cosmic::iced::Background::Color(c)) => c,
            other => panic!("a tile button must paint a colour, got {other:?}"),
        };
        assert_eq!(
            button_fill,
            cosmic::iced::Color::from(tile_component(&theme).base)
        );

        // Left to inherit, exactly as ButtonClass::Standard leaves them.
        let style = theme.active(false, false, &class);
        assert!(style.text_color.is_none() && style.icon_color.is_none());
    }

    #[test]
    fn a_connectivity_row_lights_up_the_same_as_a_tile() {
        // Under one pointer, a row inside the Connectivity tile and the tile
        // beside it have to be the same colour. They were not: the rows took
        // their washes from `cosmic.button`, tiles from the background card
        // component, so hovering one looked nothing like hovering the other.
        use cosmic::widget::button::Catalog;

        let theme = cosmic::Theme::dark();
        let row = quiet_button();
        let tile = tile_button_class(TileFinish::Solid);

        let fill = |style: button::Style| match style.background {
            Some(cosmic::iced::Background::Color(c)) => Some(c),
            _ => None,
        };

        assert_eq!(
            fill(theme.hovered(false, false, &row)),
            fill(theme.hovered(false, false, &tile)),
            "hover must match the tile beside it"
        );
        assert_eq!(
            fill(theme.pressed(false, false, &row)),
            fill(theme.pressed(false, false, &tile)),
            "pressed must match the tile beside it"
        );

        // At rest the row still paints nothing: the tile's card underneath is
        // the surface, and a second fill on top of it would read as a box in
        // a box.
        assert!(fill(theme.active(false, false, &row)).is_none());

        // And it still inherits its text and icon colour rather than being
        // told an accent one.
        let hovered = theme.hovered(false, false, &row);
        assert!(hovered.text_color.is_none() && hovered.icon_color.is_none());
    }

    #[test]
    fn a_wide_tile_keeps_more_of_its_state_than_a_small_one() {
        // The point of a Wide form: the same one line, with room to say the
        // thing a Small tile had to cut. "17% · Balan…" is what this avoids.
        let state = "Held by cosmic-media-player-session";
        assert!(state.chars().count() > MAX_WIDE_STATE_CHARS);

        let small = truncate(state, MAX_STATE_CHARS);
        let wide = truncate(state, MAX_WIDE_STATE_CHARS);
        assert!(wide.chars().count() > small.chars().count());
        const { assert!(MAX_WIDE_STATE_CHARS > MAX_STATE_CHARS) };

        // A state that fits the wide limit is not cut at all.
        let short = "Balanced";
        assert_eq!(truncate(short, MAX_WIDE_STATE_CHARS), short);
    }

    #[test]
    fn each_finish_paints_a_different_amount_of_the_card() {
        use cosmic::widget::button::Catalog;

        let mut theme = cosmic::Theme::dark();
        theme.transparent = true;

        let fill = |finish| match theme
            .active(false, false, &tile_button_class(finish))
            .background
        {
            Some(cosmic::iced::Background::Color(c)) => Some(c.a),
            _ => None,
        };

        let solid = fill(TileFinish::Solid).expect("solid fills");
        let frosted = fill(TileFinish::Frosted).expect("frosted fills");

        // Frosted lets more of the blur through than solid, but is still a
        // surface — not so thin it stops reading as a tile.
        assert!(frosted < solid, "{frosted} !< {solid}");
        assert!(
            frosted > solid * 0.3,
            "frosted is too thin to read as a tile"
        );

        // Outline paints nothing and says where the tile is with an edge.
        assert!(fill(TileFinish::Outline).is_none());
        let outlined = theme.active(false, false, &tile_button_class(TileFinish::Outline));
        assert!(outlined.border_width > 0.0);
        assert!(outlined.border_color.a > 0.0);
        // The filled finishes carry no edge — the fill is the boundary.
        assert_eq!(
            theme
                .active(false, false, &tile_button_class(TileFinish::Solid))
                .border_width,
            0.0
        );

        // An outlined tile still answers the pointer, or it reads as a label.
        assert!(theme
            .hovered(false, false, &tile_button_class(TileFinish::Outline))
            .background
            .is_some());
    }

    #[test]
    fn a_connectivity_row_inherits_its_colour_instead_of_being_accented() {
        // The bug this pins: the rows were `ButtonClass::Text`, which
        // libcosmic paints from `cosmic.text_button` — the accent colour — so
        // a row's label and glyph came out accented while every other tile,
        // being a `Standard` button, stayed neutral. A `Standard` button
        // leaves text_color and icon_color unset; so must this.
        use cosmic::widget::button::Catalog;

        let theme = cosmic::Theme::dark();
        let quiet = quiet_button();

        for style in [
            theme.active(false, false, &quiet),
            theme.hovered(false, false, &quiet),
            theme.pressed(false, false, &quiet),
            theme.disabled(&quiet),
        ] {
            assert!(
                style.text_color.is_none() && style.icon_color.is_none(),
                "a row must inherit its colour, not be told one"
            );
        }

        // Same as the standard tile button, which is the whole point.
        let standard = theme.active(false, false, &cosmic::theme::Button::Standard);
        assert!(standard.text_color.is_none() && standard.icon_color.is_none());

        // And the class it replaced really does force a colour, so this test
        // fails for the right reason if anyone switches back.
        let text = theme.active(false, false, &cosmic::theme::Button::Text);
        assert!(text.text_color.is_some());

        // At rest it is the card underneath that shows, not a second fill.
        assert!(theme.active(false, false, &quiet).background.is_none());
        assert!(theme.hovered(false, false, &quiet).background.is_some());
    }

    fn spacing() -> Spacing {
        // The standard-density values from cosmic-theme.
        Spacing {
            gap: 8,
            pad_y: 8,
            pad_x: 12,
            section: 12,
        }
    }

    #[test]
    fn a_tile_is_always_tall_enough_for_its_icon() {
        // The exact regression: a hardcoded 52px height against 34px of padding
        // left 18px for a 20px icon, and the icon was clipped. Deriving the
        // height makes that arithmetically impossible.
        let height = tile_height(spacing());
        let padding = f32::from(spacing().pad_y) * 2.0;
        assert!(
            height - padding >= f32::from(ICON_SIZE),
            "tile height {height} leaves {} for a {ICON_SIZE}px icon",
            height - padding
        );
    }

    #[test]
    fn three_connectivity_rows_fit_a_tall_tile() {
        // The whole reason tile_height is solved from the rows: at the old
        // one-line height the third row was clipped off the bottom.
        for spacing in [
            Spacing {
                gap: 4,
                pad_y: 4,
                pad_x: 8,
                section: 8,
            },
            Spacing {
                gap: 8,
                pad_y: 8,
                pad_x: 16,
                section: 16,
            },
        ] {
            let inner = tall_height(spacing) - f32::from(spacing.pad_y) * 2.0;
            let needed =
                connectivity_row_height(spacing) * 3.0 + connectivity_row_gap(spacing) * 2.0;
            assert!(
                inner >= needed,
                "{spacing:?}: Tall tile leaves {inner}px for rows that need {needed}px"
            );
        }
    }

    #[test]
    fn tile_height_grows_with_a_roomier_density() {
        // COSMIC's spacious density doubles some tokens. The height must follow,
        // rather than staying at a number chosen for the standard density.
        let compact = tile_height(Spacing {
            pad_y: 4,
            ..spacing()
        });
        let spacious = tile_height(Spacing {
            pad_y: 16,
            ..spacing()
        });
        assert!(spacious > compact);
        assert!(compact >= f32::from(ICON_SIZE));
    }

    #[test]
    fn short_text_is_left_alone() {
        assert_eq!(truncate("HomeNet", 14), "HomeNet");
        assert_eq!(truncate("exactly14chars", 14), "exactly14chars");
    }

    #[test]
    fn long_text_is_shortened_within_the_limit() {
        let out = truncate("BT-HomeHub-2.4GHz-Extended", 14);
        assert_eq!(out.chars().count(), 14);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn multibyte_text_is_not_cut_mid_character() {
        // Byte slicing here would panic; SSIDs and device names are arbitrary
        // UTF-8 and users really do put emoji in them.
        let out = truncate("café-café-café-café-café", 8);
        assert_eq!(out.chars().count(), 8);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn a_tiny_limit_does_not_underflow() {
        assert_eq!(truncate("abcdef", 1), "…");
        assert_eq!(truncate("abcdef", 0), "…");
    }
}
