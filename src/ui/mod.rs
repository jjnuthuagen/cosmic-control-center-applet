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

use crate::config::TileStyle;

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
    f32::from(ICON_SIZE) + f32::from(spacing.pad_y) * 2.0 + icon_base_padding(spacing) * 2.0
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

/// A grid tile: an icon and the thing's current state.
pub struct Tile<'a, Msg> {
    icon_name: std::borrow::Cow<'a, str>,
    state: String,
    /// Full name of the control, shown on hover. The tile deliberately has no
    /// visible name label — this is where the discoverability goes instead.
    name: String,
    active: bool,
    style: TileStyle,
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
            on_press: None,
        }
    }

    /// How strongly to signal the on state. See [`TileStyle`].
    pub fn style(mut self, style: TileStyle) -> Self {
        self.style = style;
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

    pub fn view(self, spacing: Spacing) -> Element<'a, Msg> {
        // `Low` never shows an on state on the grid at all. Selection lives
        // inside the drill-down pages instead, which is how Battery has always
        // behaved: you learn which power profile is active by opening it.
        let signalled = self.active && self.style != TileStyle::Low;

        let icon_name = self.icon_name.into_owned();
        let glyph: Element<'a, Msg> = match self.style {
            TileStyle::Medium => icon_base(icon_name, signalled, spacing),
            _ => icon::from_name(icon_name).size(ICON_SIZE).into(),
        };

        let content = row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(spacing.gap)
            .push(glyph)
            .push(
                text::body(truncate(&self.state, MAX_STATE_CHARS))
                    .width(Length::Fill)
                    // Tiles are a fixed height sized for one line. Without this
                    // a long state string wraps, and the tile's content is
                    // squeezed and sits out of line with its neighbour.
                    // Character truncation alone is not enough: it counts
                    // characters, and what overflows is pixels.
                    .wrapping(cosmic::iced::widget::text::Wrapping::None),
            );

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
                button::ButtonClass::Standard
            })
            .width(Length::Fill)
            .height(Length::Fixed(tile_height(spacing)));

        let tile = match self.on_press {
            Some(msg) => tile.on_press(msg),
            None => tile,
        };

        // The name lives in the tooltip because the tile shows state, not name.
        // Without this, a user who does not recognise an icon has no way to
        // find out what it is.
        tooltip(tile, text::body(self.name), tooltip::Position::Top).into()
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
fn icon_base<'a, Msg: 'static>(
    icon_name: String,
    active: bool,
    spacing: Spacing,
) -> Element<'a, Msg> {
    container(icon::from_name(icon_name).size(ICON_SIZE))
        .padding(spacing.pad_y / 2)
        .class(theme::Container::Custom(Box::new(move |theme| {
            let cosmic = theme.cosmic();
            let fill = if active {
                let mut accent = cosmic.accent_color();
                accent.alpha = 0.35;
                accent
            } else {
                let mut neutral = cosmic.on_bg_color();
                neutral.alpha = 0.08;
                neutral
            };

            container::Style {
                background: Some(Background::Color(Color::from(fill))),
                border: Border {
                    radius: cosmic.corner_radii.radius_xs.into(),
                    ..Default::default()
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

/// Lay tiles out two to a row, padding an odd count with a ghost so the last
/// row stays square.
pub fn tile_grid<'a, Msg: Clone + 'a>(
    tiles: Vec<Element<'a, Msg>>,
    spacing: Spacing,
) -> Vec<Element<'a, Msg>> {
    let mut rows = Vec::with_capacity(tiles.len().div_ceil(2));
    let mut tiles = tiles.into_iter();

    while let Some(left) = tiles.next() {
        let right = tiles.next().unwrap_or_else(|| ghost_tile(spacing));
        rows.push(
            row::with_capacity(2)
                .spacing(spacing.gap)
                .push(container(left).width(Length::FillPortion(1)))
                .push(container(right).width(Length::FillPortion(1)))
                .into(),
        );
    }

    rows
}

/// An icon plus a full-width slider, used for volume and brightness.
///
/// When `enabled` is false the slider is replaced by an inert progress bar
/// rather than a styled-grey slider. That is deliberate: a disabled-looking
/// slider that still moves under the cursor is worse than one that plainly does
/// not respond, and iced's slider always takes a change handler, so "disabled"
/// would otherwise mean "still draggable, just painted differently".
pub fn slider_row<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    value: f64,
    on_change: impl Fn(f64) -> Msg + 'a,
    on_icon_press: Option<Msg>,
    enabled: bool,
    spacing: Spacing,
) -> Element<'a, Msg> {
    let leading: Element<'a, Msg> = match on_icon_press {
        Some(msg) => button::icon(icon::from_name(icon_name).size(ICON_SIZE))
            .padding(spacing.pad_y)
            .on_press(msg)
            .into(),
        None => container(icon::from_name(icon_name).size(ICON_SIZE))
            .padding(spacing.pad_y)
            .into(),
    };

    let track: Element<'a, Msg> = if enabled {
        slider(0.0..=100.0, value, on_change)
            .step(1.0)
            .width(Length::Fill)
            .into()
    } else {
        // Still shows where the level *was*, so turning it back on is not a
        // surprise, but takes no input.
        progress_bar::determinate_linear((value / 100.0) as f32)
            .width(Length::Fill)
            .into()
    };

    container(
        row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(spacing.gap)
            .push(leading)
            .push(track),
    )
    // Sliders sit next to tiles that have their own inset; without matching
    // horizontal padding the track runs wider than everything above it.
    .padding(Padding::from([0, spacing.pad_x]))
    .into()
}

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
