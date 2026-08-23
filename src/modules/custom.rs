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
}

fn default_icon() -> String {
    "application-x-executable-symbolic".to_string()
}

impl Tile {
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
    /// the applet must not hold them or care when they finish.
    pub fn run(&self) {
        let Some((program, arguments)) = self.command.split_first() else {
            tracing::warn!("custom tile `{}` has no command", self.name);
            return;
        };

        match std::process::Command::new(program).args(arguments).spawn() {
            Ok(child) => tracing::debug!("ran `{}` as pid {}", self.name, child.id()),
            Err(err) => tracing::warn!("could not run `{}`: {err}", self.name),
        }
    }
}

/// Drop entries that cannot run, warning about each.
///
/// Filtering at load rather than at draw means a broken entry is reported once
/// at startup instead of being a tile that quietly does nothing forever.
pub fn usable(tiles: &[Tile]) -> Vec<Tile> {
    tiles
        .iter()
        .filter(|tile| {
            if tile.is_runnable() {
                true
            } else {
                tracing::warn!("ignoring custom tile `{}`: its command is empty", tile.name);
                false
            }
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
    if usable.len() != tiles.len() {
        summary.push_str(&format!(", {} ignored", tiles.len() - usable.len()));
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
        }
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
    fn icon_defaults_when_omitted() {
        let decoded: Tile = toml::from_str("name = \"Thing\"\ncommand = [\"true\"]\n").unwrap();
        assert_eq!(decoded.icon, default_icon());
    }
}
