# 0044: Sample packs fetched at build time, pinned and licensed

**Status:** Accepted · **Date:** 2026-10-17 · **Extends:** 0006, 0013

## Context

The samplers (epic #126) need recorded sounds: instruments for the multisampler (#129), kits for the pad sampler (#124, #130), spoken phrases for the vocoder's modulator (#184). Free packs exist, but they are large, come as 7z archives, Hydrogen kits, SoundFonts and single files, and each has its own licence. Some forbid what a sampler does with them; some have none.

ADR-0006 keeps the server static: it serves files, it takes no uploads. ADR-0013 makes the engine read WAV only (#122), mixed to mono (#123), so whatever a pack holds must arrive as WAV.

#129 planned a shell script, `tools/fetch-samples.sh`, reading `tools/samples/instruments.toml`. What was built is `tools/fetch_samples.py` reading `tools/samples/packs.json`, one ledger for every kind of pack.

## Decision

**`make samples` downloads the packs listed in `tools/samples/packs.json`, verifies each against a pinned SHA-256, converts them to mono WAV and writes them, with a manifest and credits, into the gitignored `web/public/samples/`, which the static server ships as it is.**

- **One ledger.** Each pack in `packs.json` has an `id`, a `kind` (`instrument`, `drums`, `voice`, `sf2`), a URL (or a list of `files`, each with its own URL and checksum), its `sha256`, its `license` and a `credit`. A pack goes in only with its licence recorded (#130's provenance rule).
- **Pinned, or the build stops.** A download is checked against its pinned SHA-256; a mismatch deletes the file and stops `make samples` with both hashes. Downloads are cached in `.cache/samples`, so a second run fetches nothing, and a pack whose built marker carries the pinned hash is "up to date".
- **Converted once, here.** SFZ instruments, Hydrogen `.h2drumkit` kits, single WAV files and SoundFont 2 banks become mono 16-bit WAV with the root note and loop points in a `smpl` chunk. The reader takes PCM 8/16/24-bit WAV and 16/24-bit AIFF; there is no FLAC or Ogg decoder, and such a source is refused. Standard library only; a `.7z` needs `bsdtar` or `7zz`.
- **SoundFonts become `gm-…` instruments** (#652): each listed program of the FluidR3 GM bank is an instrument of its own, one velocity layer, stereo pairs mixed down, attenuation baked in.
- **A manifest the view lays out.** Each built pack writes `instrument.json` or `kit.json`; `manifest.json` indexes them, and `fetchPacks` in `web/src/audio/engine.ts` reads it once per page load. The view forwards bytes and zones (ADR-0001, ADR-0013); the mapping is data.
- **The manifest and credits are generated, never tracked** (#672). `fetch_samples.py --manifest` indexes what is built, fetching nothing; a Makefile rule runs it when `manifest.json` is missing, before `make dev`, `make web` and `make image`.
- **Licences travel with the sounds.** `CREDITS.txt` lists every built pack's licence and credit, and each notice once. CC0 is the default. A pack under another licence carries its `notice` verbatim (the BSD-style CMU ARCTIC notice, #184, with the conversion marked; FluidR3's MIT notice), or is marked `redistribute: false`.
- **Some kits are for this machine only.** The tidal-drum-machines kits (`roland-tr808`, `linndrum`, …), Dirt-Samples' `jazz` and clean-breaks' `breaks` (#573, #661) have no licence. They are built on request like the others, never committed, and their ledger entry says not to publish them.
- **A few CC0 instruments are committed.** The five FreePats instruments are force-added under the ignored folder, so a fresh clone plays without a download; everything else exists only where `make samples` ran.

## Consequences

- A clean checkout builds and runs: it has the five committed instruments and writes their manifest. `make samples` adds the rest (19 ledger entries, 12 GM programs among them).
- The Containerfile copies `web/` into the image, so the image ships whatever `web/public/samples/` holds. The image is built and served locally (ADR-0030) and never pushed; the `redistribute: false` kits would be in an image that was.
- A test (`tools/test_fetch_samples.py`) requires every pack to be pinned and licensed, and any licence other than CC0 or CC-BY to bring a notice or `redistribute: false`. The builders are tested offline with fake files.
- Each checkout and worktree has its own `web/public/samples/` and manifest; a missing pack says "run make samples".
- Samples from the web (ADR-0013's amendment, #728) are not this: the view fetches them when a song names them, and `make samples` never does.
- Not built from #129's starter set: VCSL, Karoryfer and the Salamander piano. No CC-BY pack is in the ledger, so CC-BY "opt-in with credit" stays a rule without a case.

## Alternatives considered

- **Commit the packs to git**: hundreds of MiB of binaries, and some licences forbid it or are absent.
- **Commit the generated manifest** (#672): any checkout, stash or rebase reset it to the committed few packs, and every pack built on disk vanished from the app.
- **An upload endpoint** for packs: breaks ADR-0006's static server; the sample browser already takes a user's own WAV in the page.
- **Decode in the browser or the engine** (FLAC, Ogg, SF2): JavaScript beyond messaging, or a decoder in the engine (ADR-0001, ADR-0013); converting once at build time costs nothing at play.
- **Philharmonia**: its terms forbid shipping it as a sampler. **GPL and CC-BY-SA Hydrogen kits**: copyleft. **Iowa MIS and 99Sounds**: unclear redistribution terms. **Roland 808/909 samples** as shipped packs: no verifiable provenance (#130); they exist only as local, unlicensed kits.
