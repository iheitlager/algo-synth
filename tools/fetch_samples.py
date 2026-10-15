#!/usr/bin/env python3
"""Fetch the free sample packs, drum kits and voices listed in tools/samples/packs.json.

Run through `make samples`. For each pack: download (cached in .cache/samples),
verify the SHA-256, extract with bsdtar, read the SFZ, and write mono 16-bit
WAV files with the loop points in a `smpl` chunk, plus a manifest of zones,
into web/public/samples/ (gitignored; the static server ships it, ADR-0006).
The engine only reads WAV (#122) and mixes to mono (#123), so nothing is lost
by converting here. Standard library only; unpacking .7z needs 7zz/7z/7za (bsdtar reads the newer archives).

An "sf2" pack (#652) is one SoundFont 2 bank: each program it lists becomes an
instrument of its own, `gm-<name>`, from one velocity layer, its stereo pairs
mixed to mono and its zones' attenuation baked into the samples.

A "voice" pack (#184) is a list of single WAV files, each with its own URL and
checksum, built into a kit for the pad sampler: one phrase per pad. A "drums"
pack may be one too (#573): each file names its pad and choke group.
"""

from __future__ import annotations

import array
import hashlib
import json
import math
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "tools" / "samples" / "packs.json"
CACHE = ROOT / ".cache" / "samples"
OUT = ROOT / "web" / "public" / "samples"

# The engine's `LoopMode` and `ZoneField` ids (sampler.rs) are the manifest's
# vocabulary; the view sends them through `zone_set`.
LOOP_MODES = {"no_loop": 0, "one_shot": 0, "loop_continuous": 1, "loop_sustain": 2}


# --- SFZ --------------------------------------------------------------------

def parse_sfz(text: str) -> list[dict[str, str]]:
    """The regions of an SFZ file as flat dicts of opcodes.

    Opcodes inherit global -> master -> group -> region. A value runs up to the
    next ` opcode=` so `sample=` may hold spaces. Comments and `#` directives
    are dropped.
    """
    text = re.sub(r"//[^\n]*", "", text)
    text = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
    levels: dict[str, dict[str, str]] = {"global": {}, "master": {}, "group": {}}
    region: dict[str, str] | None = None
    regions: list[dict[str, str]] = []

    def flush() -> None:
        if region is not None:
            regions.append(region)

    parts = re.split(r"<(global|master|group|region|control|curve|effect)>", text)
    # parts: [prefix, header, body, header, body, ...]
    for header, body in zip(parts[1::2], parts[2::2]):
        ops = dict(
            (m.group(1), m.group(2).strip())
            for m in re.finditer(r"(\w+)=(.*?)(?=\s\w+=|$)", body.strip(), re.S)
        )
        if header == "region":
            flush()
            merged: dict[str, str] = {}
            for lv in ("global", "master", "group"):
                merged.update(levels[lv])
            merged.update(ops)
            region = merged
        elif header in levels:
            flush()
            region = None
            levels[header] = ops
            # A new group/master clears what is below it.
            if header == "global":
                levels["master"], levels["group"] = {}, {}
            elif header == "master":
                levels["group"] = {}
    flush()
    return regions


NOTES = {"c": 0, "d": 2, "e": 4, "f": 5, "g": 7, "a": 9, "b": 11}


def note_number(value: str) -> int:
    """A MIDI note from a number or a name (`C4` is 60, `Db2`, `F#3`)."""
    m = re.fullmatch(r"([A-Ga-g])([#b]?)(-?\d+)", value.strip())
    if not m:
        return int(float(value))
    semis = NOTES[m.group(1).lower()] + {"#": 1, "b": -1, "": 0}[m.group(2)]
    return semis + 12 * (int(m.group(3)) + 1)


def zone_of(region: dict[str, str], sample: str) -> dict[str, object]:
    """A manifest zone from an SFZ region; `sample` is its path in the output."""
    def num(key: str, default: int) -> int:
        try:
            if key in ("key", "lokey", "hikey", "pitch_keycenter"):
                return note_number(region[key])
            return int(float(region[key]))
        except (KeyError, ValueError):
            return default

    if "key" in region:
        lo = hi = root = num("key", 60)
    else:
        lo, hi = num("lokey", 0), num("hikey", 127)
        root = num("pitch_keycenter", lo)
    root = num("pitch_keycenter", root)
    return {
        "sample": sample,
        "keyLo": lo,
        "keyHi": hi,
        "velLo": max(1, num("lovel", 1)),
        "velHi": min(127, num("hivel", 127)),
        "root": root,
        "tune": num("tune", 0) + 100 * num("transpose", 0),
        "loop": LOOP_MODES.get(region.get("loop_mode", "no_loop"), 0),
        "seqLen": num("seq_length", 1),
        "seqPos": num("seq_position", 1),
        "release": region.get("trigger") == "release",
    }


# --- WAV --------------------------------------------------------------------

def read_aiff(data: bytes) -> tuple[int, int, array.array]:
    """(rate, channels, samples as 16-bit ints, interleaved) of a PCM 16/24 AIFF."""
    pos, comm, pcm = 12, None, b""
    while pos + 8 <= len(data):
        cid, size = data[pos : pos + 4], struct.unpack(">I", data[pos + 4 : pos + 8])[0]
        body = data[pos + 8 : pos + 8 + size]
        if cid == b"COMM":
            channels, _frames, bits = struct.unpack(">HIH", body[:8])
            exponent = struct.unpack(">H", body[8:10])[0] & 0x7FFF
            mantissa = struct.unpack(">Q", body[10:18])[0]
            comm = (channels, bits, round(mantissa * 2.0 ** (exponent - 16383 - 63)))
        elif cid == b"SSND":
            offset = struct.unpack(">I", body[:4])[0]
            pcm = body[8 + offset :]
        pos += 8 + size + (size & 1)
    if comm is None or comm[1] not in (16, 24):
        raise ValueError(f"unsupported AIFF format {comm} (PCM 16/24 only)")
    channels, bits, rate = comm
    width = bits // 8
    samples = array.array("h", (int.from_bytes(pcm[i : i + 2], "big", signed=True)
                                for i in range(0, len(pcm) - width + 1, width)))
    return rate, channels, samples


def read_wav(data: bytes) -> tuple[int, int, array.array]:
    """(rate, channels, samples as 16-bit ints, interleaved) of a PCM 8/16/24 WAV or a 16/24 AIFF."""
    if data[:4] == b"FORM" and data[8:12] == b"AIFF":
        return read_aiff(data)
    if data[:4] != b"RIFF" or data[8:12] != b"WAVE":
        raise ValueError("not a WAV file")
    pos, fmt, pcm = 12, None, b""
    while pos + 8 <= len(data):
        cid, size = data[pos : pos + 4], struct.unpack("<I", data[pos + 4 : pos + 8])[0]
        body = data[pos + 8 : pos + 8 + size]
        if cid == b"fmt ":
            tag, channels, rate, _, _, bits = struct.unpack("<HHIIHH", body[:16])
            if tag == 0xFFFE:
                tag = struct.unpack("<H", body[24:26])[0]
            fmt = (tag, channels, rate, bits)
        elif cid == b"data":
            pcm = body
        pos += 8 + size + (size & 1)
    if fmt is None or fmt[0] != 1 or fmt[3] not in (8, 16, 24):
        raise ValueError(f"unsupported WAV format {fmt} (PCM 8/16/24 only; FLAC needs ffmpeg)")
    _, channels, rate, bits = fmt
    if bits == 8:
        # Unsigned, silence at 128.
        samples = array.array("h", ((b - 128) * 256 for b in pcm))
    elif bits == 16:
        samples = array.array("h")
        samples.frombytes(pcm[: len(pcm) // 2 * 2])
        if sys.byteorder == "big":
            samples.byteswap()
    else:
        # Keep the top 16 bits of each 24-bit sample.
        samples = array.array("h", (int.from_bytes(pcm[i + 1 : i + 3], "little", signed=True)
                                    for i in range(0, len(pcm) - 2, 3)))
    return rate, channels, samples


def write_mono_wav(rate: int, channels: int, samples: array.array,
                   root: int, loop: tuple[int, int] | None) -> bytes:
    """A mono 16-bit WAV: stereo averaged, root note and loop in a `smpl` chunk."""
    if channels == 2:
        mono = array.array("h", ((samples[i] + samples[i + 1]) // 2
                                 for i in range(0, len(samples) - 1, 2)))
    elif channels == 1:
        mono = samples
    else:
        raise ValueError(f"{channels} channels")
    if sys.byteorder == "big":
        mono = array.array("h", mono)
        mono.byteswap()
    pcm = mono.tobytes()
    chunks = b"fmt " + struct.pack("<IHHIIHH", 16, 1, 1, rate, rate * 2, 2, 16)
    chunks += b"data" + struct.pack("<I", len(pcm)) + pcm
    if len(pcm) & 1:
        chunks += b"\0"
    if loop:
        smpl = struct.pack("<9I", 0, 0, 0, root, 0, 0, 0, 1, 0)
        smpl += struct.pack("<6I", 0, 0, loop[0], loop[1], 0, 0)
        chunks += b"smpl" + struct.pack("<I", len(smpl)) + smpl
    return b"RIFF" + struct.pack("<I", 4 + len(chunks)) + b"WAVE" + chunks


# --- Packs ------------------------------------------------------------------

def download(pack: dict) -> Path:
    CACHE.mkdir(parents=True, exist_ok=True)
    path = CACHE / f"{pack['id']}.{pack.get('ext', '7z')}"
    if not path.exists() or sha256(path) != pack["sha256"]:
        print(f"  downloading {pack['url']}")
        req = urllib.request.Request(pack["url"], headers={"User-Agent": "algo-synth-fetch"})
        with urllib.request.urlopen(req, timeout=120) as r, open(path, "wb") as f:
            f.write(r.read())
    got = sha256(path)
    if got != pack["sha256"]:
        path.unlink()
        raise SystemExit(f"{pack['id']}: checksum {got} is not the pinned {pack['sha256']}")
    return path


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def extract(archive: Path, dest: Path) -> None:
    """Unpack a .7z: bsdtar, else 7zz/7z/7za.

    libarchive's bsdtar lacks the Delta filter that older archives use, so a
    pack may need one of the others.
    """
    if shutil.which("bsdtar"):
        done = subprocess.run(["bsdtar", "-xf", str(archive), "-C", str(dest)],
                              capture_output=True)
        if done.returncode == 0:
            return
    for tool in ("7zz", "7z", "7za"):
        if shutil.which(tool):
            subprocess.run([tool, "x", "-y", f"-o{dest}", str(archive)],
                           check=True, capture_output=True)
            return
    raise SystemExit(
        f"cannot unpack {archive.name}: bsdtar cannot read it and no 7zz/7z/7za is installed "
        "(brew install sevenzip)"
    )


def build(pack: dict, archive: Path) -> None:
    """Convert one downloaded pack into OUT/instruments/<id>/."""
    dest = OUT / "instruments" / pack["id"]
    with tempfile.TemporaryDirectory() as tmp:
        extract(archive, Path(tmp))
        sfzs = sorted(Path(tmp).rglob("*.sfz"))
        if len(sfzs) != 1:
            raise SystemExit(f"{pack['id']}: expected one .sfz, found {len(sfzs)}")
        sfz = sfzs[0]
        regions = parse_sfz(sfz.read_text(encoding="utf-8", errors="replace"))
        if dest.exists():
            for old in dest.glob("*"):
                old.unlink()
        dest.mkdir(parents=True, exist_ok=True)

        # One output file per (sample, root, loop): a file is converted once.
        files: dict[tuple, str] = {}
        zones = []
        seen: set[tuple] = set()
        for region in regions:
            rel = region.get("sample", "").replace("\\", "/")
            src = sfz.parent / rel
            if not rel or not src.is_file():
                raise SystemExit(f"{pack['id']}: missing sample {rel!r}")
            zone = zone_of(region, "")
            # The engine plays the first zone covering a key, and the sampler is
            # mono: of regions on the same keys (a stereo pair, say) keep one.
            span = tuple(zone[k] for k in ("keyLo", "keyHi", "velLo", "velHi", "release", "seqPos"))
            if span in seen:
                continue
            seen.add(span)
            loop = None
            if zone["loop"]:
                loop = (int(region.get("loop_start", 0)), int(region.get("loop_end", 0)))
            key = (rel, zone["root"], loop)
            if key not in files:
                stem = re.sub(r"[^A-Za-z0-9._-]+", "_", Path(rel).stem)
                name = f"{stem}.wav" if not any(v == f"{stem}.wav" for v in files.values()) \
                    else f"{stem}_{len(files)}.wav"
                rate, channels, samples = read_wav(src.read_bytes())
                (dest / name).write_bytes(
                    write_mono_wav(rate, channels, samples, int(zone["root"]), loop))
                files[key] = name
            zone["sample"] = f"instruments/{pack['id']}/{files[key]}"
            zones.append(zone)
        meta = {
            "id": pack["id"], "name": pack["name"], "license": pack["license"],
            "credit": pack["credit"], "sha256": pack["sha256"], "zones": zones,
        }
        (dest / "instrument.json").write_text(json.dumps(meta, indent=1))


# --- SoundFont 2 (#652) ----------------------------------------------------------

# The generators read, by their SF2 2.04 numbers.
GEN_START, GEN_END, GEN_LOOP_START, GEN_LOOP_END = 0, 1, 2, 3
GEN_START_COARSE, GEN_END_COARSE = 4, 12
GEN_LOOP_START_COARSE, GEN_LOOP_END_COARSE = 45, 50
GEN_INSTRUMENT, GEN_KEYS, GEN_VELS = 41, 43, 44
GEN_ATTENUATION, GEN_COARSE, GEN_FINE, GEN_SAMPLE = 48, 51, 52, 53
GEN_MODES, GEN_ROOT = 54, 58
RANGES = (GEN_KEYS, GEN_VELS)
# The velocity whose layer a program keeps: the sampler takes 64 zones (#129).
LAYER_VELOCITY = 100


def riff_chunks(data: bytes, pos: int, end: int) -> dict[str, bytes]:
    """The chunks between `pos` and `end`, a LIST by its type."""
    out: dict[str, bytes] = {}
    while pos + 8 <= end:
        cid, size = data[pos : pos + 4].decode("latin-1"), struct.unpack_from("<I", data, pos + 4)[0]
        body = data[pos + 8 : pos + 8 + size]
        out[body[:4].decode("latin-1") if cid == "LIST" else cid] = body[4:] if cid == "LIST" else body
        pos += 8 + size + (size & 1)
    return out


def parse_sf2(data: bytes) -> dict:
    """A SoundFont 2 bank: its PCM (16-bit mono, all samples end to end), its
    sample headers, and its presets and instruments as lists of zones, each a
    dict of generator -> value (ranges as (lo, hi))."""
    if data[:4] != b"RIFF" or data[8:12] != b"sfbk":
        raise ValueError("not a SoundFont 2 file")
    top = riff_chunks(data, 12, len(data))
    sdta, pdta = riff_chunks(top["sdta"], 0, len(top["sdta"])), riff_chunks(top["pdta"], 0, len(top["pdta"]))
    pcm = array.array("h")
    pcm.frombytes(sdta["smpl"][: len(sdta["smpl"]) // 2 * 2])
    if sys.byteorder == "big":
        pcm.byteswap()

    def name(raw: bytes) -> str:
        return raw.split(b"\0", 1)[0].decode("latin-1").strip()

    def records(cid: str, size: int, fmt: str) -> list[tuple]:
        body = pdta[cid]
        return [struct.unpack_from(fmt, body, i) for i in range(0, len(body) - size + 1, size)]

    def zones(headers: list[tuple], bag_at: int, bags: list[tuple], gens: list[tuple]) -> list[list[dict]]:
        out = []
        for h, nxt in zip(headers, headers[1:]):
            zs = []
            for b in range(h[bag_at], nxt[bag_at]):
                z: dict = {}
                for oper, amount in gens[bags[b][0] : bags[b + 1][0]]:
                    if oper in RANGES:
                        z[oper] = (amount & 0xFF, amount >> 8)
                    else:
                        z[oper] = amount - 0x10000 if amount >= 0x8000 and oper not in (GEN_INSTRUMENT, GEN_SAMPLE) else amount
                zs.append(z)
            out.append(zs)
        return out

    phdr = records("phdr", 38, "<20sHHHIII")
    inst = records("inst", 22, "<20sH")
    pbag, ibag = records("pbag", 4, "<HH"), records("ibag", 4, "<HH")
    pgen, igen = records("pgen", 4, "<HH"), records("igen", 4, "<HH")
    shdr = records("shdr", 46, "<20sIIIIIBbHH")
    return {
        "pcm": pcm,
        "presets": [{"name": name(h[0]), "program": h[1], "bank": h[2], "zones": z}
                    for h, z in zip(phdr, zones(phdr, 3, pbag, pgen))],
        "instruments": [{"name": name(h[0]), "zones": z} for h, z in zip(inst, zones(inst, 1, ibag, igen))],
        "samples": [{"name": name(h[0]), "start": h[1], "end": h[2], "loopStart": h[3], "loopEnd": h[4],
                     "rate": h[5], "pitch": h[6], "correction": h[7], "link": h[8], "type": h[9]}
                    for h in shdr[:-1]],
    }


def split_global(zones: list[dict], last: int) -> tuple[dict, list[dict]]:
    """A zone list's global zone (the first, if it does not end in `last`) and the rest."""
    if zones and last not in zones[0]:
        return zones[0], [z for z in zones[1:] if last in z]
    return {}, [z for z in zones if last in z]


def overlap(a: tuple[int, int], b: tuple[int, int]) -> tuple[int, int] | None:
    lo, hi = max(a[0], b[0]), min(a[1], b[1])
    return (lo, hi) if lo <= hi else None


def sf2_regions(bank: dict, program: int, bank_no: int = 0) -> list[dict]:
    """The regions program `program` plays, before curation: per instrument
    zone, its sample, key and velocity ranges, root, tune in cents, loop mode
    and points (from the sample's start) and attenuation in centibels.

    Preset generators add to the instrument's; their ranges narrow it.
    """
    preset = next((p for p in bank["presets"] if p["program"] == program and p["bank"] == bank_no), None)
    if preset is None:
        raise ValueError(f"no preset {bank_no}:{program}")
    pglobal, pzones = split_global(preset["zones"], GEN_INSTRUMENT)
    out = []
    for pz in pzones:
        p = {**pglobal, **pz}
        inst = bank["instruments"][p[GEN_INSTRUMENT]]
        iglobal, izones = split_global(inst["zones"], GEN_SAMPLE)
        for iz in izones:
            g = {**iglobal, **iz}
            keys = overlap(g.get(GEN_KEYS, (0, 127)), p.get(GEN_KEYS, (0, 127)))
            vels = overlap(g.get(GEN_VELS, (0, 127)), p.get(GEN_VELS, (0, 127)))
            if keys is None or vels is None:
                continue
            smp = bank["samples"][g[GEN_SAMPLE]]
            add = lambda gen: g.get(gen, 0) + p.get(gen, 0)
            root = g.get(GEN_ROOT, -1)
            if root < 0:
                root = smp["pitch"] if smp["pitch"] <= 127 else 60
            start = smp["start"] + g.get(GEN_START, 0) + 32768 * g.get(GEN_START_COARSE, 0)
            end = smp["end"] + g.get(GEN_END, 0) + 32768 * g.get(GEN_END_COARSE, 0)
            loop_start = smp["loopStart"] + g.get(GEN_LOOP_START, 0) + 32768 * g.get(GEN_LOOP_START_COARSE, 0)
            loop_end = smp["loopEnd"] + g.get(GEN_LOOP_END, 0) + 32768 * g.get(GEN_LOOP_END_COARSE, 0)
            modes = g.get(GEN_MODES, 0) & 3
            # Whole semitones of tune move the root instead: the same pitch,
            # and the view takes a tune of at most an octave (organs sit two down).
            tune = 100 * add(GEN_COARSE) + add(GEN_FINE) + smp["correction"]
            semis = int(tune / 100)
            root, tune = root - semis, tune - 100 * semis
            if not 0 <= root <= 127:
                raise ValueError(f"{preset['name']}: root {root} is off the keyboard")
            out.append({
                "sample": g[GEN_SAMPLE], "keyLo": keys[0], "keyHi": keys[1], "velLo": vels[0], "velHi": vels[1],
                "root": root, "tune": tune,
                # 1 loops on, 3 loops until the key is let go; 0 and 2 do not loop.
                "loop": {1: 1, 3: 2}.get(modes, 0),
                "start": start, "end": end, "loopStart": loop_start - start, "loopEnd": loop_end - start,
                "attenuation": max(0, add(GEN_ATTENUATION)),
                "rate": smp["rate"], "link": smp["link"], "type": smp["type"],
            })
    return out


def curate(regions: list[dict]) -> list[dict]:
    """The regions an instrument keeps: those that play at LAYER_VELOCITY, one
    per key range (a stereo pair is one), over every velocity."""
    out, seen = [], set()
    for r in regions:
        if not r["velLo"] <= LAYER_VELOCITY <= r["velHi"]:
            continue
        span = (r["keyLo"], r["keyHi"])
        if span in seen:
            continue
        seen.add(span)
        out.append({**r, "velLo": 1, "velHi": 127})
    return out


def sf2_pcm(bank: dict, r: dict) -> array.array:
    """A region's samples, mono: a stereo pair's two sides averaged, scaled by
    its attenuation (`gain`)."""
    pcm = bank["pcm"]
    side = pcm[r["start"] : r["end"]]
    # Left (4) or right (2) of a pair: the other side starts at its own offset.
    if r["type"] in (2, 4) and r["link"] < len(bank["samples"]):
        other = bank["samples"][r["link"]]
        twin = pcm[other["start"] + (r["start"] - bank["samples"][r["sample"]]["start"]):][: len(side)]
        if len(twin) == len(side):
            side = array.array("h", ((a + b) // 2 for a, b in zip(side, twin)))
    gain = r.get("gain", 1.0)
    if gain != 1.0:
        side = array.array("h", (max(-32768, min(32767, round(v * gain))) for v in side))
    return side


def build_sf2(pack: dict, path: Path) -> None:
    """Each program of an SF2 pack into OUT/instruments/gm-<name>/ (#652)."""
    bank = parse_sf2(path.read_bytes())
    for pid, program in pack["programs"].items():
        regions = curate(sf2_regions(bank, program))
        if len(regions) > 64:
            raise SystemExit(f"{pid}: {len(regions)} zones; the sampler takes 64")
        # The loudest zone plays at full scale; the others keep their balance.
        least = min((r["attenuation"] for r in regions), default=0)
        dest = OUT / "instruments" / pid
        if dest.exists():
            for old in dest.glob("*"):
                old.unlink()
        dest.mkdir(parents=True, exist_ok=True)
        zones = []
        for i, r in enumerate(regions):
            r["gain"] = 10 ** (-(r["attenuation"] - least) / 200)
            name = f"{pid}-{i + 1:02d}.wav"
            loop = (r["loopStart"], r["loopEnd"]) if r["loop"] else None
            (dest / name).write_bytes(write_mono_wav(r["rate"], 1, sf2_pcm(bank, r), r["root"], loop))
            zones.append({
                "sample": f"instruments/{pid}/{name}", "keyLo": r["keyLo"], "keyHi": r["keyHi"],
                "velLo": 1, "velHi": 127, "root": r["root"], "tune": r["tune"], "loop": r["loop"],
                "seqLen": 1, "seqPos": 1, "release": False,
            })
        meta = {"id": pid, "name": f"GM {pid.removeprefix('gm-').replace('-', ' ').title()}",
                "license": pack["license"], "credit": pack["credit"], "notice": pack["notice"],
                "sha256": pinned(pack), "zones": zones}
        (dest / "instrument.json").write_text(json.dumps(meta, indent=1))
        print(f"  {pid}: {len(zones)} zones")


# --- Drum kits (Hydrogen .h2drumkit) -------------------------------------------

PADS = 16  # padsampler.rs: pad i plays note 36 + i, which is where Hydrogen puts instrument i
FIRST_NOTE = 36


def parse_h2(xml: str, choke_by_name: dict[str, int] | None = None) -> list[dict[str, object]]:
    """The pads of a Hydrogen drumkit.xml, in instrument order: pad i is instrument i.

    Of an instrument's velocity layers the loudest is kept. Level is the instrument's
    volume (0.8 being the pad default), pan the difference of its right and left, and
    the choke group its mute group, or `choke_by_name` (the kit author's hats
    do not declare one, though a closed hat should cut an open one).
    """
    root = ET.fromstring(xml)

    def text(node: ET.Element, tag: str, default: str = "") -> str:
        found = node.find(f"{{*}}{tag}")
        return (found.text or default).strip() if found is not None else default

    pads: list[dict[str, object]] = []
    for node in root.findall(".//{*}instrument"):
        if len(pads) >= PADS:
            break
        layers = node.findall("{*}layer")
        if not layers:
            continue
        top = max(layers, key=lambda l: float(text(l, "max", "1")))
        name = text(node, "name")
        mute = int(text(node, "muteGroup", "-1"))
        group = (choke_by_name or {}).get(name, mute + 1 if mute >= 0 else 0)
        pad_l, pad_r = float(text(node, "pan_L", "1")), float(text(node, "pan_R", "1"))
        pads.append({
            "pad": len(pads),
            "name": name,
            "file": text(top, "filename"),
            "tune": 0,
            "level": round(min(2.0, 0.8 * float(text(node, "volume", "1"))), 2),
            "pan": round(max(-1.0, min(1.0, pad_r - pad_l)), 2),
            "decay": 0,
            "choke": max(0, min(8, group)),
            "velLevel": 1.0,
            "velStart": 0.0,
            "oneShot": True,
        })
    return pads


def build_kit(pack: dict, archive: Path) -> None:
    """Convert one downloaded Hydrogen kit into OUT/kits/<id>/."""
    dest = OUT / "kits" / pack["id"]
    with tempfile.TemporaryDirectory() as tmp:
        extract(archive, Path(tmp))
        xmls = sorted(Path(tmp).rglob("drumkit.xml"))
        if len(xmls) != 1:
            raise SystemExit(f"{pack['id']}: expected one drumkit.xml, found {len(xmls)}")
        pads = parse_h2(xmls[0].read_text(encoding="utf-8", errors="replace"), pack.get("choke"))
        if dest.exists():
            for old in dest.glob("*"):
                old.unlink()
        dest.mkdir(parents=True, exist_ok=True)
        out = []
        for pad in pads:
            src = xmls[0].parent / str(pad.pop("file"))
            if not src.is_file():
                raise SystemExit(f"{pack['id']}: missing sample {src.name!r}")
            rate, channels, samples = read_wav(src.read_bytes())
            stem = re.sub(r"[^A-Za-z0-9._-]+", "_", str(pad.pop("name")))
            name = f"{pad['pad'] + 1:02d}-{stem}.wav"
            (dest / name).write_bytes(write_mono_wav(rate, channels, samples, 60, None))
            out.append({**pad, "sample": f"kits/{pack['id']}/{name}"})
        meta = {"id": pack["id"], "name": pack["name"], "license": pack["license"],
                "credit": pack["credit"], "sha256": pack["sha256"], "pads": out}
        (dest / "kit.json").write_text(json.dumps(meta, indent=1))


# --- Voices (single files, one phrase per pad) -----------------------------------------

def pinned(pack: dict) -> str:
    """What a built pack is checked against: its archive's checksum, or for a
    pack of single files one checksum over all of theirs and its balance."""
    if "programs" in pack:
        # The programs kept are part of what is built (#652).
        joined = pack["sha256"] + json.dumps(pack["programs"], sort_keys=True)
        return hashlib.sha256(joined.encode()).hexdigest()
    if "files" in pack:
        joined = "\n".join(f["sha256"] for f in pack["files"])
        # Balancing changes the kit, so a kit built before it is rebuilt (#585).
        if pack.get("balance"):
            joined += "\nbalance"
        return hashlib.sha256(joined.encode()).hexdigest()
    return pack["sha256"]


def fetch_file(pack: dict, i: int, entry: dict) -> Path:
    """Download one file of a pack (cached), verified against its own checksum."""
    return download({"id": f"{pack['id']}-{i + 1:02d}", "ext": "wav", **entry})


# A balanced pack (#585) brings each pad to its voice's loudness: the loudest 50 ms of
# the hit, RMS in dBFS, aimed at the middle of the drum machines' own. By pad (General
# MIDI note 36 + pad): kick, rim, snare, clap, hats, toms, crash and ride; -10 elsewhere.
LOUDNESS_TARGETS = {0: -7.0, 1: -10.0, 2: -9.5, 3: -10.0, 6: -12.5, 10: -12.5,
                    9: -8.0, 11: -8.0, 14: -8.0, 13: -11.0, 15: -11.0}
PAD_LEVEL = 0.8  # a pad at its target keeps the kits' usual level


def loudness(rate: int, channels: int, samples: array.array, window: float = 0.05) -> float:
    """The loudest `window` seconds of a hit, RMS in dBFS, its channels averaged."""
    mono = [sum(samples[i : i + channels]) / channels for i in range(0, len(samples) - channels + 1, channels)]
    w = max(1, min(len(mono), int(rate * window)))
    sq = [v * v for v in mono]
    run = best = sum(sq[:w])
    for i in range(w, len(sq)):
        run += sq[i] - sq[i - w]
        best = max(best, run)
    return 10 * math.log10(best / w / 32768**2 + 1e-12)


def balanced_level(pad: int, rate: int, channels: int, samples: array.array) -> float:
    """The pad level that brings a hit to its voice's target, within the engine's 0.2-2."""
    gain = 10 ** ((LOUDNESS_TARGETS.get(pad, -10.0) - loudness(rate, channels, samples)) / 20)
    return round(min(2.0, max(0.2, PAD_LEVEL * gain)), 2)


def build_voice(pack: dict, fetch=fetch_file) -> None:
    """Convert a pack of single WAV files into OUT/kits/<id>/: pad i plays file i
    (or the file's `pad`) as a one-shot, named after what it says, at its
    balanced level if the pack asks (#585). The pack's notice goes with it."""
    dest = OUT / "kits" / pack["id"]
    if dest.exists():
        for old in dest.glob("*"):
            old.unlink()
    dest.mkdir(parents=True, exist_ok=True)
    pads = []
    for i, entry in enumerate(pack["files"][:PADS]):
        rate, channels, samples = read_wav(fetch(pack, i, entry).read_bytes())
        pad = entry.get("pad", i)
        stem = re.sub(r"[^A-Za-z0-9_-]+", "_", entry["name"]).strip("_")[:32]
        name = f"{pad + 1:02d}-{stem}.wav"
        (dest / name).write_bytes(write_mono_wav(rate, channels, samples, 60, None))
        level = balanced_level(pad, rate, channels, samples) if pack.get("balance") else PAD_LEVEL
        pads.append({
            "pad": pad, "name": entry["name"], "tune": 0, "level": level, "pan": 0.0,
            "decay": 0, "choke": entry.get("choke", 0), "velLevel": 1.0, "velStart": 0.0, "oneShot": True,
            "sample": f"kits/{pack['id']}/{name}",
        })
    meta = {"id": pack["id"], "name": pack["name"], "license": pack["license"],
            "credit": pack["credit"], "sha256": pinned(pack), "pads": pads}
    if "notice" in pack:
        meta["notice"] = pack["notice"]
    (dest / "kit.json").write_text(json.dumps(meta, indent=1))


def write_manifest() -> None:
    """The browser's index: every built instrument and kit, and the credits."""
    base = OUT / "instruments"
    metas = [json.loads(p.read_text()) for p in sorted(base.glob("*/instrument.json"))]
    kits = [json.loads(p.read_text()) for p in sorted((OUT / "kits").glob("*/kit.json"))]
    (OUT / "manifest.json").write_text(json.dumps({"version": 1, "instruments": metas, "kits": kits}))
    credits = ["Sample packs fetched by `make samples`; each is used under its own license.\n"]
    notices = set()
    for m in metas + kits:
        credits.append(f"{m['name']}: {m['license']}. {m['credit']}\n")
        # The programs of one SoundFont share its notice (#652): once is enough.
        if "notice" in m and m["notice"] not in notices:
            notices.add(m["notice"])
            credits.append(m["notice"].rstrip() + "\n")
    (OUT / "CREDITS.txt").write_text("\n".join(credits))


def main() -> None:
    ledger = json.loads(LEDGER.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    for pack in ledger["packs"]:
        kind = pack.get("kind", "instrument")
        if kind not in ("instrument", "drums", "voice", "sf2"):
            continue
        folder, marker_name = ("instruments", "instrument.json") if kind in ("instrument", "sf2") \
            else ("kits", "kit.json")
        built = pack.get("programs", [pack["id"]])
        markers = [OUT / folder / i / marker_name for i in built]
        if all(m.exists() and json.loads(m.read_text()).get("sha256") == pinned(pack) for m in markers):
            print(f"{pack['id']}: up to date")
            continue
        print(f"{pack['id']}")
        if kind == "sf2":
            build_sf2(pack, download(pack))
        elif "files" in pack:
            build_voice(pack)
        else:
            (build if kind == "instrument" else build_kit)(pack, download(pack))
    write_manifest()
    print(f"manifest: {OUT / 'manifest.json'}")


if __name__ == "__main__":
    main()
