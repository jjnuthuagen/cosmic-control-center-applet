# Packaging

## Arch / AUR

`PKGBUILD` builds from a tagged release tarball.

```sh
cd packaging
makepkg -si          # build and install locally
```

Notes for whoever maintains this:

- **`depends` is deliberately short.** It lists only what the binary links
  against. Everything the applet *talks to* — NetworkManager, BlueZ,
  power-profiles-daemon, UPower, WirePlumber — is discovered over D-Bus at
  runtime, and each module hides its tile when its service is absent. Those are
  `optdepends`. Promoting them to `depends` would drag NetworkManager onto a
  machine that only wanted the volume slider.
- **`sha256sums` is `SKIP`** until a tag exists. Run `updpkgsums` after tagging
  and commit the real hash — an AUR package with `SKIP` gives users no integrity
  check at all.
- `check()` runs the test suite. Every test is hermetic (nothing touches D-Bus,
  sysfs or the network), so it is safe in a clean chroot. Verifying the real
  machine is what `cosmic-control-center-applet --check` is for, after install.

Publishing needs an AUR account, an SSH key registered with it, and:

```sh
git clone ssh://aur@aur.archlinux.org/cosmic-control-center-applet.git aur
cp packaging/PKGBUILD aur/
cd aur && makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO && git commit -m "Initial release 0.1.0" && git push
```

## Prebuilt binary

`.github/workflows/release.yml` builds on tag push and attaches a tarball to the
GitHub Release. The tarball contains the binary, the desktop entry, the example
config, both licences and an `install.sh` that puts them in `~/.local`.
