#!/usr/bin/env python3
"""Cut a release from the changelog fragments in changes/ (#186).

A PR adds `changes/<issue>.<added|changed|fixed>.md` with its changelog
bullet(s) and never touches the version, so PRs merged in parallel do not
collide. `make release` collects the fragments into a new `## [X.Y.Z] - date`
section of CHANGELOG.md, bumps the version (minor when anything was added or
changed, else patch) in Cargo.toml and README.md, and deletes the fragments.
Cargo.lock follows with `cargo metadata`. Standard library only.
"""

from __future__ import annotations

import datetime
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHANGES = ROOT / "changes"
KINDS = {"added": "Added", "changed": "Changed", "fixed": "Fixed"}
FRAGMENT = re.compile(r"^(\d+)\.(added|changed|fixed)\.md$")


def fragments(folder: Path) -> list[tuple[int, str, str]]:
    """(issue, kind, text) of every fragment, by kind then issue; an unknown
    name is an error before anything is written."""
    found = []
    for path in sorted(folder.glob("*.md")):
        if path.name == "README.md":
            continue
        m = FRAGMENT.match(path.name)
        if not m:
            raise SystemExit(f"{path.name}: a fragment is <issue>.<added|changed|fixed>.md")
        text = path.read_text().strip()
        if not text:
            raise SystemExit(f"{path.name}: empty")
        found.append((int(m.group(1)), m.group(2), text))
    order = list(KINDS)
    return sorted(found, key=lambda f: (order.index(f[1]), f[0]))


def bump(version: str, frags: list[tuple[int, str, str]]) -> str:
    """Minor when anything was added or changed, else patch."""
    major, minor, patch = (int(x) for x in version.split("."))
    if any(kind != "fixed" for _, kind, _ in frags):
        return f"{major}.{minor + 1}.0"
    return f"{major}.{minor}.{patch + 1}"


def section(version: str, date: str, frags: list[tuple[int, str, str]]) -> str:
    """The new changelog section: a heading per kind, each fragment's bullets."""
    out = [f"## [{version}] - {date}", ""]
    for kind, title in KINDS.items():
        texts = [t for _, k, t in frags if k == kind]
        if not texts:
            continue
        out += [f"### {title}", ""]
        for t in texts:
            out.append(t if t.startswith("- ") else f"- {t}")
        out.append("")
    return "\n".join(out) + "\n"


def release(root: Path, date: str) -> str | None:
    """Write the release into the files under `root`; the new version, or None
    when there is nothing to release."""
    frags = fragments(root / "changes")
    if not frags:
        return None
    cargo = root / "Cargo.toml"
    text = cargo.read_text()
    m = re.search(r'^version = "(\d+\.\d+\.\d+)"', text, re.M)
    if not m:
        raise SystemExit("Cargo.toml: no version")
    old = m.group(1)
    new = bump(old, frags)
    changelog = root / "CHANGELOG.md"
    log = changelog.read_text()
    marker = "## [Unreleased]\n\n"
    if marker not in log:
        raise SystemExit("CHANGELOG.md: no '## [Unreleased]' heading")
    changelog.write_text(log.replace(marker, marker + section(new, date, frags), 1))
    cargo.write_text(text.replace(m.group(0), f'version = "{new}"', 1))
    readme = root / "README.md"
    if readme.exists():
        readme.write_text(readme.read_text().replace(f"## Version: {old}", f"## Version: {new}"))
    for path in (root / "changes").glob("*.md"):
        if path.name != "README.md":
            path.unlink()
    return new


def main() -> None:
    new = release(ROOT, datetime.date.today().isoformat())
    if new is None:
        print("nothing to release: changes/ has no fragments")
        return
    # Cargo.lock carries the workspace version too.
    subprocess.run(["cargo", "metadata", "-q", "--format-version", "1"], cwd=ROOT,
                   check=True, stdout=subprocess.DEVNULL)
    print(f"released {new}: CHANGELOG.md, Cargo.toml, Cargo.lock and README.md; commit them as 'chore: release v{new}'")


if __name__ == "__main__":
    sys.exit(main())
