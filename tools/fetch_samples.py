#!/usr/bin/env python3
"""Fetch the free sample packs and drum kits listed in tools/samples/packs.json.

Run through `make samples`. For each pack: download (cached in .cache/samples),
verify the SHA-256, extract with bsdtar, read the SFZ, and write mono 16-bit
WAV files with the loop points in a `smpl` chunk, plus a manifest of zones,
into web/public/samples/ (gitignored; the static server ships it, ADR-0006).
The engine only reads WAV (#122) and mixes to mono (#123), so nothing is lost
by converting here. Standard library only; unpacking .7z needs 7zz/7z/7za (bsdtar reads the newer archives).
"""

from __future__ import annotations

import array
import hashlib
import json
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


def write_manifest() -> None:
    """The browser's index: every built instrument and kit, and the credits."""
    base = OUT / "instruments"
    metas = [json.loads(p.read_text()) for p in sorted(base.glob("*/instrument.json"))]
    kits = [json.loads(p.read_text()) for p in sorted((OUT / "kits").glob("*/kit.json"))]
    (OUT / "manifest.json").write_text(json.dumps({"version": 1, "instruments": metas, "kits": kits}))
    credits = ["Sample packs fetched by `make samples`; each is used under its own license.\n"]
    for m in metas + kits:
        credits.append(f"{m['name']}: {m['license']}. {m['credit']}\n")
    (OUT / "CREDITS.txt").write_text("\n".join(credits))


def main() -> None:
    ledger = json.loads(LEDGER.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    for pack in ledger["packs"]:
        kind = pack.get("kind", "instrument")
        if kind not in ("instrument", "drums"):
            continue
        folder, marker_name = ("instruments", "instrument.json") if kind == "instrument" else ("kits", "kit.json")
        marker = OUT / folder / pack["id"] / marker_name
        if marker.exists() and json.loads(marker.read_text()).get("sha256") == pack["sha256"]:
            print(f"{pack['id']}: up to date")
            continue
        print(f"{pack['id']}")
        (build if kind == "instrument" else build_kit)(pack, download(pack))
    write_manifest()
    print(f"manifest: {OUT / 'manifest.json'}")


if __name__ == "__main__":
    main()
