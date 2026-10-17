# 0049: Changelog fragments per PR, and `make release` cuts the version

**Status:** Accepted · **Date:** 2026-10-17

## Context

Every PR used to bump the version in `Cargo.toml`, `README.md` and `CHANGELOG.md`. Three agents, each in its own git worktree, open and merge PRs in parallel, and all of them edited those same lines.

A PR whose CI was running was overtaken by another PR's release. It then had to drop its release commit, rebase, release again and rerun CI. Three of six merges on 2026-10-04 went that way (#186).

The version and the changelog are the only lines every PR touched. Nothing else in a PR has to know what the others did.

## Decision

**A PR adds a changelog fragment and never touches the version; `make release` collects the fragments into a new version whenever a release is wanted (#186, PR #187).**

- **One fragment per PR.** A PR adds `changes/<issue>.<added|changed|fixed>.md` holding its changelog bullet(s), written as in `CHANGELOG.md`. It never edits `CHANGELOG.md` or the version. `changes/README.md` and `CLAUDE.md` § Releases say so.
- **`make release` runs `tools/release.py`.** It writes a new `## [X.Y.Z] - date` section under `## [Unreleased]` in `CHANGELOG.md`, with an `### Added`, `### Changed` and `### Fixed` heading for each kind present, fragments ordered by issue number.
- **The bump follows from the fragments.** Minor when anything was added or changed, otherwise patch. The new version goes into `Cargo.toml` and `README.md`; `cargo metadata` brings `Cargo.lock` along.
- **The fragments are deleted** once their section is written. The tool commits nothing; it names the commit, `chore: release vX.Y.Z`.
- **Nothing to release changes nothing.** With no fragments the tool says so and writes no file.
- **A bad fragment stops the release before anything is written.** A name that is not `<issue>.<added|changed|fixed>.md`, or an empty fragment, is an error. So is a `Cargo.toml` without a version or a `CHANGELOG.md` without `## [Unreleased]`.

## Consequences

- PRs merged in parallel no longer collide: each adds a file of its own name, keyed by its issue.
- `main` carries unreleased fragments between releases. Its version is the last release, not the sum of what has merged since.
- A release is a separate, deliberate step, run by hand. Its commit is the only one that touches the version.
- The fragment's kind decides the bump, so a misfiled `fixed` for a feature gives a patch release.
- `tools/release.py` uses the standard library only; `tools/test_release.py` covers the section and minor bump, the patch bump, the empty run and the badly named fragment.

## Alternatives considered

- **A version bump in every PR**, as before: the collisions above, and a rerun of CI for each one overtaken.
- **A standard fragment tool**: it does the same job, but is a new dependency for about a hundred lines of Python with the repository's own conventions (the issue number as the name, the README version line).
