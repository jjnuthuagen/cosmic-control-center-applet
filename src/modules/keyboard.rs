//! Keyboard backlight.
//!
//! Same mechanism as display brightness — read from sysfs, write through
//! logind's `SetBrightness` — but the `leds` subsystem rather than `backlight`,
//! and presented completely differently.
//!
//! # Why this is a cycling tile and not a slider
//!
//! Keyboard backlights are stepped, not continuous. This ThinkPad reports
//! `max_brightness = 2`, i.e. off / low / high. Mapping a 0-100 slider onto
//! three values gives a control where most of the travel does nothing and the
//! handle jumps to the nearest third when released. A tile that cycles through
//! the actual levels matches the hardware, and matches how the keyboard's own
//! backlight key behaves.
//!
//! Devices with a wide range exist, so the levels are sampled from whatever
//! range the device reports rather than assuming three; [`MAX_LEVELS`] caps how
//! many the cycle offers, because nobody wants to press a tile 255 times.

use cosmic::iced::Subscription;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{poll_subscription, Availability};

const LEDS_DIR: &str = "/sys/class/leds";
const SUBSYSTEM: &str = "leds";

/// Most steps the cycle will offer, however fine-grained the device is.
const MAX_LEVELS: usize = 4;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// Index into [`State::levels`].
    pub level: usize,
    /// Raw device values this tile cycles through, lowest first.
    pub levels: Vec<u32>,
    device: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Changed {
        device: String,
        levels: Vec<u32>,
        level: usize,
    },
    Unavailable,
}

impl State {
    /// Fluent key describing the current level.
    ///
    /// Named rather than numeric because "1 of 3" tells the user nothing they
    /// cannot see by looking at the keyboard.
    pub fn level_key(&self) -> &'static str {
        if self.levels.len() <= 1 {
            return "keyboard-off";
        }
        let top = self.levels.len() - 1;
        match self.level {
            0 => "keyboard-off",
            n if n == top => "keyboard-high",
            n if n * 2 <= top => "keyboard-low",
            _ => "keyboard-medium",
        }
    }

    pub fn is_on(&self) -> bool {
        self.level > 0
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::Changed {
                device,
                levels,
                level,
            } => {
                self.availability = Availability::Available;
                self.device = Some(device);
                self.level = level.min(levels.len().saturating_sub(1));
                self.levels = levels;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.device = None;
                self.levels.clear();
                self.level = 0;
            }
        }
    }

    /// Step to the next level, wrapping back to off at the top.
    pub fn cycle(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        let device = self.device.clone()?;
        if self.levels.is_empty() {
            return None;
        }

        self.level = (self.level + 1) % self.levels.len();
        let raw = self.levels[self.level];

        Some(async move {
            if let Err(err) = write(&device, raw).await {
                tracing::warn!("could not set the keyboard backlight: {err}");
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription(
            "keyboard-backlight",
            Duration::from_millis(1500),
            || async { Some(sample()) },
        )
    }
}

fn sample() -> Event {
    let Some((name, max)) = discover() else {
        return Event::Unavailable;
    };
    let levels = levels_for(max);
    let current = read_u32(&device_path(&name).join("brightness")).unwrap_or(0);

    Event::Changed {
        level: nearest_level(&levels, current),
        levels,
        device: name,
    }
}

/// The first `leds` device that looks like a keyboard backlight.
///
/// `/sys/class/leds` is full of things that are not backlights — caps lock,
/// mute indicators, the trackpoint — so this matches on the name rather than
/// taking the first entry the way the display backlight can.
fn discover() -> Option<(String, u32)> {
    let mut names: Vec<String> = std::fs::read_dir(LEDS_DIR)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| {
            let lower = name.to_ascii_lowercase();
            lower.contains("kbd_backlight") || lower.contains("keyboard_backlight")
        })
        .collect();
    names.sort();

    names.into_iter().find_map(|name| {
        let max = read_u32(&device_path(&name).join("max_brightness"))?;
        // A device that cannot be turned up has nothing to cycle through.
        (max > 0).then_some((name, max))
    })
}

/// The raw values the cycle steps through, always starting at off.
fn levels_for(max: u32) -> Vec<u32> {
    let steps = (max as usize + 1).min(MAX_LEVELS);
    if steps <= 1 {
        return vec![0, max];
    }

    (0..steps)
        .map(|i| {
            // Spread evenly across the range, and make sure the last step is
            // exactly `max` rather than a rounded-down approximation of it.
            if i == steps - 1 {
                max
            } else {
                (u64::from(max) * i as u64 / (steps - 1) as u64) as u32
            }
        })
        .collect()
}

/// Which level a raw device value corresponds to.
///
/// The value may not be one of ours — the keyboard's own backlight key sets it
/// directly — so snap to the closest rather than giving up.
fn nearest_level(levels: &[u32], value: u32) -> usize {
    levels
        .iter()
        .enumerate()
        .min_by_key(|(_, level)| level.abs_diff(value))
        .map_or(0, |(index, _)| index)
}

fn device_path(name: &str) -> PathBuf {
    Path::new(LEDS_DIR).join(name)
}

fn read_u32(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

async fn write(name: &str, raw: u32) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    SessionProxy::new(&connection)
        .await?
        .set_brightness(SUBSYSTEM, name, raw)
        .await
}

/// One-shot read for `--check`.
pub fn probe() -> Result<String, String> {
    let (name, max) = discover().ok_or("no keyboard backlight in /sys/class/leds")?;
    let levels = levels_for(max);
    let current = read_u32(&device_path(&name).join("brightness")).unwrap_or(0);
    Ok(format!(
        "{name} (max {max}), {} level(s), currently {current}",
        levels.len()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_three_step_device_offers_exactly_its_steps() {
        // The ThinkPad case: max_brightness = 2 means off, low, high.
        assert_eq!(levels_for(2), vec![0, 1, 2]);
    }

    #[test]
    fn an_on_off_device_is_two_levels() {
        assert_eq!(levels_for(1), vec![0, 1]);
    }

    #[test]
    fn a_fine_grained_device_is_capped() {
        // Nobody wants to press a tile 255 times to get back to off.
        let levels = levels_for(255);
        assert_eq!(levels.len(), MAX_LEVELS);
        assert_eq!(levels.first(), Some(&0));
        // The top step must be the real maximum, not a rounded-down one.
        assert_eq!(levels.last(), Some(&255));
    }

    #[test]
    fn levels_are_ascending_and_start_at_off() {
        for max in [1u32, 2, 3, 7, 16, 255] {
            let levels = levels_for(max);
            assert_eq!(levels[0], 0, "max {max} must start at off");
            assert!(
                levels.windows(2).all(|w| w[0] < w[1]),
                "max {max} produced {levels:?}"
            );
        }
    }

    #[test]
    fn a_value_set_by_the_hardware_key_snaps_to_the_nearest_level() {
        // The keyboard's own backlight key writes the device directly, so the
        // value we read back is not necessarily one we chose.
        let levels = levels_for(255);
        assert_eq!(nearest_level(&levels, 0), 0);
        assert_eq!(nearest_level(&levels, 255), levels.len() - 1);
        assert_eq!(nearest_level(&levels, 90), 1);
    }

    #[test]
    fn cycling_wraps_back_to_off() {
        let mut state = State::default();
        state.update(Event::Changed {
            device: "tpacpi::kbd_backlight".into(),
            levels: vec![0, 1, 2],
            level: 0,
        });

        let seen: Vec<usize> = (0..4)
            .map(|_| {
                let _write = state.cycle();
                state.level
            })
            .collect();
        assert_eq!(seen, vec![1, 2, 0, 1]);
    }

    #[test]
    fn cycling_without_a_device_does_nothing() {
        let mut state = State::default();
        assert!(state.cycle().is_none());
    }

    #[test]
    fn level_names_cover_the_range() {
        let mut state = State::default();
        state.update(Event::Changed {
            device: "kbd".into(),
            levels: vec![0, 1, 2],
            level: 0,
        });
        assert_eq!(state.level_key(), "keyboard-off");
        assert!(!state.is_on());

        state.level = 2;
        assert_eq!(state.level_key(), "keyboard-high");
        assert!(state.is_on());
    }

    #[test]
    fn a_reported_level_beyond_the_range_is_clamped() {
        // Guards an index panic if the device changed under us between the read
        // and the update.
        let mut state = State::default();
        state.update(Event::Changed {
            device: "kbd".into(),
            levels: vec![0, 1],
            level: 9,
        });
        assert_eq!(state.level, 1);
    }
}
