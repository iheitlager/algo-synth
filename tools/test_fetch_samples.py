import array
import json
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


H2 = """<?xml version="1.0" encoding="UTF-8"?>
<drumkit_info xmlns="http://www.hydrogen-music.org/drumkit"><name>K</name><instrumentList>
 <instrument><id>0</id><name>Kick</name><volume>1</volume><pan_L>1</pan_L><pan_R>1</pan_R><muteGroup>-1</muteGroup>
  <layer><filename>kick.wav</filename><min>0</min><max>1</max></layer></instrument>
 <instrument><id>1</id><name>Closed HH</name><volume>0.5</volume><pan_L>0.5</pan_L><pan_R>1</pan_R><muteGroup>-1</muteGroup>
  <layer><filename>soft.wav</filename><min>0</min><max>0.5</max></layer><layer><filename>hard.wav</filename><min>0.5</min><max>1</max></layer></instrument>
 <instrument><id>2</id><name>Open HH</name><volume>3</volume><pan_L>1</pan_L><pan_R>0</pan_R><muteGroup>2</muteGroup>
  <layer><filename>open.wav</filename><min>0</min><max>1</max></layer></instrument>
 <instrument><id>3</id><name>Empty</name><volume>1</volume></instrument>
</instrumentList></drumkit_info>"""


class Hydrogen(unittest.TestCase):
    def test_pads_follow_instruments(self):
        pads = fs.parse_h2(H2, {"Closed HH": 1})
        self.assertEqual([p["file"] for p in pads], ["kick.wav", "hard.wav", "open.wav"], "the loudest layer; no layers is skipped")
        self.assertEqual([p["pad"] for p in pads], [0, 1, 2])
        self.assertEqual((pads[0]["level"], pads[0]["pan"], pads[0]["choke"]), (0.8, 0.0, 0))
        self.assertEqual((pads[1]["level"], pads[1]["pan"], pads[1]["choke"]), (0.4, 0.5, 1))
        self.assertEqual((pads[2]["level"], pads[2]["pan"], pads[2]["choke"]), (2.0, -1.0, 3), "level caps at 2, a mute group becomes a choke group")
        self.assertTrue(all(p["oneShot"] for p in pads))

    def test_at_most_sixteen_pads(self):
        many = "".join(f"<instrument><name>i{i}</name><layer><filename>f{i}.wav</filename></layer></instrument>" for i in range(20))
        self.assertEqual(len(fs.parse_h2(f'<drumkit_info xmlns="x"><instrumentList>{many}</instrumentList></drumkit_info>')), 16)


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

    def test_eight_bit_is_unsigned_and_widened(self):
        # Unsigned: 128 is silence, 255 almost full scale, 0 full negative.
        raw = bytearray(wav(22_050, 1, 8, [(0,), (0,), (0,)]))
        i = raw.index(b"data") + 8
        raw[i : i + 3] = bytes([128, 255, 0])
        rate, ch, samples = fs.read_wav(bytes(raw))
        self.assertEqual((rate, ch, list(samples)), (22_050, 1, [0, 127 * 256, -128 * 256]))

    def test_aiff_is_read_like_wav(self):
        # 22 050 Hz as an 80-bit float: exponent 16397, mantissa 22050 << 49.
        rate80 = struct.pack(">H", 16397) + struct.pack(">Q", 22_050 << 49)
        comm = struct.pack(">HIH", 2, 2, 16) + rate80
        pcm = struct.pack(">hhhh", 100, -100, 32767, -32768)
        body = (b"AIFF" + b"COMM" + struct.pack(">I", len(comm)) + comm
                + b"SSND" + struct.pack(">I", 8 + len(pcm)) + struct.pack(">II", 0, 0) + pcm)
        rate, ch, samples = fs.read_wav(b"FORM" + struct.pack(">I", len(body)) + body)
        self.assertEqual((rate, ch, list(samples)), (22_050, 2, [100, -100, 32767, -32768]))

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
            # The checksum, not the transport, is what guards a download (one kit is only on http).
            self.assertTrue(p["url"].startswith(("https://", "http://")), p["id"])
            self.assertTrue(p["license"] and p["credit"], p["id"])
            self.assertIn(p["license"], {"CC0-1.0", "CC-BY-3.0", "CC-BY-4.0"}, p["id"])


if __name__ == "__main__":
    unittest.main()


def wav_bytes(n: int = 1600, rate: int = 16000) -> bytes:
    """A short mono 16-bit WAV, as the CMU ARCTIC files are."""
    return fs.write_mono_wav(rate, 1, array.array("h", [i % 200 - 100 for i in range(n)]), 60, None)


class Voice(unittest.TestCase):
    """A voice pack (#184): single files, one phrase per pad, with their notice."""

    def setUp(self):
        import tempfile
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.saved = (fs.OUT, fs.CACHE)
        fs.OUT, fs.CACHE = self.root / "out", self.root / "cache"
        self.pack = {
            "id": "voices", "name": "Voices", "kind": "voice", "license": "BSD-style",
            "credit": "Somebody. Modified: rewritten as mono WAV.", "notice": "Keep this notice.\n",
            "files": [{"url": f"x{i}", "sha256": f"{i:064x}", "name": f"Phrase {i}, said."} for i in range(18)],
        }

    def tearDown(self):
        fs.OUT, fs.CACHE = self.saved
        self.tmp.cleanup()

    def fake_fetch(self, pack, i, entry):
        path = self.root / f"in-{i}.wav"
        path.write_bytes(wav_bytes())
        return path

    def test_one_phrase_per_pad_named_by_what_it_says(self):
        fs.build_voice(self.pack, fetch=self.fake_fetch)
        kit = json.loads((fs.OUT / "kits" / "voices" / "kit.json").read_text())
        self.assertEqual(len(kit["pads"]), 16, "at most one per pad")
        first = kit["pads"][0]
        self.assertEqual((first["pad"], first["name"], first["oneShot"], first["choke"]), (0, "Phrase 0, said.", True, 0))
        self.assertEqual(first["sample"], "kits/voices/01-Phrase_0_said.wav")
        self.assertTrue((fs.OUT / first["sample"]).is_file())
        self.assertEqual(kit["notice"], "Keep this notice.\n")
        self.assertEqual(kit["sha256"], fs.pinned(self.pack))

    def test_the_notice_and_the_change_reach_the_credits(self):
        fs.build_voice(self.pack, fetch=self.fake_fetch)
        fs.write_manifest()
        credits = (fs.OUT / "CREDITS.txt").read_text()
        self.assertIn("Voices: BSD-style. Somebody. Modified: rewritten as mono WAV.", credits)
        self.assertIn("Keep this notice.", credits)

    def test_the_pin_covers_every_file(self):
        other = json.loads(json.dumps(self.pack))
        other["files"][5]["sha256"] = "f" * 64
        self.assertNotEqual(fs.pinned(other), fs.pinned(self.pack))
        self.assertEqual(fs.pinned({"sha256": "abc"}), "abc", "an archive keeps its own")

    def test_a_wrong_checksum_stops_the_build(self):
        src = self.root / "a.wav"
        src.write_bytes(wav_bytes())
        good = {"url": src.as_uri(), "sha256": fs.sha256(src), "name": "Fine."}
        self.assertTrue(fs.fetch_file(self.pack, 0, good).is_file())
        bad = {**good, "sha256": "0" * 64}
        with self.assertRaises(SystemExit):
            fs.fetch_file(self.pack, 1, bad)


class Ledger(unittest.TestCase):
    def test_every_voice_file_is_pinned_and_named(self):
        ledger = json.loads(fs.LEDGER.read_text())
        voices = [p for p in ledger["packs"] if p.get("kind") == "voice"]
        self.assertTrue(voices)
        for p in voices:
            self.assertIn("notice", p)
            self.assertLessEqual(len(p["files"]), fs.PADS)
            for f in p["files"]:
                self.assertRegex(f["sha256"], r"^[0-9a-f]{64}$")
                self.assertTrue(f["url"].startswith("http") and f["name"])
