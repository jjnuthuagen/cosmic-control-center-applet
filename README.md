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
