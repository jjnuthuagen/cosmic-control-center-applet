//! Display brightness.
//!
//! "Display settings" in this applet means the backlight and nothing else — no
//! resolution, scaling or monitor arrangement. Those live in `cosmic-settings`.
//!
//! Reads come from sysfs, writes go through logind's
//! `org.freedesktop.login1.Session.SetBrightness`. That split is deliberate:
//! sysfs `brightness` is world-readable but only root may write it, and logind
//! will perform the write on behalf of the active session with no polkit prompt
//! and no udev rule to install. Writing sysfs directly would need one or the
//! other.

use cosmic::iced::Subscription;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::{poll_subscription, Availability};

const BACKLIGHT_DIR: &str = "/sys/class/backlight";
const SUBSYSTEM: &str = "backlight";

/// Below this the screen is usually black, which looks like a crash to the
/// user and leaves them unable to find the slider to undo it.
const MIN_PERCENT: f64 = 1.0;

/// Where undimming lands if the remembered level was lost.
const DEFAULT_RESTORE_PERCENT: f64 = 50.0;

/// How many polls a locally-set level is held for while the hardware catches
/// up. At the 1.5s poll interval this is a few seconds — long enough for a
/// logind write to land, short enough that a write which silently failed is not
/// left on screen indefinitely.
const SETTLE_SAMPLES: u8 = 4;

/// Smallest disagreement worth holding for, in percent. Devices quantise: a
/// backlight with a coarse range cannot land on an arbitrary percentage, so the
/// floor is widened to one raw step in [`Pending::tolerance`].
const MIN_SETTLE_TOLERANCE: f64 = 2.0;

#[zbus::proxy(
    interface = "org.freedesktop.login1.Session",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
    /// `(subsystem, name, brightness)` — brightness is a raw device value, not
    /// a percentage.
    fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// 0.0..=100.0, or `None` until the first sample arrives.
    pub percent: Option<f64>,
    /// Dimmed to minimum by pressing the icon, mirroring mute for volume.
    pub dimmed: bool,
    /// Level to come back to when undimming.
    restore: Option<f64>,
    device: Option<Device>,
    /// A level written locally that sysfs has not reported back yet.
    pending: Option<Pending>,
}

/// A local write waiting for the hardware to agree with it.
///
/// Without this the poll clobbers the slider mid-drag: the handle is moved,
/// the write is in flight, and a sample taken from the old value snaps it
/// backwards under the cursor. The target is held until sysfs reports it, or
/// until [`SETTLE_SAMPLES`] have passed — so a write that never lands gives up
/// and shows the truth rather than a level the screen is not at.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    target: f64,
    samples_left: u8,
}

impl Pending {
    /// How far off a sample may be and still count as having landed.
    fn tolerance(max: u32) -> f64 {
        // One raw step, or the floor, whichever is larger. A device with
        // `max = 10` moves in 10% jumps and can never report 47%.
        let step = if max == 0 {
            0.0
        } else {
            100.0 / f64::from(max)
        };
        step.max(MIN_SETTLE_TOLERANCE)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Device {
    name: String,
    max: u32,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// A fresh sample. `None` means no backlight device is present.
    Sampled(Option<f64>),
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Sampled(Some(percent)) => {
                self.availability = Availability::Available;
                if self.device.is_none() {
                    self.device = discover();
                }
                // Don't clobber a value the user is mid-drag on with a stale
                // poll: the slider writes optimistically and the device takes a
                // moment to catch up. Accepting every sample would make the
                // handle jump backwards under the cursor.
                if !self.settled_on(percent) {
                    return;
                }
                self.percent = Some(percent);
            }
            Event::Sampled(None) => {
                self.availability = Availability::Unavailable;
                self.percent = None;
                self.dimmed = false;
                self.restore = None;
                self.device = None;
                self.pending = None;
            }
        }
    }

    /// Whether `percent` may be accepted as the displayed level.
    ///
    /// Returns false while a local write is still waiting to be reflected, and
    /// clears the wait once the sample agrees or patience runs out.
    fn settled_on(&mut self, percent: f64) -> bool {
        let Some(pending) = &mut self.pending else {
            return true;
        };
        let tolerance = Pending::tolerance(self.device.as_ref().map_or(0, |device| device.max));

        if (percent - pending.target).abs() <= tolerance {
            self.pending = None;
            return true;
        }

        pending.samples_left = pending.samples_left.saturating_sub(1);
        if pending.samples_left == 0 {
            // The write never landed. Better to show what the screen is
            // actually doing than to keep a number that is not true.
            tracing::debug!(
                "brightness write to {:.0}% never took effect; sysfs reads {percent:.0}%",
                pending.target
            );
            self.pending = None;
            return true;
        }
        false
    }

    /// Apply a percentage locally and return the work needed to make it real.
    ///
    /// The local write is what keeps the slider smooth — waiting for logind to
    /// round-trip before moving the handle makes dragging feel broken.
    pub fn set(&mut self, percent: f64) -> Option<impl std::future::Future<Output = ()>> {
        let device = self.device.clone().or_else(discover)?;
        // Setting a level above the floor by any route means we are no longer
        // dimmed; otherwise the icon would still claim to be holding the screen
        // down while the slider says otherwise.
        if percent > MIN_PERCENT {
            self.dimmed = false;
        }
        let percent = percent.clamp(MIN_PERCENT, 100.0);
        self.percent = Some(percent);
        self.device = Some(device.clone());
        self.pending = Some(Pending {
            target: percent,
            samples_left: SETTLE_SAMPLES,
        });

        let raw = percent_to_raw(percent, device.max);
        Some(async move {
            if let Err(err) = write(&device.name, raw).await {
                // A failed write is worth reporting but not worth interrupting
                // the user over; the next poll will snap the slider back to
                // whatever the hardware actually did.
                tracing::warn!("could not set brightness: {err}");
            }
        })
    }

    /// Drop to minimum, or return to the level before that.
    ///
    /// The counterpart of mute on the volume row: one press to get the screen
    /// out of the way, one press to get it back. Without remembering the old
    /// level, undimming would have to guess, and any guess is wrong.
    pub fn toggle_dim(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        let target = if self.dimmed {
            // Fall back to a usable level rather than to nothing if the stored
            // value went missing — never leave the user on a black screen with a
            // dead toggle.
            self.restore.take().unwrap_or(DEFAULT_RESTORE_PERCENT)
        } else {
            self.restore = self.percent;
            MIN_PERCENT
        };

        self.dimmed = !self.dimmed;
        self.set(target)
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("brightness", Duration::from_millis(1500), || async {
            Some(Event::Sampled(sample()))
        })
    }
}

async fn write(name: &str, raw: u32) -> zbus::Result<()> {
    let connection = zbus::Connection::system().await?;
    SessionProxy::new(&connection)
        .await?
        .set_brightness(SUBSYSTEM, name, raw)
        .await
}

/// The first device in `/sys/class/backlight` with a usable `max_brightness`.
///
/// Machines with more than one backlight are rare and almost always want the
/// panel, which sorts first often enough that picking the first entry is a
/// reasonable default. Sorting makes the choice stable across boots; readdir
/// order is not.
fn discover() -> Option<Device> {
    let mut names: Vec<String> = std::fs::read_dir(BACKLIGHT_DIR)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();

    names.into_iter().find_map(|name| {
        let max = read_u32(&Path::new(BACKLIGHT_DIR).join(&name).join("max_brightness"))?;
        // A zero maximum would make every percentage calculation a division by
        // zero; treat such a device as not present.
        (max > 0).then_some(Device { name, max })
    })
}

fn sample() -> Option<f64> {
    let device = discover()?;
    let current = read_u32(&device_path(&device.name).join("brightness"))?;
    Some(raw_to_percent(current, device.max))
}

fn device_path(name: &str) -> PathBuf {
    Path::new(BACKLIGHT_DIR).join(name)
}

fn read_u32(path: &Path) -> Option<u32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn raw_to_percent(raw: u32, max: u32) -> f64 {
    if max == 0 {
        return 0.0;
    }
    (f64::from(raw) / f64::from(max) * 100.0).clamp(0.0, 100.0)
}

fn percent_to_raw(percent: f64, max: u32) -> u32 {
    let raw = (percent / 100.0 * f64::from(max)).round();
    // Guard the cast: a percentage above 100 from a caller would wrap to a
    // small value rather than saturating, dimming the screen instead of
    // brightening it.
    raw.clamp(1.0, f64::from(max)) as u32
}

/// One-shot read for `--check`.
pub fn probe() -> Result<String, String> {
    let device = discover().ok_or("no backlight device in /sys/class/backlight")?;
    let percent = sample().ok_or("device present but its brightness is unreadable")?;
    Ok(format!(
        "{} (max {}), currently {percent:.0}%",
        device.name, device.max
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dimmable() -> State {
        State {
            availability: Availability::Available,
            percent: Some(70.0),
            device: Some(Device {
                name: "test0".into(),
                max: 100,
            }),
            ..State::default()
        }
    }

    #[test]
    fn dimming_remembers_the_previous_level() {
        let mut state = dimmable();
        let _write = state.toggle_dim();
        assert!(state.dimmed);
        assert_eq!(state.percent, Some(MIN_PERCENT));

        let _write = state.toggle_dim();
        assert!(!state.dimmed);
        assert_eq!(state.percent, Some(70.0));
    }

    #[test]
    fn undimming_without_a_remembered_level_still_lights_the_screen() {
        // The alternative is restoring to None, leaving a black screen and a
        // toggle that appears to do nothing.
        let mut state = dimmable();
        state.dimmed = true;
        state.restore = None;

        let _write = state.toggle_dim();
        assert!(!state.dimmed);
        assert_eq!(state.percent, Some(DEFAULT_RESTORE_PERCENT));
    }

    #[test]
    fn dragging_the_slider_clears_the_dim_state() {
        let mut state = dimmable();
        let _write = state.toggle_dim();
        assert!(state.dimmed);

        let _write = state.set(40.0);
        assert!(!state.dimmed);
    }

    #[test]
    fn a_stale_poll_does_not_snap_the_slider_back() {
        // The regression: drag the handle, the write is in flight, and a sample
        // taken from the old value arrives and moves the handle backwards.
        let mut state = dimmable();
        let _write = state.set(30.0);
        assert_eq!(state.percent, Some(30.0));

        state.update(Event::Sampled(Some(70.0)));
        assert_eq!(state.percent, Some(30.0), "a stale sample was accepted");
    }

    #[test]
    fn the_sample_is_accepted_once_the_hardware_agrees() {
        let mut state = dimmable();
        let _write = state.set(30.0);

        state.update(Event::Sampled(Some(30.0)));
        assert!(state.pending.is_none());

        // And an external change — a hardware key — is picked up straight away
        // once nothing local is outstanding.
        state.update(Event::Sampled(Some(80.0)));
        assert_eq!(state.percent, Some(80.0));
    }

    #[test]
    fn a_write_that_never_lands_gives_up_rather_than_lying() {
        // logind can refuse the write. Holding the requested level forever
        // would show a brightness the screen is not at, with no way back.
        let mut state = dimmable();
        let _write = state.set(30.0);

        for _ in 0..SETTLE_SAMPLES {
            state.update(Event::Sampled(Some(70.0)));
        }
        assert_eq!(state.percent, Some(70.0));
        assert!(state.pending.is_none());
    }

    #[test]
    fn a_coarse_device_still_settles() {
        // max = 10 moves in 10% steps, so a request for 47% can only ever read
        // back as 50%. A fixed tolerance would never accept it and the slider
        // would stick for SETTLE_SAMPLES on every drag.
        let mut state = State {
            availability: Availability::Available,
            percent: Some(20.0),
            device: Some(Device {
                name: "coarse0".into(),
                max: 10,
            }),
            ..State::default()
        };
        let _write = state.set(47.0);
        state.update(Event::Sampled(Some(50.0)));
        assert!(state.pending.is_none(), "a quantised read was not accepted");
        assert_eq!(state.percent, Some(50.0));
    }

    #[test]
    fn percent_round_trips_through_raw() {
        let max = 96_000;
        for percent in [1.0, 25.0, 50.0, 99.0, 100.0] {
            let raw = percent_to_raw(percent, max);
            let back = raw_to_percent(raw, max);
            assert!(
                (back - percent).abs() < 0.01,
                "{percent}% became {raw} and read back as {back}%"
            );
        }
    }

    #[test]
    fn brightness_never_reaches_zero() {
        // The screen going fully black is unrecoverable without a hardware key,
        // so the floor is enforced in both directions.
        assert_eq!(percent_to_raw(0.0, 96_000), 1);
        assert_eq!(percent_to_raw(-50.0, 96_000), 1);
    }

    #[test]
    fn out_of_range_input_saturates_instead_of_wrapping() {
        let max = 96_000;
        assert_eq!(percent_to_raw(1_000.0, max), max);
    }

    #[test]
    fn a_zero_maximum_does_not_divide_by_zero() {
        assert_eq!(raw_to_percent(10, 0), 0.0);
    }
}
