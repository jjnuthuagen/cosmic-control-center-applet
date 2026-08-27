//! User-defined tiles that run a command.
//!
//! Everything else in this applet is a control someone decided to build. This
//! is the escape hatch: a screenshot button, a VPN script, a "mute all the
//! things" shortcut — whatever the built-in list does not cover.
//!
//! # No shell
//!
//! `command` is an argv array, not a string, and it is executed directly rather
//! than through `sh -c`. That is deliberate. The config file is the user's own,
//! so this is not a privilege boundary — it is the same trust level as their
//! shell profile — but running everything through a shell would mean quoting
//! rules, word splitting and glob expansion silently changing what a command
//! does. An argv array does exactly and only what it says.
//!
//! Anyone who genuinely wants a pipeline can still write
//! `command = ["sh", "-c", "..."]` and has then chosen that explicitly.

use serde::{Deserialize, Serialize};

/// One user-defined tile.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Tile {
    /// Shown on the tile, and in its tooltip.
    pub name: String,
    /// Icon-theme name. Resolved like any other, so a name the theme lacks
    /// falls back rather than rendering blank.
    #[serde(default = "default_icon")]
    pub icon: String,
    /// Program and its arguments. The first element is the program.
    pub command: Vec<String>,
    /// Optional second line, for whatever the name does not make obvious.
    #[serde(default)]
    pub detail: Option<String>,
    /// The footprint this tile draws at.
    ///
    /// `half` is the icon-only form, four to a row — the right shape for a
    /// launcher, where the glyph is the whole message and the name is in the
    /// tooltip. Defaults to `small` so a config written before this field
    /// existed draws exactly as it did.
    ///
    /// A custom tile has no [`crate::tile_layout::TileKey`], so unlike the
    /// built-in controls there is no default shape to fall back to and no
    /// `shape_with` rule to consult: whatever is written here is what is
    /// drawn.
    #[serde(default = "default_shape")]
    pub shape: crate::tile_layout::TileShape,
    /// Ask before running it.
    ///
    /// For the commands you cannot take back: logging out, rebooting, powering
    /// off. A tile is a small target in a grid people click around in, and
    /// every other tile in that grid is harmless, so the one that ends your
    /// session should not act on the same flick of the wrist.
    #[serde(default)]
    pub confirm: bool,
    /// Whether the tile is drawn.
    ///
    /// Defaults to true, so a config written before this field existed keeps
    /// showing its tiles. It exists because the Settings window had no way to
    /// switch a custom tile off — the built-in controls each had a toggle and
    /// these did not, which made a tile you had added yourself the one thing in
    /// the popup you could only remove by editing the file by hand.
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

fn default_shape() -> crate::tile_layout::TileShape {
    crate::tile_layout::TileShape::Small
}

fn default_icon() -> String {
    "application-x-executable-symbolic".to_string()
}

impl Tile {
    /// Whether this entry should be drawn at all: switched on, and able to
    /// run.
    pub fn is_usable(&self) -> bool {
        self.enabled && self.is_runnable()
    }

    /// Whether this entry can actually be run.
    ///
    /// An empty `command` would otherwise produce a tile that looks identical
    /// to a working one and does nothing when pressed.
    pub fn is_runnable(&self) -> bool {
        self.command
            .first()
            .is_some_and(|program| !program.is_empty())
    }

    /// Launch it, detached.
    ///
    /// Nothing is awaited: these are user commands that may run for hours, and
    /// the applet must not hold them or care when they finish. It is still
    /// reaped — see [`crate::process::spawn_and_reap`].
    pub fn run(&self) {
        let Some((program, arguments)) = self.command.split_first() else {
            tracing::warn!("custom tile `{}` has no command", self.name);
            return;
        };

        let mut command = std::process::Command::new(program);
        command.args(arguments);
        // Reaped in the background. Nothing waits for the result, but something
        // has to collect it — the applet is the parent and runs for the whole
        // session, so an unreaped child stays a zombie until it exits.
        match crate::process::spawn_and_reap(command) {
            Ok(pid) => tracing::debug!("ran `{}` as pid {pid}", self.name),
            Err(err) => tracing::warn!("could not run `{}`: {err}", self.name),
        }
    }
}

/// The launchers a fresh install starts with.
///
/// Icon-only, so they read as a strip of buttons under the controls rather
/// than competing with them, and **monochrome** throughout: an app's own
/// colour icon beside a symbolic one makes a row of eight look like a
/// collection of stickers. Every name here is a standard freedesktop one, so
/// it resolves in whatever icon theme the user is on.
///
/// These are only ever *offered* — [`Config::load`] writes the subset whose
/// program actually exists, so a machine without COSMIC Tweaks does not get a
/// tile that does nothing. After that they are ordinary custom tiles: edit,
/// reshape or delete them like any other.
///
/// [`Config::load`]: crate::config::Config::load
pub fn default_launchers() -> Vec<Tile> {
    use crate::tile_layout::TileShape;

    let tile = |name: &str, icon: &str, command: &[&str], detail: Option<&str>| Tile {
        name: name.to_string(),
        icon: icon.to_string(),
        command: command.iter().map(|s| s.to_string()).collect(),
        detail: detail.map(str::to_string),
        shape: TileShape::Half,
        confirm: false,
        enabled: true,
    };

    // The ones that end the session ask first.
    let ending = |name: &str, icon: &str, command: &[&str], detail: &str| Tile {
        confirm: true,
        ..tile(name, icon, command, Some(detail))
    };

    let mut tiles = vec![
        tile(
            "Settings",
            "preferences-system-symbolic",
            &["cosmic-settings"],
            None,
        ),
        tile(
            "Tweaks",
            "preferences-other-symbolic",
            &["flatpak", "run", "dev.edfloreshz.CosmicTweaks"],
            None,
        ),
        tile(
            "Terminal",
            "utilities-terminal-symbolic",
            &["cosmic-term"],
            None,
        ),
        tile(
            "System Monitor",
            "utilities-system-monitor-symbolic",
            &["cosmic-monitor"],
            None,
        ),
        // No symbolic Claude glyph exists in any icon theme, and drawing one
        // would be someone else's trademark. A speech bubble says "assistant"
        // and stays monochrome with the rest.
        tile(
            "Claude",
            "chat-message-new-symbolic",
            &["claude-desktop"],
            None,
        ),
        // The two that end the machine. They run on a single press, like
        // every other tile — see the note in the example config.
        ending(
            "Restart",
            "system-reboot-symbolic",
            &["systemctl", "reboot"],
            "Reboots now",
        ),
        ending(
            "Power off",
            "system-shutdown-symbolic",
            &["systemctl", "poweroff"],
            "Shuts down now",
        ),
    ];

    // Logging out means naming the user to log out, and the command is an
    // argv array with no shell to expand `$USER` for us. Without a name there
    // is nothing sensible to run, so the tile is simply not offered.
    if let Some(user) = std::env::var_os("USER").and_then(|u| u.into_string().ok()) {
        tiles.push(ending(
            "Log out",
            "system-log-out-symbolic",
            &["loginctl", "terminate-user", &user],
            "Ends the session",
        ));
    }

    tiles
}

/// The subset of `tiles` whose program is actually on this machine.
///
/// Used for the shipped defaults only. A user's own tile is never dropped for
/// this: they may be pointing at something that comes and goes, and silently
/// deleting it from their config would be worse than a tile that reports a
/// failure when pressed.
pub fn installed(tiles: Vec<Tile>) -> Vec<Tile> {
    tiles.into_iter().filter(is_present).collect()
}

/// Whether the thing a shipped launcher launches is actually here.
///
/// `which` on the program is not enough for `flatpak run <app>`: every machine
/// with Flatpak has `flatpak`, so the tile would be offered on all of them and
/// do nothing on most. The app id is the thing to look for.
fn is_present(tile: &Tile) -> bool {
    let Some(program) = tile.command.first() else {
        return false;
    };
    if which(program).is_none() {
        return false;
    }
    if program.ends_with("flatpak") && tile.command.get(1).is_some_and(|arg| arg == "run") {
        let Some(app_id) = tile.command.get(2) else {
            return false;
        };
        return flatpak_installed(app_id);
    }
    true
}

/// Whether a Flatpak app id is installed, per `flatpak info`.
///
/// Asks Flatpak rather than looking for a directory: user and system installs
/// live in different places, and which one an app is in is not this applet's
/// business.
fn flatpak_installed(app_id: &str) -> bool {
    std::process::Command::new("flatpak")
        .args(["info", app_id])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Warn about entries that cannot run.
///
/// Reported once at load rather than at draw, so a broken entry is a line in
/// the log instead of a tile that quietly does nothing forever.
pub fn warn_unusable(tiles: &[Tile]) {
    for tile in tiles.iter().filter(|tile| !tile.is_runnable()) {
        tracing::warn!("custom tile `{}` has an empty command", tile.name);
    }
}

/// The tiles that should be drawn: switched on, and able to run.
///
/// Filtering at load rather than at draw means a broken entry is reported once
/// at startup instead of being a tile that quietly does nothing forever.
pub fn usable(tiles: &[Tile]) -> Vec<Tile> {
    tiles
        .iter()
        .filter(|tile| {
            if !tile.is_runnable() {
                tracing::warn!("ignoring custom tile `{}`: its command is empty", tile.name);
                return false;
            }
            tile.enabled
        })
        .cloned()
        .collect()
}

/// One-shot report for `--check`.
pub fn probe(tiles: &[Tile]) -> Result<String, String> {
    if tiles.is_empty() {
        return Err("none defined".to_string());
    }

    let usable = usable(tiles);
    let missing: Vec<&str> = usable
        .iter()
        .filter(|tile| which(&tile.command[0]).is_none())
        .map(|tile| tile.name.as_str())
        .collect();

    let mut summary = format!("{} tile(s)", usable.len());
    // Switched off and broken are different answers to "why is my tile
    // missing?", and this report exists to tell them apart.
    let off = tiles.iter().filter(|tile| !tile.enabled).count();
    if off > 0 {
        summary.push_str(&format!(", {off} switched off"));
    }
    let broken = tiles.len() - usable.len() - off;
    if broken > 0 {
        summary.push_str(&format!(", {broken} ignored"));
    }
    if !missing.is_empty() {
        // Worth saying: the tile will draw and then fail on press, which is
        // exactly the kind of thing this report exists to surface.
        summary.push_str(&format!(", program not found for: {}", missing.join(", ")));
    }
    Ok(summary)
}

/// Is `program` runnable — an existing path, or something on `PATH`?
fn which(program: &str) -> Option<std::path::PathBuf> {
    let candidate = std::path::Path::new(program);
    if candidate.is_absolute() || program.contains('/') {
        return candidate.is_file().then(|| candidate.to_path_buf());
    }

    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(program))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(name: &str, command: Vec<&str>) -> Tile {
        Tile {
            name: name.to_string(),
            icon: default_icon(),
            command: command.into_iter().map(str::to_string).collect(),
            detail: None,
            shape: default_shape(),
            confirm: false,
            enabled: true,
        }
    }

    #[test]
    fn the_shipped_launchers_are_icon_only_and_monochrome() {
        let launchers = default_launchers();
        assert!(!launchers.is_empty());
        for tile in &launchers {
            assert_eq!(
                tile.shape,
                crate::tile_layout::TileShape::Half,
                "{} is not icon-only",
                tile.name
            );
            // An app's own colour icon beside a symbolic one makes the row
            // look like a collection of stickers.
            assert!(
                tile.icon.ends_with("-symbolic"),
                "{} uses {}, which is not a monochrome icon",
                tile.name,
                tile.icon
            );
            assert!(tile.is_runnable(), "{} has no command", tile.name);
        }
    }

    #[test]
    fn a_flatpak_launcher_needs_the_app_not_just_flatpak() {
        // Every machine with Flatpak has `flatpak`, so checking the program
        // alone would offer the tile everywhere and have it do nothing on most
        // of them.
        let absent_app = tile("Tweaks", vec!["flatpak", "run", "org.example.NotInstalled"]);
        if which("flatpak").is_some() {
            assert!(
                !is_present(&absent_app),
                "an absent flatpak app was offered"
            );
        }

        // A plain program is still judged on the program alone.
        assert!(is_present(&tile("Shell", vec!["sh"])));
        assert!(!is_present(&tile("Nope", vec!["definitely-not-real-xyz"])));
    }

    #[test]
    fn everything_that_ends_the_session_asks_first() {
        // These go out to every install by default. A tile is a small target
        // in a grid people click around in, and the rest of that grid is
        // harmless.
        for name in ["Log out", "Restart", "Power off"] {
            let Some(tile) = default_launchers().into_iter().find(|t| t.name == name) else {
                // Log out is not offered when USER is unset; the others always
                // are.
                assert_eq!(name, "Log out");
                continue;
            };
            assert!(tile.confirm, "{name} runs without asking");
        }

        // And nothing else nags: a launcher that just opens a window should
        // open it.
        for tile in default_launchers().iter().filter(|t| !t.confirm) {
            assert!(
                !["Log out", "Restart", "Power off"].contains(&tile.name.as_str()),
                "{} should have asked",
                tile.name
            );
        }
    }

    #[test]
    fn a_tile_runs_without_asking_unless_it_says_otherwise() {
        // Confirmation is opt-in: a config written before it existed keeps
        // behaving as it did.
        let older: Tile = toml::from_str("name = \"X\"\ncommand = [\"true\"]\n").unwrap();
        assert!(!older.confirm);

        let asks: Tile =
            toml::from_str("name = \"X\"\ncommand = [\"true\"]\nconfirm = true\n").unwrap();
        assert!(asks.confirm);
        // Round-trips, or the Settings window would drop it on save.
        assert_eq!(
            toml::from_str::<Tile>(&toml::to_string(&asks).unwrap()).unwrap(),
            asks
        );
    }

    #[test]
    fn a_launcher_is_only_offered_when_its_program_exists() {
        // The defaults are offered to every machine, so one whose program is
        // absent must not become a tile that quietly does nothing.
        let present = tile("Present", vec!["sh"]);
        let absent = tile("Absent", vec!["definitely-not-a-real-program-xyz"]);
        let kept = installed(vec![present.clone(), absent]);
        assert_eq!(kept, vec![present]);
    }

    #[test]
    fn logging_out_names_the_user_because_there_is_no_shell_to_expand_it() {
        let Some(logout) = default_launchers()
            .into_iter()
            .find(|t| t.name == "Log out")
        else {
            // No USER in the environment: the tile is not offered at all,
            // which is the other half of the rule.
            assert!(std::env::var_os("USER").is_none());
            return;
        };
        assert_eq!(logout.command[0], "loginctl");
        let user = std::env::var("USER").unwrap();
        assert_eq!(logout.command.last().unwrap(), &user);
        // A literal, not something a shell would have to expand.
        assert!(!logout.command.iter().any(|a| a.contains('$')));
    }

    #[test]
    fn a_tile_without_a_shape_stays_the_plain_square() {
        // A config written before `shape` existed must draw exactly as it did,
        // so the default is Small rather than the icon-only Half.
        let older: Tile = toml::from_str("name = \"X\"\ncommand = [\"true\"]\n").unwrap();
        assert_eq!(older.shape, crate::tile_layout::TileShape::Small);

        let launcher: Tile =
            toml::from_str("name = \"X\"\ncommand = [\"true\"]\nshape = \"half\"\n").unwrap();
        assert_eq!(launcher.shape, crate::tile_layout::TileShape::Half);

        // And it round-trips, or the Settings window would drop it on save.
        let encoded = toml::to_string(&launcher).unwrap();
        assert_eq!(toml::from_str::<Tile>(&encoded).unwrap(), launcher);
    }

    #[test]
    fn an_empty_command_is_not_runnable() {
        // Otherwise it draws like any other tile and does nothing on press.
        assert!(!tile("Broken", vec![]).is_runnable());
        assert!(!tile("Blank", vec![""]).is_runnable());
        assert!(tile("Fine", vec!["true"]).is_runnable());
    }

    #[test]
    fn unusable_entries_are_dropped_at_load() {
        let tiles = vec![
            tile("Good", vec!["true"]),
            tile("Broken", vec![]),
            tile("Also good", vec!["echo", "hello"]),
        ];
        let usable = usable(&tiles);
        assert_eq!(usable.len(), 2);
        assert!(usable.iter().all(|t| t.name != "Broken"));
    }

    #[test]
    fn a_command_is_argv_not_a_shell_string() {
        // The whole point: arguments containing spaces stay one argument, and
        // nothing is glob-expanded or word-split behind the user's back.
        let t = tile("Say", vec!["echo", "hello world", "*.txt"]);
        assert_eq!(t.command.len(), 3);
        assert_eq!(t.command[1], "hello world");
        assert_eq!(t.command[2], "*.txt");
    }

    #[test]
    fn programs_on_path_are_found_and_nonsense_is_not() {
        assert!(which("sh").is_some());
        assert!(which("definitely-not-a-real-program-xyzzy").is_none());
    }

    #[test]
    fn an_absolute_path_is_checked_directly() {
        assert!(which("/bin/sh").is_some() || which("/usr/bin/sh").is_some());
        assert!(which("/nonexistent/program").is_none());
    }

    #[test]
    fn probe_reports_a_missing_program() {
        let tiles = vec![tile("Ghost", vec!["definitely-not-a-real-program-xyzzy"])];
        let report = probe(&tiles).unwrap();
        assert!(report.contains("Ghost"), "{report}");
    }

    #[test]
    fn probe_says_when_none_are_defined() {
        assert!(probe(&[]).is_err());
    }

    #[test]
    fn a_tile_round_trips_through_toml() {
        let original = tile(
            "Screenshot",
            vec!["cosmic-screenshot", "--interactive=true"],
        );
        let encoded = toml::to_string_pretty(&original).unwrap();
        let decoded: Tile = toml::from_str(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn a_tile_is_shown_unless_it_is_switched_off() {
        let mut tiles = vec![tile("On", vec!["true"]), tile("Off", vec!["true"])];
        tiles[1].enabled = false;

        let shown = usable(&tiles);
        assert_eq!(shown.len(), 1);
        assert_eq!(shown[0].name, "On");
    }

    #[test]
    fn a_config_written_before_the_switch_existed_still_shows_its_tiles() {
        // Anyone whose config.toml predates `enabled` must not silently lose
        // every custom tile on upgrade.
        let decoded: Tile = toml::from_str("name = \"Thing\"\ncommand = [\"true\"]\n").unwrap();
        assert!(decoded.enabled);
        assert_eq!(usable(&[decoded]).len(), 1);
    }

    #[test]
    fn icon_defaults_when_omitted() {
        let decoded: Tile = toml::from_str("name = \"Thing\"\ncommand = [\"true\"]\n").unwrap();
        assert_eq!(decoded.icon, default_icon());
    }
}
