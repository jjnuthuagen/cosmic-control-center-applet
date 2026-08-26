# Control Center — a COSMIC panel applet

A single panel button that opens the controls you actually reach for: Wi-Fi, Bluetooth, battery power profile, DNS profile, display, volume and brightness — laid out as a Quick-Settings-style grid with drill-down sub-menus.

Built in Rust with [libcosmic](https://github.com/pop-os/libcosmic) for the [COSMIC desktop](https://github.com/pop-os/cosmic-epoch).

> **Status: pre-alpha.** Nothing is implemented yet. This README describes the target.

## Layout

```
+---------------------------------------+
|  [ Wi-Fi    >]   |  [ Bluetooth >]    |
|  SSID: HomeNet   |  Devices: 2        |
+---------------------------------------+
|  [ Battery  >]   |  [ DNS Settings >] |
|  82% (Balanced)  |  Profile: Quad9    |
+---------------------------------------+
|                  |  [ Dark Mode ]     |
+---------------------------------------+
|  (speaker) [========= slider =======]  |
+---------------------------------------+
|  (sun)     [========= slider =======]  |
+---------------------------------------+
```

## The grid

Tiles come in four shapes on a four-sub-column grid:

| Shape | Footprint | Used by |
|---|---|---|
| Half | ½ × 1 | Icon only, four to a row — name and state on hover |
| Small | 1 × 1 | Most controls |
| Tall | 1 × 2 | Connectivity — Wi-Fi, Bluetooth and VPN stacked, a switch each |
| Wide | 2 × 1 | Volume, brightness, microphone: icon, track, percentage |

**Free placement.** A tile sits exactly where you put it. Nothing is packed
and nothing is re-flowed: a gap you leave is a gap you keep, and the same
control can appear more than once, at more than one size — a Half of it up
top for the glance, a Wide of it lower down for the detail.

**Arranging:** right-click the panel button → Tiles. The grid there is
exactly what the popup draws. Drag a tile to move it; it lands only on free
space, and if it will not fit, it snaps back and the cell it refused flashes
— nothing you did not drag ever moves. Hover a tile for the − that takes it
off. Controls that are not on the grid are listed underneath; tap one to add
it at the first free space.

**Being on the grid is what shows a control.** There is no second switch to
keep in agreement: place a control and it is drawn and its backend starts,
remove it and neither happens. Three controls have no tile of their own —
Media is a row under the grid, and Game Mode and the charge limit live inside
the Battery page — so those keep plain switches, on the Styling tab.

The layout is written to `config.toml` as `[[appearance.layout]]` blocks, one
per tile, each naming a control, a shape and a cell. A config from before
free placement is converted on first read and looks exactly as it did.

**How tiles are painted** is a separate choice, on the Styling tab, from
`solid` (a filled card), through `frosted` (the blur comes through, but the
tile stays denser than the popup so it still reads as a tile), to `outline`
(no fill at all — a faint edge, and one unbroken sheet of frosted glass
across the whole popup). The last two only look different from `solid` when
the desktop's frosted styling is on, since that is what makes the popup
translucent to begin with.

The group and the standalone tiles are just different controls you can
place: `connectivity` is the grouped tile, and `wifi`, `bluetooth` and `vpn`
are each a tile of their own. Place both and Wi-Fi appears twice, which is
allowed — some people want the group for switching and a big tile for the
SSID. Out of the box the group is placed and the three standalone tiles are
not.

Switching off a standalone tile does not empty that row inside the group:
the group's rows follow the hardware, and the module itself keeps running as
long as either wants it.

## Modules

| Module | Backend |
|---|---|
| Wi-Fi | `org.freedesktop.NetworkManager` |
| Bluetooth | `org.bluez` |
| Battery power profiles | `org.freedesktop.UPower.PowerProfiles` (power-profiles-daemon) + `org.freedesktop.UPower` |
| DNS profiles | NetworkManager connection settings |
| Brightness | `org.freedesktop.login1.Session.SetBrightness` (logind) |
| Brightness | logind `SetBrightness` |
| Volume | WirePlumber (`wpctl`), falling back to `pactl` |
| Microphone | WirePlumber / PulseAudio, default source |
| Keyboard backlight | logind `SetBrightness`, `leds` subsystem |
| Media | MPRIS (`org.mpris.MediaPlayer2`) |
| VPN | NetworkManager saved VPN profiles |
| Do Not Disturb | COSMIC notifications config |
| Keep awake | logind idle inhibitor |
| Charge limit | UPower `EnableChargeThreshold` |
| Game Mode | Feral GameMode (`com.feralinteractive.GameMode`) |
| Your own | Any command, via `[[custom]]` in the config |

Every backend is reachable **without root**, and that constraint drove the choices. Notably DNS goes through NetworkManager rather than systemd-resolved: `org.freedesktop.resolve1.set-dns-servers` is `auth_admin_keep` in polkit, so a resolved-based switcher would demand an administrator password on every single change. NetworkManager grants `settings.modify.own` outright.

The one case that can still prompt is a connection owned by the *system* rather than by you — that falls under `settings.modify.system`, which is `auth_admin_keep`. The applet tells you when this happens instead of failing silently.

Volume is the sole place the applet shells out. PipeWire exposes no D-Bus volume interface, and the real bindings (`libpulse-binding`, `pipewire-rs`) are C dependencies with their own mainloops. `wpctl` ships with WirePlumber, which COSMIC already requires. All process handling is confined to one type so a native binding can replace it later.

## Modularity

Modules are enabled and disabled in `config.toml`. A disabled module is never constructed and never connects to its bus, so desktop users can cleanly hide battery:

```toml
[modules]
wifi       = true
bluetooth  = true
battery    = false   # desktop — no battery hardware
dns        = true
volume     = true
brightness = true
```

Missing hardware or a missing daemon degrades the module gracefully; it never
crashes the applet. You do not need to disable `battery` on a desktop — the
percentage hides itself while the power-profile switch stays, since
power-profiles-daemon works fine without a battery. Turn a module off only when
the hardware works and you still don't want the tile.

## Your own tiles

Any command can become a tile — a screenshot, a script, a VPN toggle. Add a
`[[custom]]` block to `config.toml`:

```toml
[[custom]]
name = "Screenshot"
command = ["cosmic-screenshot", "--interactive=true"]
icon = "camera-photo-symbolic"     # optional
detail = "Select a region"         # optional second line
enabled = true                     # optional; false hides it without deleting it
```

Only `name` and `command` are required, so the shortest useful tile is two
lines:

```toml
[[custom]]
name = "Lock"
command = ["loginctl", "lock-session"]
```

`command` is a list rather than a string, and runs directly instead of through
a shell. An argument containing spaces stays one argument and nothing is
glob-expanded behind your back. If you want a pipeline, ask for one:

```toml
command = ["sh", "-c", "grim - | wl-copy"]
```

Right-click the panel button and open Settings to find these under **Your
tiles**, each with a switch, and a button that opens the folder holding
`config.toml`. The shipped [`config.example.toml`](data/config.example.toml)
documents every option with examples to copy.

## Building

Requires a Rust toolchain and the COSMIC/libcosmic build dependencies.

```sh
git clone https://github.com/jjnuthuagen/cosmic-control-center-applet
cd cosmic-control-center-applet
just install     # or: cargo build --release
```

`just install` puts the binary in `~/.local/bin` and the desktop entry in
`~/.local/share/applications`. Then add it in **Settings → Desktop → Panel →
Configure applets**.

`just verify` runs everything CI runs (fmt, clippy, tests), so a green local run
means a green pull request.

Runtime services expected (all optional — each gates only its own module):

- `NetworkManager`
- `bluez`
- `power-profiles-daemon`
- `systemd-resolved`
- `pipewire` / `wireplumber`

## Localisation

All user-facing strings live in `i18n/<lang>/main.ftl` ([Fluent](https://projectfluent.org/)). Translations are very welcome — copy `i18n/en/` and translate.

## Something missing?

A tile only appears when its backend is actually readable, so a missing tile
usually means missing hardware or a daemon that is not running. To find out
which:

```sh
cosmic-control-center-applet --check
```

It probes every backend and prints what it found. Please include its output in
any bug report.

Icons are chosen from state and resolved against your active icon theme, falling
back when a theme is missing a name. To see what your theme actually gives you:

```sh
cosmic-control-center-applet --icons
```

Anything listed as `image-missing-symbolic`, or the same name for two states that
should look different, is worth reporting along with your theme name.

## Contributing

Issues and PRs welcome. Before opening a PR:

```sh
just verify     # fmt, clippy -D warnings, tests — exactly what CI runs
```

See [TESTING.md](TESTING.md) for the manual UI checklist and the list of
regressions that have a test guarding them.

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
