# Changelog fragments

A PR does not bump the version or edit `CHANGELOG.md`. It adds one file here:

    changes/<issue>.<added|changed|fixed>.md

holding its changelog bullet(s), written as in `CHANGELOG.md`, for example
`changes/184.added.md`:

    - **A Voice pack:** sixteen spoken phrases for the pad sampler (#184).

`make release` collects the fragments into a new section of `CHANGELOG.md`,
bumps the version (minor when anything was added or changed, else patch) in
`Cargo.toml`, `Cargo.lock` and `README.md`, and deletes the fragments (#186).
