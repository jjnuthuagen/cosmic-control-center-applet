//! Media playback over MPRIS.
//!
//! # Only players that are already running
//!
//! `org.mpris.MediaPlayer2.playerctld` is D-Bus activatable, so enumerating
//! names and talking to the first match would *start* playerctld on a machine
//! that has it installed but is not playing anything. That is the same mistake
//! [`crate::modules::gamemode`] made, and it produces a media control that
//! conjures a daemon into existence just by opening the popup.
//!
//! So only names with a current owner are considered. A machine with no player
//! running has no media row, which is correct: there is nothing to control.

use cosmic::iced::Subscription;
use std::collections::HashMap;
use std::time::Duration;
use zbus::zvariant::OwnedValue;

use super::{poll_subscription, Availability};

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const POLL_INTERVAL: Duration = Duration::from_millis(1500);

#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait Player {
    fn play_pause(&self) -> zbus::Result<()>;
    fn next(&self) -> zbus::Result<()>;
    fn previous(&self) -> zbus::Result<()>;

    /// "Playing", "Paused" or "Stopped".
    #[zbus(property)]
    fn playback_status(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
    #[zbus(property)]
    fn can_go_next(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn can_go_previous(&self) -> zbus::Result<bool>;
}

#[derive(Debug, Clone, Default)]
pub struct State {
    pub availability: Availability,
    pub playing: bool,
    /// Track title, or the player's bus suffix if it reports no metadata.
    pub title: String,
    pub artist: Option<String>,
    pub can_next: bool,
    pub can_previous: bool,
    /// Bus name of the player being controlled.
    player: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Playing(Box<Now>),
    Unavailable,
}

#[derive(Debug, Clone, Default)]
pub struct Now {
    pub player: String,
    pub playing: bool,
    pub title: String,
    pub artist: Option<String>,
    pub can_next: bool,
    pub can_previous: bool,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Playing(now) => {
                let now = *now;
                self.availability = Availability::Available;
                self.player = Some(now.player);
                self.playing = now.playing;
                self.title = now.title;
                self.artist = now.artist;
                self.can_next = now.can_next;
                self.can_previous = now.can_previous;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.player = None;
                self.playing = false;
                self.title.clear();
                self.artist = None;
            }
        }
    }

    /// One line for the tile: the track, and who it is by if known.
    pub fn summary(&self) -> String {
        match &self.artist {
            Some(artist) if !artist.is_empty() => format!("{} — {artist}", self.title),
            _ => self.title.clone(),
        }
    }

    pub fn play_pause(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        // Optimistic, so the icon flips under the cursor rather than after the
        // next poll.
        self.playing = !self.playing;
        self.command(Command::PlayPause)
    }

    pub fn next(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        self.can_next.then(|| self.command(Command::Next))?
    }

    pub fn previous(&mut self) -> Option<impl std::future::Future<Output = ()>> {
        self.can_previous.then(|| self.command(Command::Previous))?
    }

    fn command(&self, command: Command) -> Option<impl std::future::Future<Output = ()>> {
        let player = self.player.clone()?;
        Some(async move {
            if let Err(err) = send(&player, command).await {
                tracing::warn!("media command failed: {err}");
            }
        })
    }

    pub fn subscription(&self) -> Subscription<Event> {
        poll_subscription("media", POLL_INTERVAL, || async {
            Some(match now_playing().await {
                Ok(Some(now)) => Event::Playing(Box::new(now)),
                Ok(None) => Event::Unavailable,
                Err(err) => {
                    tracing::debug!("MPRIS unavailable: {err}");
                    Event::Unavailable
                }
            })
        })
    }
}

#[derive(Debug, Clone, Copy)]
enum Command {
    PlayPause,
    Next,
    Previous,
}

async fn send(player: &str, command: Command) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let proxy = PlayerProxy::builder(&connection)
        .destination(player.to_string())?
        .build()
        .await?;

    match command {
        Command::PlayPause => proxy.play_pause().await,
        Command::Next => proxy.next().await,
        Command::Previous => proxy.previous().await,
    }
}

/// The first running player, or `None` if nothing is playing anything.
async fn now_playing() -> zbus::Result<Option<Now>> {
    let connection = zbus::Connection::session().await?;
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;

    // `list_names` is owned names only. `list_activatable_names` would include
    // players that are merely installed — see the note at the top.
    let mut players: Vec<String> = bus
        .list_names()
        .await?
        .into_iter()
        .map(|name| name.to_string())
        .filter(|name| name.starts_with(PREFIX))
        .collect();
    // Stable choice when several are running, rather than bus-order roulette.
    players.sort();

    // Prefer one that is actually playing over one that is merely open, so a
    // paused browser tab does not hide the music.
    let mut fallback = None;
    for player in players {
        let Ok(now) = describe(&connection, &player).await else {
            continue;
        };
        if now.playing {
            return Ok(Some(now));
        }
        fallback.get_or_insert(now);
    }

    Ok(fallback)
}

async fn describe(connection: &zbus::Connection, player: &str) -> zbus::Result<Now> {
    let proxy = PlayerProxy::builder(connection)
        .destination(player.to_string())?
        .build()
        .await?;

    let metadata = proxy.metadata().await.unwrap_or_default();

    Ok(Now {
        playing: proxy.playback_status().await.as_deref() == Ok("Playing"),
        // Falling back to the bus suffix means the row still identifies *which*
        // player it is driving when a stream reports no title.
        title: string_of(&metadata, "xesam:title")
            .unwrap_or_else(|| player.trim_start_matches(PREFIX).to_string()),
        artist: string_of(&metadata, "xesam:artist"),
        can_next: proxy.can_go_next().await.unwrap_or(false),
        can_previous: proxy.can_go_previous().await.unwrap_or(false),
        player: player.to_string(),
    })
}

/// Read a metadata field that may be a string or a list of them.
///
/// `xesam:artist` is specified as an array; some players send a bare string
/// anyway, so both are accepted.
fn string_of(metadata: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = metadata.get(key)?;

    if let Ok(text) = <String>::try_from(value.clone()) {
        return (!text.is_empty()).then_some(text);
    }
    if let Ok(list) = <Vec<String>>::try_from(value.clone()) {
        let joined = list.join(", ");
        return (!joined.is_empty()).then_some(joined);
    }
    None
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    match now_playing().await {
        Ok(Some(now)) => Ok(format!(
            "{} on {} ({})",
            now.title,
            now.player.trim_start_matches(PREFIX),
            if now.playing { "playing" } else { "paused" }
        )),
        Ok(None) => Err("no media player is running".to_string()),
        Err(err) => Err(format!("could not ask the session bus: {err}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata(pairs: &[(&str, OwnedValue)]) -> HashMap<String, OwnedValue> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    }

    fn text(value: &str) -> OwnedValue {
        OwnedValue::try_from(zbus::zvariant::Value::from(value)).unwrap()
    }

    #[test]
    fn artist_is_read_whether_it_is_a_list_or_a_string() {
        // The spec says array; real players send both.
        let as_list = metadata(&[(
            "xesam:artist",
            OwnedValue::try_from(zbus::zvariant::Value::from(vec!["Boards of Canada"])).unwrap(),
        )]);
        assert_eq!(
            string_of(&as_list, "xesam:artist").as_deref(),
            Some("Boards of Canada")
        );

        let as_string = metadata(&[("xesam:artist", text("Boards of Canada"))]);
        assert_eq!(
            string_of(&as_string, "xesam:artist").as_deref(),
            Some("Boards of Canada")
        );
    }

    #[test]
    fn several_artists_are_joined() {
        let many = metadata(&[(
            "xesam:artist",
            OwnedValue::try_from(zbus::zvariant::Value::from(vec!["A", "B"])).unwrap(),
        )]);
        assert_eq!(string_of(&many, "xesam:artist").as_deref(), Some("A, B"));
    }

    #[test]
    fn empty_metadata_reads_as_absent_rather_than_blank() {
        // An empty string would render as a title of nothing at all.
        let blank = metadata(&[("xesam:title", text(""))]);
        assert_eq!(string_of(&blank, "xesam:title"), None);
        assert_eq!(string_of(&HashMap::new(), "xesam:title"), None);
    }

    #[test]
    fn the_summary_omits_a_missing_artist() {
        let mut state = State::default();
        state.update(Event::Playing(Box::new(Now {
            title: "Roygbiv".into(),
            artist: None,
            ..Now::default()
        })));
        assert_eq!(state.summary(), "Roygbiv");

        state.artist = Some("Boards of Canada".into());
        assert_eq!(state.summary(), "Roygbiv — Boards of Canada");
    }

    #[test]
    fn skip_is_refused_when_the_player_says_it_cannot() {
        let mut state = State::default();
        state.update(Event::Playing(Box::new(Now {
            title: "Stream".into(),
            can_next: false,
            can_previous: false,
            ..Now::default()
        })));
        assert!(state.next().is_none());
        assert!(state.previous().is_none());
    }

    #[test]
    fn losing_the_player_clears_the_row() {
        let mut state = State::default();
        state.update(Event::Playing(Box::new(Now {
            title: "Roygbiv".into(),
            ..Now::default()
        })));
        assert!(state.availability.is_shown());

        state.update(Event::Unavailable);
        assert!(!state.availability.is_shown());
        assert!(state.title.is_empty());
    }
}
