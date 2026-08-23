# Testing

Three layers, in the order you should use them.

## 1. `--check` — does this machine's hardware work?

```sh
cosmic-control-center-applet --check
```

Probes every backend through the same read paths the applet uses and prints what
each found:

```
modules:
  wifi        ok       radio on, 7 network(s) visible, connected to HomeNet
  bluetooth   ok       /org/bluez/hci0 is on, 3 device(s) known, 1 connected
  battery     ok       battery: 80%; profiles: balanced of [power-saver, balanced, performance]
  dns         ok       HomeNet: automatic (DHCP)
  volume      ok       Wpctl, 45%
  brightness  ok       intel_backlight (max 19200), currently 60%
  gamemode    ok       0 client(s) registered, idle
  tiling      ok       active workspace is tiled
  desktop     ok       theme is dark
```

**This is the first thing to run when a tile is missing.** A tile that is not
drawn and a module that is switched off look identical in the UI; this tells them
apart. `MISSING` lines say why, and the exit code is non-zero if an *enabled*
module could not be read. A disabled module, or absent hardware the module
correctly reports, is not a failure.

Please paste its output into any bug report.

## 2. `just verify` — does the code hold together?

```sh
just verify     # cargo fmt --all, clippy -D warnings, cargo test
```

Exactly what CI runs — but **necessary, not sufficient**. CI uses the latest
stable Rust; a distro toolchain a release behind will miss lints CI enforces.
That has already happened once: `clippy::result_large_err` fired on 1.98 and not
on the 1.97 package, so three pushes went out with red CI while `just verify` was
green locally. If they disagree, believe CI.

The unit tests cover the parts that are worth pinning down: security-kind
decisions for a network, per-SSID collapsing, address encoding, output parsing
for both audio backends, brightness scaling, availability gating, and the
specific regressions listed below. They do **not** cover layout — see section 3.

### Regressions with a test guarding them

| Test | What it prevents |
|---|---|
| `a_tile_is_always_tall_enough_for_its_icon` | A hardcoded tile height smaller than its padding plus icon, which clipped the icons. |
| `repeated_toggles_alternate` | Dark mode deriving its target from a cached theme, so it only worked once. |
| `a_desktop_keeps_its_profile_switch` | Gating power profiles on battery presence, removing them from every desktop. |
| `addresses_round_trip` | Byte-order confusion in NetworkManager's `ipv4.dns`, which reverses every address. |
| `collapsing_keeps_known_and_connected_from_any_duplicate` | Losing the saved-profile flag on a mesh, making a known network ask for its password again. |
| `multibyte_text_is_not_cut_mid_character` | A panic when eliding an SSID containing non-ASCII characters. |
| `automatic_clears_the_override` | Leaving `ignore-auto-dns` set, suppressing DHCP resolvers while offering none. |
| `an_unknown_enterprise_network_is_not_offered_a_password_box` | A password field that cannot possibly work for 802.1X. |
| `undimming_without_a_remembered_level_still_lights_the_screen` | Restoring brightness to nothing, leaving a black screen and a dead toggle. |
| `a_game_holding_gamemode_cannot_be_switched_off_here` | The applet yanking GameMode out from under a running game. |
| `battery_buckets_never_leave_the_family` | An out-of-range percentage producing an icon name no theme ships. |
| `an_empty_candidate_list_yields_the_spec_fallback` | An icon lookup returning nothing and rendering blank. |

## 3. Manual checklist — the UI

Layout, hit targets and drag behaviour cannot be asserted in a unit test. Walk
this before tagging a release. **Do it twice, once in each theme**, since the
tiles, the ghost slot and the accent fill are all theme-derived.

### Panel and popup
- [ ] Icon appears on the panel and is vertically centred.
- [ ] Clicking opens the popup; clicking again closes it.
- [ ] Clicking elsewhere dismisses it.
- [ ] Reopening always lands on the tile grid, never on a sub-page you left open.

### Tile grid
- [ ] No icon is clipped, top or bottom.
- [ ] All tiles are the same height and the two columns are equal width.
- [ ] An odd number of tiles leaves a faint ghost slot, not a gap.
- [ ] Hovering a tile shows its name.
- [ ] A long SSID or device name elides with `…` rather than overflowing or wrapping.
- [ ] Tiles that are "on" show the accent fill.

### Sliders
- [ ] Volume and brightness handles sit at the current value on open.
- [ ] Dragging is smooth and the effect follows immediately.
- [ ] Brightness never reaches fully black.
- [ ] Pressing the speaker icon mutes; the track goes inert and cannot be dragged.
- [ ] Pressing it again unmutes and the slider becomes draggable at the old level.
- [ ] Pressing the brightness icon drops to minimum and makes that track inert too.
- [ ] Pressing it again returns to the level it was at before.
- [ ] Changing volume with the keyboard keys updates the slider within ~1s.

### Wi-Fi page
- [ ] Back returns to the grid.
- [ ] Airplane mode and Wi-Fi are switches, visibly different from the list rows below.
- [ ] Airplane mode toggles, and the Wi-Fi row disappears while it is on.
- [ ] Turning the radio off empties the list; turning it on repopulates within a few seconds.
- [ ] Networks are sorted connected first, then known, then by strength.
- [ ] A mesh publishing one SSID from several radios appears once.
- [ ] Selecting a saved network connects with no password prompt.
- [ ] Selecting an unknown secured network opens a password field under that row.
- [ ] A wrong password shows the failure and leaves the field open to retry.
- [ ] Cancel closes the field and clears it.
- [ ] An unknown enterprise network offers no password box, and says to use Settings.
- [ ] IPv4, IPv6 and MAC appear at the bottom while connected.
- [ ] With many networks in range the page scrolls and nothing is cut off.

### Bluetooth page
- [ ] The adapter control is a switch, not a tinted button.
- [ ] Adapter switch turns the radio on and off.
- [ ] Powering off empties the device list.
- [ ] Paired devices connect and disconnect on tap.
- [ ] A connect in progress shows "Connecting…".
- [ ] Unpaired devices are listed but not tappable, and say to pair in Settings.

### Battery and DNS pages
- [ ] Only profiles the daemon actually supports are listed.
- [ ] Game Mode appears below a divider, separate from the profiles, when gamemoded is installed.
- [ ] Toggling Game Mode on and off works, and `--check` reports the client count changing.
- [ ] Launching a game with `gamemoderun` shows Game Mode on and the switch disabled.
- [ ] Selecting a profile marks it immediately and it survives reopening.
- [ ] A degraded-performance reason appears when the daemon reports one.
- [ ] Selecting a DNS provider marks it and changes the tile's state text.
- [ ] Manual entry accepts `1.1.1.1, 1.0.0.1`; Apply stays disabled until it parses.
- [ ] On a system-owned connection, the authorisation notice appears rather than silence.

### Quick toggles
- [ ] Dark mode switches the whole desktop, **and keeps switching on repeated presses**.
- [ ] Tiling actually retiles the **current** workspace, not just future ones.
- [ ] `cosmic-control-center-applet --toggle-tiling` flips it from a terminal and prints the new state.

### Icons
Icons are chosen from state and resolved against the active theme, so this is
worth a pass in more than one icon theme.
- [ ] Battery icon tracks the level, and shows charging separately from discharging.
- [ ] A nearly-flat battery that is not charging shows the caution icon.
- [ ] Bluetooth icon differs between off, on-with-nothing-connected, and connected.
- [ ] Wi-Fi icon differs between airplane, off, disconnected, and connected — and tracks signal strength.
- [ ] Tiling icon differs between tiled and floating.
- [ ] Volume and brightness icons track their level and their muted/dimmed state.
- [ ] Switch to a sparse icon theme: icons fall back to something sensible, and none render blank.

### Degraded machines
Worth checking on anything other than a laptop:
- [ ] Desktop with no battery: no percentage, but power profiles still work.
- [ ] No Bluetooth adapter: the tile is absent, and the grid stays square.
- [ ] Every module set to `false` in `config.toml`: the popup shows sliders only, no empty rows.

## If theming looks wrong

Applet popups follow the **panel's** appearance, not the desktop light/dark
switch. Check `~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/background`:
if it is `Dark` or `Light` rather than `ThemeDefault`, the panel is pinned and
every applet — stock ones included — will ignore the desktop theme. `opacity`
alongside it is the frosted-glass control.

Note also that cosmic-panel restarts every applet when the theme changes, so an
open popup is destroyed rather than restyled.

## Rebuilding onto the panel

Killing the process is **not** enough — cosmic-panel does not respawn a dead
applet. It spawns them when `plugins_wings` changes:

```sh
just install
# then remove and re-add the entry in
# ~/.config/cosmic/com.system76.CosmicPanel.Panel/v1/plugins_wings
```
