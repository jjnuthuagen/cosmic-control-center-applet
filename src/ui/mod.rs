//! Shared widgets.
//!
//! Nothing in here picks a colour, a radius or a font size by hand. Everything
//! comes from the active COSMIC theme via `cosmic.spacing` / `cosmic.corner_radii`
//! so the popup follows the system's light/dark setting and accent colour
//! without the applet knowing which one is in use.

use cosmic::iced::{Alignment, Length};
use cosmic::widget::{button, column, container, icon, row, slider, text};
use cosmic::{Apply, Element};

/// A 1x1 or 2x1 grid tile.
///
/// `detail` is the second line — the SSID, the charge, the DNS provider. It is
/// optional because the quick toggles (Dark Mode) have nothing useful to put
/// there and a blank second line would make them taller than they need to be.
pub struct Tile<'a, Msg> {
    icon_name: &'a str,
    label: String,
    detail: Option<String>,
    active: bool,
    on_press: Option<Msg>,
    on_drill_down: Option<Msg>,
}

impl<'a, Msg: Clone + 'static> Tile<'a, Msg> {
    pub fn new(icon_name: &'a str, label: impl Into<String>) -> Self {
        Self {
            icon_name,
            label: label.into(),
            detail: None,
            active: false,
            on_press: None,
            on_drill_down: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn detail_maybe(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
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

    /// Adds the `>` affordance that opens a drill-down page.
    pub fn on_drill_down(mut self, msg: Msg) -> Self {
        self.on_drill_down = Some(msg);
        self
    }

    pub fn view(self, spacing: u16) -> Element<'a, Msg> {
        let mut label_column = column::with_capacity(2).push(text::body(self.label));
        if let Some(detail) = self.detail {
            label_column = label_column.push(text::caption(detail));
        }

        let content = row::with_capacity(3)
            .align_y(Alignment::Center)
            .spacing(spacing)
            .push(icon::from_name(self.icon_name).size(20))
            .push(label_column.width(Length::Fill))
            .push_maybe(self.on_drill_down.map(|msg| {
                button::icon(icon::from_name("go-next-symbolic").size(16))
                    .on_press(msg)
                    // The arrow is a separate hit target from the toggle: the
                    // tile body switches the thing on and off, the arrow opens
                    // its settings. Merging them would make it impossible to
                    // toggle Wi-Fi without opening the network list.
                    .apply(Element::from)
            }));

        let tile = button::custom(container(content).padding(spacing).width(Length::Fill))
            .class(if self.active {
                button::ButtonClass::Suggested
            } else {
                button::ButtonClass::Standard
            })
            .width(Length::Fill);

        match self.on_press {
            Some(msg) => tile.on_press(msg).into(),
            None => tile.into(),
        }
    }
}

/// An icon plus a full-width slider, used for volume and brightness.
///
/// `on_release` exists so the caller can distinguish "the user is dragging"
/// from "the user has settled". Writing on every drag frame floods the backend;
/// writing only on release makes the screen or the audio lag behind the handle.
/// Both are wired up, and the modules debounce by writing optimistically.
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
