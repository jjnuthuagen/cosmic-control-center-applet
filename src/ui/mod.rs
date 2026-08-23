//! Shared widgets.
//!
//! Nothing here picks a colour, radius or font size by hand. Everything comes
//! from the active COSMIC theme, so the popup follows the system light/dark
//! setting and accent colour without knowing which is in use.
//!
//! # Grid geometry
//!
//! Tiles are a fixed height and share the row's width equally. That is the
//! whole reason the tile carries no name label: a tile showing "Wi-Fi" above
//! "HomeNet" is two lines tall, one showing "Battery" above "82% · Balanced" is
//! two lines of very different width, and the grid ends up ragged. Icon plus
//! current state is one line of predictable size, and the icon already says
//! which control it is.

use cosmic::iced::{Alignment, Background, Border, Color, Length};
use cosmic::widget::{button, container, icon, row, slider, text};
use cosmic::{theme, Element};

/// Every tile is exactly this tall, so rows line up whatever is in them.
pub const TILE_HEIGHT: f32 = 52.0;
/// Longest state string a tile will show before ellipsis. Sized for the
/// narrower of the two columns at the popup's fixed width.
const MAX_STATE_CHARS: usize = 16;

/// A grid tile: an icon and the thing's current state.
pub struct Tile<'a, Msg> {
    icon_name: &'a str,
    state: String,
    active: bool,
    on_press: Option<Msg>,
}

impl<'a, Msg: Clone + 'static> Tile<'a, Msg> {
    pub fn new(icon_name: &'a str, state: impl Into<String>) -> Self {
        Self {
            icon_name,
            state: state.into(),
            active: false,
            on_press: None,
        }
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

    pub fn view(self, spacing: u16) -> Element<'a, Msg> {
        let content = row::with_capacity(2)
            .align_y(Alignment::Center)
            .spacing(spacing)
            .push(icon::from_name(self.icon_name).size(20))
            .push(text::body(truncate(&self.state, MAX_STATE_CHARS)).width(Length::Fill));

        let tile = button::custom(
            container(content)
                .padding(spacing)
                .center_y(Length::Fill)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .class(if self.active {
            button::ButtonClass::Suggested
        } else {
            button::ButtonClass::Standard
        })
        .width(Length::Fill)
        .height(Length::Fixed(TILE_HEIGHT));

        match self.on_press {
            Some(msg) => tile.on_press(msg).into(),
            None => tile.into(),
        }
    }
}

/// A placeholder occupying a grid cell that has no tile in it.
///
/// Used only to keep a row square when an odd number of tiles is showing. It is
/// deliberately not interactive and carries no label: it says "the grid is this
/// wide" and nothing more. A ghost per hidden module would put permanent holes
/// in the popup on machines that simply lack the hardware.
pub fn ghost_tile<'a, Msg: 'a>() -> Element<'a, Msg> {
    container(cosmic::widget::Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(TILE_HEIGHT))
        .class(theme::Container::Custom(Box::new(|theme| {
            let cosmic = theme.cosmic();
            // A faint wash of the foreground colour: visible enough to read as
            // a slot, far too faint to compete with a real tile.
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
    spacing: u16,
) -> Vec<Element<'a, Msg>> {
    let mut rows = Vec::with_capacity(tiles.len().div_ceil(2));
    let mut tiles = tiles.into_iter();

    while let Some(left) = tiles.next() {
        let right = tiles.next().unwrap_or_else(ghost_tile);
        rows.push(
            row::with_capacity(2)
                .spacing(spacing)
                .push(container(left).width(Length::FillPortion(1)))
                .push(container(right).width(Length::FillPortion(1)))
                .into(),
        );
    }

    rows
}

/// An icon plus a full-width slider, used for volume and brightness.
pub fn slider_row<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    value: f64,
    on_change: impl Fn(f64) -> Msg + 'a,
    on_icon_press: Option<Msg>,
    spacing: u16,
) -> Element<'a, Msg> {
    let leading: Element<'a, Msg> = match on_icon_press {
        Some(msg) => button::icon(icon::from_name(icon_name).size(18))
            .on_press(msg)
            .into(),
        None => icon::from_name(icon_name).size(18).into(),
    };

    row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(spacing)
        .push(leading)
        .push(
            slider(0.0..=100.0, value, on_change)
                .step(1.0)
                .width(Length::Fill),
        )
        .into()
}

/// Header for a drill-down page: a back button and the page title.
pub fn page_header<'a, Msg: Clone + 'static>(
    title: impl Into<String>,
    back: Msg,
    spacing: u16,
) -> Element<'a, Msg> {
    row::with_capacity(2)
        .align_y(Alignment::Center)
        .spacing(spacing)
        .push(button::icon(icon::from_name("go-previous-symbolic").size(16)).on_press(back))
        .push(text::heading(title.into()))
        .into()
}

/// A full-width row inside a drill-down list.
pub fn list_row<'a, Msg: Clone + 'static>(
    icon_name: &'a str,
    label: impl Into<String>,
    detail: Option<String>,
    selected: bool,
    on_press: Option<Msg>,
    spacing: u16,
) -> Element<'a, Msg> {
    let mut labels = cosmic::widget::column::with_capacity(2).push(text::body(label.into()));
    if let Some(detail) = detail {
        labels = labels.push(text::caption(detail));
    }

    let content = row::with_capacity(3)
        .align_y(Alignment::Center)
        .spacing(spacing)
        .push(icon::from_name(icon_name).size(18))
        .push(labels.width(Length::Fill));

    let widget = button::custom(container(content).padding(spacing).width(Length::Fill))
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
    fn short_text_is_left_alone() {
        assert_eq!(truncate("HomeNet", 16), "HomeNet");
        assert_eq!(truncate("exactly-sixteen!", 16), "exactly-sixteen!");
    }

    #[test]
    fn long_text_is_shortened_within_the_limit() {
        let out = truncate("BT-HomeHub-2.4GHz-Extended", 16);
        assert_eq!(out.chars().count(), 16);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn multibyte_text_is_not_cut_mid_character() {
        // Byte slicing here would panic; SSIDs and device names are arbitrary
        // UTF-8 and users really do use emoji in them.
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
