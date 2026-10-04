import array
import struct
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import fetch_samples as fs

SFZ = """
//+ A comment line
<global>
 ampeg_release=2 // trailing comment
 loop_mode=loop_continuous
<group>
 lovel=1 hivel=63
<region>
 lokey=33 hikey=38
 pitch_keycenter=36
 loop_start=100 loop_end=900
 sample=samples/C 2.wav
<region>
 key=60 hivel=127 lovel=64
 loop_mode=no_loop
 seq_length=2 seq_position=2
 sample=samples/C4.wav
<group>
 trigger=release
<region>
 lokey=0 hikey=127 sample=samples/rel.wav
"""


class Sfz(unittest.TestCase):
    def test_regions_inherit_and_override(self):
        r = fs.parse_sfz(SFZ)
        self.assertEqual(len(r), 3)
        self.assertEqual(r[0]["sample"], "samples/C 2.wav")
        self.assertEqual(r[0]["loop_mode"], "loop_continuous")
        self.assertEqual(r[0]["hivel"], "63")
        self.assertEqual(r[1]["loop_mode"], "no_loop")
        self.assertEqual(r[1]["lovel"], "64", "a region overrides its group")
        self.assertNotIn("hivel", {k for k in r[2] if r[2][k] == "63"}, "a new group clears the old")
        self.assertEqual(r[2]["trigger"], "release")

    def test_zone_fields(self):
        r = fs.parse_sfz(SFZ)
        z = fs.zone_of(r[0], "x.wav")
        self.assertEqual((z["keyLo"], z["keyHi"], z["root"], z["loop"]), (33, 38, 36, 1))
        self.assertEqual((z["velLo"], z["velHi"]), (1, 63))
        z = fs.zone_of(r[1], "y.wav")
        self.assertEqual((z["keyLo"], z["keyHi"], z["root"], z["loop"]), (60, 60, 60, 0))
        self.assertEqual((z["seqLen"], z["seqPos"]), (2, 2))
        self.assertTrue(fs.zone_of(r[2], "z.wav")["release"])


def wav(rate, channels, bits, frames):
    width = bits // 8
    pcm = b"".join(
        int(v).to_bytes(width, "little", signed=True) for f in frames for v in f
    )
    fmt = struct.pack("<HHIIHH", 1, channels, rate, rate * channels * width, channels * width, bits)
    body = b"WAVEfmt " + struct.pack("<I", 16) + fmt + b"data" + struct.pack("<I", len(pcm)) + pcm
    return b"RIFF" + struct.pack("<I", len(body)) + body


class Notes(unittest.TestCase):
    def test_names_and_numbers(self):
        self.assertEqual([fs.note_number(n) for n in ("C4", "c#4", "Db2", "A0", "G1", "36", "-1")],
                         [60, 61, 37, 21, 31, 36, -1])
        z = fs.zone_of({"lokey": "G1", "hikey": "Db2", "pitch_keycenter": "C2"}, "x")
        self.assertEqual((z["keyLo"], z["keyHi"], z["root"]), (31, 37, 36))


class Wav(unittest.TestCase):
    def test_stereo_24_bit_becomes_mono_16_with_a_loop(self):
        # Left +0.5, right -0.5 cancel; second frame both +0.25.
        src = wav(44_100, 2, 24, [(0x400000, -0x400000), (0x200000, 0x200000)])
        rate, ch, samples = fs.read_wav(src)
        self.assertEqual((rate, ch), (44_100, 2))
        self.assertEqual(list(samples), [16384, -16384, 8192, 8192])
        out = fs.write_mono_wav(rate, ch, samples, 60, (0, 1))
        rate2, ch2, mono = fs.read_wav(out)
        self.assertEqual((rate2, ch2), (44_100, 1))
        self.assertEqual(list(mono), [0, 8192])
        i = out.index(b"smpl")
        self.assertEqual(struct.unpack("<I", out[i + 8 + 12 : i + 8 + 16])[0], 60, "root")
        self.assertEqual(struct.unpack("<II", out[i + 8 + 44 : i + 8 + 52]), (0, 1))

    def test_unsupported_formats_are_refused(self):
        with self.assertRaises(ValueError):
            fs.read_wav(b"not a wav at all")
        with self.assertRaises(ValueError):
            fs.read_wav(wav(48_000, 1, 16, [(1,)]).replace(b"\x01\x00\x01\x00", b"\x03\x00\x01\x00", 1))


class Ledger(unittest.TestCase):
    def test_every_pack_is_pinned_and_licensed(self):
        import json
        packs = json.loads(fs.LEDGER.read_text())["packs"]
        ids = [p["id"] for p in packs]
        self.assertEqual(len(ids), len(set(ids)))
        for p in packs:
            self.assertRegex(p["sha256"], r"^[0-9a-f]{64}$", p["id"])
            self.assertTrue(p["url"].startswith("https://"), p["id"])
            self.assertTrue(p["license"] and p["credit"], p["id"])
            self.assertIn(p["license"], {"CC0-1.0", "CC-BY-3.0", "CC-BY-4.0"}, p["id"])


if __name__ == "__main__":
    unittest.main()
