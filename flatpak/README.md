# Flatpak packaging — the road to the COSMIC Store

The COSMIC Store lists applets from System76's own Flatpak remote, populated
from https://github.com/pop-os/cosmic-flatpak (applets are not accepted on
Flathub). Getting listed is a PR there adding
`app/io.github.jjnuthuagen.ControlCenter/` containing the two files in this
directory.

## Files

- `io.github.jjnuthuagen.ControlCenter.json` — the flatpak-builder manifest.
  Pins one release commit of this repo. **Update `commit` to the tagged
  release being submitted** — it must be a tag that contains the
  `flatpak-spawn --host` shim (v0.2.1 or later), or volume and custom tiles
  will not work inside the sandbox.
- `cargo-sources.json` — every crate the build needs, pinned, so the build
  runs offline. Regenerate whenever `Cargo.lock` changes:

  ```sh
  curl -sfLO https://raw.githubusercontent.com/flatpak/flatpak-builder-tools/master/cargo/flatpak-cargo-generator.py
  uv run --with aiohttp --with toml python flatpak-cargo-generator.py ../Cargo.lock -o cargo-sources.json
  ```

## Test build

```sh
flatpak remote-add --if-not-exists --user flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install --user flathub org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
flatpak-builder --user --install --force-clean build-dir io.github.jjnuthuagen.ControlCenter.json
```

Then add the applet from Settings → Desktop → Panel → Configure panel applets.

## Sandbox notes

Everything D-Bus-shaped (NetworkManager, BlueZ, UPower, power profiles,
logind brightness, MPRIS, COSMIC settings daemon) is granted via
`finish-args`. The two things that are *not* D-Bus — `wpctl`/`pactl` for
volume, and user-defined custom tiles — run on the host through
`flatpak-spawn --host` (see `crate::process::host_command`), which is what
`--talk-name=org.freedesktop.Flatpak` is for.
