#!/usr/bin/env python3
"""Add the source replacements that flatpak-cargo-generator.py drops.

The generator names each `[source.*]` section after the git URL, so when one
repository appears in Cargo.lock under two different specs — libcosmic is
pulled in both as `?rev=<sha>` (by this crate) and bare (by cosmic-panel's
`cosmic-config` dependency) — the second section overwrites the first and only
one spec ends up replaced. Cargo treats every (url, spec) pair as a distinct
source, so the unreplaced one is still fetched from the network and the
offline flatpak build dies with:

    can't checkout from 'https://github.com/pop-os/libcosmic':
    you are in the offline mode (--offline)

Both specs resolve to the same commit in Cargo.lock, so one vendored copy
serves both; they just each need naming. Run this after regenerating
cargo-sources.json.
"""

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

HERE = Path(__file__).parent
LOCK = HERE.parent / "Cargo.lock"
SOURCES = HERE / "cargo-sources.json"


def canonical(url: str) -> str:
    """Cargo ignores a trailing .git when deciding source identity."""
    return url[:-4] if url.endswith(".git") else url


def lockfile_specs() -> dict[str, set[str]]:
    specs: dict[str, set[str]] = defaultdict(set)
    for raw in set(
        re.findall(r'source = "git\+([^"#]+)(?:#[0-9a-f]+)?"', LOCK.read_text())
    ):
        url, _, query = raw.partition("?")
        specs[canonical(url)].add(query)
    return specs


def config_entry(sources: list) -> dict:
    for entry in sources:
        if entry.get("type") == "inline" and entry.get("dest-filename") == "config":
            return entry
    sys.exit("no inline cargo `config` entry in cargo-sources.json")


def covered(config: str) -> dict[str, set[str]]:
    """Map each replaced git URL to the specs it is replaced for."""
    found: dict[str, set[str]] = defaultdict(set)
    for block in config.split("[source.")[1:]:
        git = re.search(r'^git = "([^"]+)"', block, re.M)
        if not git:
            continue
        for key in ("rev", "tag", "branch"):
            if match := re.search(rf'^{key} = "([^"]+)"', block, re.M):
                found[canonical(git.group(1))].add(f"{key}={match.group(1)}")
                break
        else:
            found[canonical(git.group(1))].add("")
    return found


def main() -> None:
    sources = json.loads(SOURCES.read_text())
    entry = config_entry(sources)
    config = entry["contents"]
    have = covered(config)

    added = []
    for url, specs in sorted(lockfile_specs().items()):
        for spec in sorted(specs):
            # A pinned rev may be abbreviated in the generated config.
            if any(
                spec == seen
                or (spec.startswith("rev=") and seen.startswith("rev=")
                    and (spec[4:].startswith(seen[4:]) or seen[4:].startswith(spec[4:])))
                for seen in have[url]
            ):
                continue
            key, _, value = spec.partition("=")
            name = f"{url}#{spec}" if spec else f"{url}#default"
            block = [f'[source."{name}"]', f'git = "{url}"',
                     "replace-with = \"vendored-sources\""]
            if spec:
                block.append(f'{key} = "{value}"')
            config += "\n" + "\n".join(block) + "\n"
            added.append(name)

    if not added:
        print("cargo-sources.json already covers every git spec in Cargo.lock")
        return

    entry["contents"] = config
    SOURCES.write_text(json.dumps(sources, indent=4) + "\n")
    for name in added:
        print(f"added source replacement: {name}")


if __name__ == "__main__":
    main()
