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

/// How often the title advances one character.
///
/// Slow enough to read a word as it passes, fast enough that a long title comes
/// round again before you have given up on it.
const SCROLL_INTERVAL: Duration = Duration::from_millis(220);

/// Separator between the end of the title and its repeat.
const GAP: &str = "   •   ";

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

/// The root MPRIS interface, which carries the player's own name.
#[zbus::proxy(
    interface = "org.mpris.MediaPlayer2",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer {
    /// "A friendly name to identify the media player to users", per the MPRIS
    /// spec. Chromium answers "Chromium" here, where its bus name is
    /// `org.mpris.MediaPlayer2.chromium.instance16790`.
    #[zbus(property)]
    fn identity(&self) -> zbus::Result<String>;

    /// Basename of the player's `.desktop` file, which is usually also its
    /// icon name. Optional in the spec and genuinely absent in the wild —
    /// Chromium does not publish it — so treat a failure as normal.
    #[zbus(property)]
    fn desktop_entry(&self) -> zbus::Result<String>;
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
    /// What the player calls itself: "Chromium", "Spotify".
    pub player_name: String,
    /// Icon name for the player, already resolved against the active theme.
    pub icon: String,
    /// How far the title has scrolled, in characters. See [`State::marquee`].
    offset: usize,
    /// Bus name of the player being controlled.
    player: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Playing(Box<Now>),
    Unavailable,
    /// One step of the title's scroll. Separate from `Playing` because it
    /// happens several times a second and touches nothing else.
    Scroll,
}

#[derive(Debug, Clone, Default)]
pub struct Now {
    pub player: String,
    pub playing: bool,
    pub title: String,
    pub artist: Option<String>,
    pub can_next: bool,
    pub can_previous: bool,
    pub player_name: String,
    pub icon: String,
}

impl State {
    pub fn update(&mut self, event: Event) {
        match event {
            Event::Playing(now) => {
                let now = *now;
                self.availability = Availability::Available;
                // A different track restarts the scroll, so a new title is read
                // from its beginning rather than from wherever the last one had
                // got to.
                if self.title != now.title {
                    self.offset = 0;
                }
                self.player = Some(now.player);
                self.playing = now.playing;
                self.title = now.title;
                self.artist = now.artist;
                self.can_next = now.can_next;
                self.can_previous = now.can_previous;
                self.player_name = now.player_name;
                self.icon = now.icon;
            }
            Event::Unavailable => {
                self.availability = Availability::Unavailable;
                self.player = None;
                self.playing = false;
                self.title.clear();
                self.artist = None;
                self.player_name.clear();
                self.offset = 0;
            }
            Event::Scroll => self.offset = self.offset.saturating_add(1),
        }
    }

    /// The track and who it is by, for the line under the player's name.
    ///
    /// The player is named on its own line now, so this no longer falls back to
    /// it — repeating "Chromium" directly under "Chromium" says nothing. A
    /// player reporting no metadata says so instead.
    pub fn summary(&self) -> String {
        if self.title.is_empty() {
            return crate::i18n::lookup("media-nothing-playing", None);
        }
        match &self.artist {
            Some(artist) if !artist.is_empty() => format!("{} — {artist}", self.title),
            _ => self.title.clone(),
        }
    }

    /// The summary, scrolled, cut to `width` characters.
    ///
    /// Track names routinely run past the width a panel popup can give them,
    /// and the row has three transport buttons that must not be pushed off the
    /// end or drawn over. Clipping fits, but "Everything In Its Right Pla…"
    /// hides exactly the part you were trying to read. So it scrolls: the text
    /// is cut to a fixed number of characters, which is what keeps the row's
    /// geometry stable, and the window onto it advances a character at a time.
    ///
    /// Character counts rather than pixels, because the text is measured by the
    /// widget after this returns and there is no font metric available here.
    /// The visible width therefore breathes slightly as wide and narrow glyphs
    /// pass through it — which is invisible in practice, and the alternative is
    /// a custom widget doing its own layout.
    pub fn marquee(&self, width: usize) -> String {
        let summary = self.summary();
        let length = summary.chars().count();

        // Short enough to sit still. Scrolling something that already fits is
        // motion for its own sake, and it is harder to read moving.
        if width == 0 || length <= width {
            return summary;
        }

        // The gap is what makes the loop legible: without it the end of the
        // title runs straight into its beginning and reads as one long string.
        let looped: Vec<char> = summary.chars().chain(GAP.chars()).collect();
        let start = self.offset % looped.len();

        looped
            .iter()
            .cycle()
            .skip(start)
            .take(width)
            .collect::<String>()
    }

    /// Whether the title needs scrolling at this width.
    ///
    /// Drives the subscription: a title that fits needs no ticks, and a panel
    /// applet waking several times a second to animate nothing is exactly the
    /// idle cost this codebase avoids elsewhere.
    pub fn scrolls(&self, width: usize) -> bool {
        width > 0 && self.summary().chars().count() > width
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

    pub fn subscription(&self, marquee_width: usize) -> Subscription<Event> {
        let poll = poll_subscription("media", POLL_INTERVAL, || async {
            Some(match now_playing().await {
                Ok(Some(now)) => Event::Playing(Box::new(now)),
                Ok(None) => Event::Unavailable,
                Err(err) => {
                    tracing::debug!("MPRIS unavailable: {err}");
                    Event::Unavailable
                }
            })
        });

        if !self.scrolls(marquee_width) {
            return poll;
        }

        Subscription::batch([
            poll,
            poll_subscription("media-scroll", SCROLL_INTERVAL, || async {
                Some(Event::Scroll)
            }),
        ])
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

    let root = MediaPlayerProxy::builder(connection)
        .destination(player.to_string())?
        .build()
        .await
        .ok();
    let identity = match &root {
        Some(root) => root.identity().await.unwrap_or_default(),
        None => String::new(),
    };
    // Optional in the spec and missing on real players, so absence is normal.
    let desktop_entry = match &root {
        Some(root) => root.desktop_entry().await.ok(),
        None => None,
    };
    let suffix = readable_bus_name(player);

    // The player's own name, with the bus suffix as a last resort. Never the
    // full bus name: MPRIS lets a player append an instance suffix to keep it
    // unique, which is how "chromium.instance16790" reached the screen.
    let player_name = if identity.is_empty() {
        suffix.clone()
    } else {
        identity.clone()
    };

    Ok(Now {
        icon: crate::ui::icons::media_player(desktop_entry.as_deref(), &suffix, &identity),
        playing: proxy.playback_status().await.as_deref() == Ok("Playing"),
        title: string_of(&metadata, "xesam:title").unwrap_or_default(),
        player_name,
        artist: string_of(&metadata, "xesam:artist"),
        can_next: proxy.can_go_next().await.unwrap_or(false),
        can_previous: proxy.can_go_previous().await.unwrap_or(false),
        player: player.to_string(),
    })
}

/// Last-resort name from a bus name: `…MediaPlayer2.chromium.instance16790`
/// becomes `chromium`.
fn readable_bus_name(player: &str) -> String {
    player
        .trim_start_matches(PREFIX)
        .split('.')
        .next()
        .unwrap_or(player)
        .to_string()
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

#[cfg(test)]
mod name_tests {
    use super::*;

    fn playing(title: &str) -> State {
        State {
            title: title.to_string(),
            ..State::default()
        }
    }

    #[test]
    fn a_title_that_fits_does_not_move() {
        // Scrolling something already readable is motion for its own sake.
        let state = playing("Blue Monday");
        assert_eq!(state.marquee(26), "Blue Monday");
        assert!(!state.scrolls(26));
    }

    #[test]
    fn a_long_title_is_cut_to_the_width_and_stays_there() {
        // The width is what keeps the transport buttons in one place, so it
        // must hold at every offset — including the wrap-around.
        let mut state = playing("Everything In Its Right Place, and then some more words");
        assert!(state.scrolls(26));
        for _ in 0..200 {
            assert_eq!(state.marquee(26).chars().count(), 26);
            state.update(Event::Scroll);
        }
    }

    #[test]
    fn scrolling_comes_back_round_to_the_start() {
        let title = "A title comfortably longer than the window";
        let mut state = playing(title);
        let first = state.marquee(20);

        // One full lap is the title plus the gap between repeats.
        let lap = title.chars().count() + GAP.chars().count();
        for _ in 0..lap {
            state.update(Event::Scroll);
        }
        assert_eq!(state.marquee(20), first);
    }

    #[test]
    fn a_new_track_is_read_from_its_beginning() {
        // Otherwise the next song starts mid-word, wherever the last one had
        // scrolled to.
        let mut state = playing("A very long first track title indeed");
        for _ in 0..10 {
            state.update(Event::Scroll);
        }

        state.update(Event::Playing(Box::new(Now {
            title: "Second track".to_string(),
            ..Now::default()
        })));
        assert_eq!(state.marquee(26), "Second track");
    }

    #[test]
    fn a_multibyte_title_is_not_cut_mid_character() {
        // Character-based, not byte-based: slicing a String by bytes panics on
        // exactly this input.
        let mut state = playing("ゆらゆら帝国で考え中 — 空洞です、とても長いタイトル");
        assert!(state.scrolls(10));
        for _ in 0..50 {
            assert_eq!(state.marquee(10).chars().count(), 10);
            state.update(Event::Scroll);
        }
    }

    #[test]
    fn a_zero_width_asks_for_nothing_rather_than_dividing_by_zero() {
        let state = playing("Anything");
        assert!(!state.scrolls(0));
        assert_eq!(state.marquee(0), "Anything");
    }

    #[test]
    fn an_instance_suffix_is_dropped_from_a_bus_name() {
        // This is what was on screen: "chromium.instance16790".
        assert_eq!(
            readable_bus_name("org.mpris.MediaPlayer2.chromium.instance16790"),
            "chromium"
        );
    }

    #[test]
    fn a_plain_bus_name_is_left_alone() {
        assert_eq!(readable_bus_name("org.mpris.MediaPlayer2.vlc"), "vlc");
    }
}

/// One-shot read for `--check`.
pub async fn probe() -> Result<String, String> {
    match now_playing().await {
        Ok(Some(now)) => Ok(format!(
            "{} on {} ({})",
            if now.title.is_empty() {
                "nothing"
            } else {
                &now.title
            },
            now.player_name,
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
