//! Output volume.
//!
//! # Why this module shells out and the others do not
//!
//! Everywhere else in this applet the rule is "talk to the bus, never spawn a
//! process". Audio is the exception, and deliberately so: PipeWire exposes no
//! D-Bus interface for volume. The real alternatives are
//!
//! * `libpulse-binding` — a C dependency plus a threaded mainloop that has to
//!   be bridged into iced's subscription model by hand, or
//! * `pipewire-rs` — likewise a C dependency, and a much larger API surface
//!   than "read and set one number".
//!
//! Both are the right long-term answer and neither is worth blocking a first
//! release on. `wpctl` ships with WirePlumber, which COSMIC already depends on,
//! and its output format is a single stable line. Everything process-shaped is
//! confined to [`Backend`] so swapping in a real binding later touches nothing
//! else.
//!
//! Polling exists for the same reason: no signal to subscribe to.

use cosmic::iced::Subscription;
use std::time::Duration;
use tokio::process::Command;

use super::{poll_subscription, Availability};

/// Above 100% PipeWire applies software gain, which distorts. The slider stops
/// at unity; anything louder is a deliberate act for a mixer, not a panel.
const MAX_PERCENT: f64 = 100.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Backend {
    /// WirePlumber's CLI. Preferred: one line, one format, no locale drift.
    Wpctl,
    /// PulseAudio-compatible fallback, for a PipeWire install without
    /// WirePlumber or a genuine PulseAudio server.
    Pactl,
}

impl Backend {
    /// Pick a backend once. Returns `None` when neither tool exists, which is
    /// how the module reports itself unavailable.
    fn detect() -> Option<Self> {
        // `which` is avoided: PATH lookup is cheap to do directly and one less
        // process to depend on.
        for (backend, binary) in [(Backend::Wpctl, "wpctl"), (Backend::Pactl, "pactl")] {
            if find_in_path(binary).is_some() {
                return Some(backend);
            }
        }
        None
    }
}

fn find_in_path(binary: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(binary))
            .find(|candidate| candidate.is_file())
    })
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    /// 0.0..=100.0.
    pub percent: Option<f64>,
    pub muted: bool,
}

#[derive(Debug, Clone)]
pub enum Event {
    Sampled { percent: f64, muted: bool },
    Unavailable,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Sampled { percent, muted } => {
                self.availability = Availability::Available;
                self.percent = Some(percent);
                self.muted = muted;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.percent = None;
            }
        }
    }

    pub fn set(&mut self, percent: f64) -> impl std::future::Future<Output = ()> {
        let percent = percent.clamp(0.0, MAX_PERCENT);
        self.percent = Some(percent);
        // Dragging the slider off zero should unmute — otherwise the handle
        // moves and nothing is audible, which reads as a broken slider.
        let unmute = self.muted && percent > 0.0;
        if unmute {
            self.muted = false;
        }
        async move {
            if let Err(err) = write_volume(percent, unmute).await {
                tracing::warn!("could not set volume: {err}");
            }
        }
    }

    pub fn toggle_mute(&mut self) -> impl std::future::Future<Output = ()> {
        self.muted = !self.muted;
        let muted = self.muted;
        async move {
            if let Err(err) = write_mute(muted).await {
                tracing::warn!("could not toggle mute: {err}");
            }
        }
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("volume", Duration::from_millis(1000), || async {
            Some(sample().await)
        })
    }
}

async fn sample() -> Event {
    let Some(backend) = Backend::detect() else {
        return Event::Unavailable;
    };

    let output = match backend {
        Backend::Wpctl => run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).await,
        Backend::Pactl => {
            // pactl needs two calls; volume and mute are separate commands.
            let volume = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).await;
            let mute = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).await;
            match (volume, mute) {
                (Some(volume), Some(mute)) => Some(format!("{volume}\n{mute}")),
                _ => None,
            }
        }
    };

    let Some(output) = output else {
        return Event::Unavailable;
    };

    match backend {
        Backend::Wpctl => parse_wpctl(&output),
        Backend::Pactl => parse_pactl(&output),
    }
    .map_or(Event::Unavailable, |(percent, muted)| Event::Sampled {
        percent,
        muted,
    })
}

async fn write_volume(percent: f64, unmute: bool) -> std::io::Result<()> {
    let Some(backend) = Backend::detect() else {
        return Ok(());
    };
    match backend {
        Backend::Wpctl => {
            // wpctl takes a 0.0-1.0 fraction, not a percentage.
            let fraction = format!("{:.3}", percent / 100.0);
            run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &fraction]).await;
            if unmute {
                run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "0"]).await;
            }
        }
        Backend::Pactl => {
            let value = format!("{}%", percent.round() as u32);
            run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &value]).await;
            if unmute {
                run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", "0"]).await;
            }
        }
    }
    Ok(())
}

async fn write_mute(muted: bool) -> std::io::Result<()> {
    let Some(backend) = Backend::detect() else {
        return Ok(());
    };
    let flag = if muted { "1" } else { "0" };
    match backend {
        Backend::Wpctl => {
            run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", flag]).await;
        }
        Backend::Pactl => {
            run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", flag]).await;
        }
    }
    Ok(())
}

async fn run(binary: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(binary)
        // These tools localise their output. Forcing C keeps the parsers
        // matching literal "Volume:" and "Mute:" on every system.
        .env("LC_ALL", "C")
        .args(args)
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        tracing::debug!("{binary} {args:?} exited with {}", output.status);
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

/// `Volume: 0.50 [MUTED]`
fn parse_wpctl(output: &str) -> Option<(f64, bool)> {
    let line = output.lines().find(|line| line.contains("Volume:"))?;
    let muted = line.contains("[MUTED]");
    let fraction: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some((super::clamp_percent(fraction * 100.0), muted))
}

/// `Volume: front-left: 32768 /  50% / -18.06 dB, ...` plus `Mute: yes`
fn parse_pactl(output: &str) -> Option<(f64, bool)> {
    let muted = output
        .lines()
        .find(|line| line.starts_with("Mute:"))
        .is_some_and(|line| line.contains("yes"));

    // Take the first channel's percentage. Per-channel volumes can differ if
    // something else set a balance; the slider represents one number, and the
    // left channel is as good a representative as any.
    let percent = output
        .split('/')
        .map(str::trim)
        .find_map(|part| part.strip_suffix('%')?.trim().parse::<f64>().ok())?;

    Some((super::clamp_percent(percent), muted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_output() {
        assert_eq!(parse_wpctl("Volume: 0.50\n"), Some((50.0, false)));
        assert_eq!(parse_wpctl("Volume: 0.50 [MUTED]\n"), Some((50.0, true)));
        assert_eq!(parse_wpctl("Volume: 1.00\n"), Some((100.0, false)));
        assert_eq!(parse_wpctl("Volume: 0.00\n"), Some((0.0, false)));
    }

    #[test]
    fn wpctl_boosted_volume_is_clamped_to_the_slider_range() {
        // wpctl happily reports above 1.0; the slider has no room for it and
        // an out-of-range value would panic the widget.
        assert_eq!(parse_wpctl("Volume: 1.50\n"), Some((100.0, false)));
    }

    #[test]
    fn parses_pactl_output() {
        let output = "Volume: front-left: 32768 /  50% / -18.06 dB,   front-right: 32768 /  50% / -18.06 dB\n        balance 0.00\nMute: yes\n";
        assert_eq!(parse_pactl(output), Some((50.0, true)));
    }

    #[test]
    fn pactl_unmuted_is_detected() {
        let output = "Volume: front-left: 65536 / 100% / 0.00 dB\nMute: no\n";
        assert_eq!(parse_pactl(output), Some((100.0, false)));
    }

    #[test]
    fn unparseable_output_is_reported_rather_than_guessed() {
        // A future format change must surface as "unavailable", not as 0%,
        // which would look like the user's audio had been silenced.
        assert_eq!(parse_wpctl("something else entirely"), None);
        assert_eq!(parse_pactl("something else entirely"), None);
    }
}
