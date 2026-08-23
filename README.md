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
| DNS profiles | `org.freedesktop.resolve1` (systemd-resolved), falling back to NetworkManager connection settings |
| Brightness | `org.freedesktop.login1.Session.SetBrightness` (logind) |
| Volume | PipeWire / WirePlumber |

Every backend is reachable **without root** — the applet never escalates privileges.

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

Missing hardware or a missing daemon degrades the module gracefully; it never crashes the applet.

## Building

Requires a Rust toolchain and the COSMIC/libcosmic build dependencies.

```sh
git clone https://github.com/jjnuthuagen/cosmic-control-center-applet
cd cosmic-control-center-applet
cargo build --release
```

Runtime services expected (all optional — each gates only its own module):

- `NetworkManager`
- `bluez`
- `power-profiles-daemon`
- `systemd-resolved`
- `pipewire` / `wireplumber`

## Localisation

All user-facing strings live in `i18n/<lang>/main.ftl` ([Fluent](https://projectfluent.org/)). Translations are very welcome — copy `i18n/en/` and translate.

## Contributing

Issues and PRs welcome. Before opening a PR:

```sh
cargo fmt --check
cargo clippy -- -D warnings
```

## Licence

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
