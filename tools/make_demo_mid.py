#!/usr/bin/env python3
"""Write web/public/demo.mid: Pachelbel's Canon over its ground bass.

The composition is public domain and this arrangement is written here, so the
file carries no third-party licence. Four parts on channels 1-4: basso
continuo and three violins in canon two bars apart. Standard library only.

    python3 tools/make_demo_mid.py      (or: make demo-midi)
"""
from pathlib import Path
import struct

DIV = 480                 # ticks per quarter
Q, E = DIV, DIV // 2      # quarter, eighth
BAR = 4 * Q
TEMPO = 60_000_000 // 72  # 72 BPM, in µs per quarter
BARS = 24

# The ground bass, one quarter each, two bars: D A B F# G D G A.
BASS = [50, 45, 47, 42, 43, 38, 43, 45]
# Four two-bar variations over it (MIDI notes, durations in ticks).
VARIATIONS = [
    [(n, Q) for n in (78, 76, 74, 73, 71, 69, 71, 73)],   # F#' E' D' C#' B A B C#'
    [(n, Q) for n in (74, 73, 71, 69, 67, 66, 67, 64)],   # D' C#' B A G F# G E
    [(n, E) for n in (74, 78, 73, 76, 71, 74, 69, 73, 67, 71, 66, 69, 67, 71, 69, 73)],
    [(n, E) for n in (81, 78, 76, 73, 78, 74, 73, 69, 74, 71, 69, 66, 71, 67, 76, 73)],
]


def vlq(n: int) -> bytes:
    out = [n & 0x7F]
    n >>= 7
    while n:
        out.append(0x80 | (n & 0x7F))
        n >>= 7
    return bytes(reversed(out))


def track(name: str, notes: list[tuple[int, int, int, int]], meta: bytes = b"") -> bytes:
    """One MTrk chunk. notes: (start tick, duration, channel << 8 | note, velocity)."""
    events = []  # (tick, order, bytes)
    for start, dur, status_note, vel in notes:
        ch, note = status_note >> 8, status_note & 0x7F
        events.append((start, 1, bytes([0x90 | ch, note, vel])))
        events.append((start + dur, 0, bytes([0x80 | ch, note, 0])))
    events.sort(key=lambda e: (e[0], e[1]))
    body = b"\x00\xff\x03" + vlq(len(name)) + name.encode() + meta
    now = 0
    for tick, _, data in events:
        body += vlq(tick - now) + data
        now = tick
    body += b"\x00\xff\x2f\x00"
    return b"MTrk" + struct.pack(">I", len(body)) + body


def part(ch: int, start: int, seq: list[tuple[int, int]], vel: int, legato: float = 0.92):
    t, out = start, []
    for note, dur in seq:
        out.append((t, int(dur * legato), (ch << 8) | note, vel))
        t += dur
    return out


def main() -> None:
    end = (BARS - 2) * BAR
    bass = part(0, 0, [(n, Q) for n in BASS] * ((BARS - 2) // 2), 90)
    bass += part(0, end, [(38, 2 * BAR)], 90, 1.0)

    violins = []
    for i, ch in enumerate((1, 2, 3)):
        start = (2 + 2 * i) * BAR
        seq, t = [], start
        while t < end:
            for var in VARIATIONS:
                for n, d in var:
                    if t >= end:
                        break
                    seq.append((n, d))
                    t += d
        violins.append(part(ch, start, seq, 80 - 6 * i))
        # Final chord: D, F#, A across the three violins.
        violins[-1] += part(ch, end, [((74, 78, 69)[i], 2 * BAR)], 70, 1.0)

    tempo = b"\x00\xff\x51\x03" + TEMPO.to_bytes(3, "big")
    tracks = [
        track("Basso continuo", bass, tempo),
        track("Violino I", violins[0]),
        track("Violino II", violins[1]),
        track("Violino III", violins[2]),
    ]
    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(tracks), DIV)
    out = Path(__file__).resolve().parent.parent / "web" / "public" / "demo.mid"
    out.write_bytes(header + b"".join(tracks))
    print(f"wrote {out.relative_to(Path.cwd()) if out.is_relative_to(Path.cwd()) else out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
