name := 'cosmic-control-center-applet'
appid := 'dev.jamesjohn.CosmicControlCenter'

# User-local install by default. For system-wide: `just prefix=/usr install`.
prefix := env_var('HOME') / '.local'

bin-src := 'target' / 'release' / name
bin-dst := prefix / 'bin' / name
desktop := appid + '.desktop'
desktop-src := 'data' / desktop
desktop-dst := prefix / 'share' / 'applications' / desktop

_default:
    @just --list

build:
    cargo build --release

check:
    cargo clippy --all-targets -- -D warnings

fmt:
    cargo fmt --all

test:
    cargo test

# Everything CI runs, so a green local run means a green PR.
verify: fmt check test

install: build
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    @echo "Installed. Add it in Settings -> Desktop -> Panel -> Configure applets."

uninstall:
    rm -f {{bin-dst}} {{desktop-dst}}

# Applets expect to be launched by cosmic-panel as a layer-shell surface, so
# this mainly catches startup panics rather than showing a usable window.
run:
    RUST_LOG=debug cargo run
