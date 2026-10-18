import array
import json
import math
import struct
import subprocess
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


def sf2_bytes(names: tuple[str, str] = ("L", "R"), links: tuple[int, int] = (1, 0)) -> bytes:
    """A tiny SoundFont 2: samples 0/1 a stereo pair, 2 mono; one instrument and
    preset 0:5 over it (#652). `names` and `links` are the pair's."""
    def chunk(cid: bytes, body: bytes) -> bytes:
        return cid + struct.pack("<I", len(body)) + body + (b"\0" if len(body) & 1 else b"")

    def lst(kind: bytes, *chunks: bytes) -> bytes:
        return chunk(b"LIST", kind + b"".join(chunks))

    pcm = array.array("h", [100] * 40 + [300] * 40 + [1000] * 40 + [0] * 46)
    name = lambda n: n.encode().ljust(20, b"\0")
    #               name          start end  loop  rate  pitch corr link type
    shdr = b"".join([
        struct.pack("<20sIIIIIBbHH", name(names[0]), 0, 40, 4, 36, 22050, 60, 0, links[0], 4),
        struct.pack("<20sIIIIIBbHH", name(names[1]), 40, 80, 44, 76, 22050, 60, 0, links[1], 2),
        struct.pack("<20sIIIIIBbHH", name("M"), 80, 120, 90, 110, 44100, 72, -5, 0, 1),
        struct.pack("<20sIIIIIBbHH", name("EOS"), 0, 0, 0, 0, 0, 0, 0, 0, 0),
    ])
    gen = lambda op, amount: struct.pack("<HH", op, amount & 0xFFFF)
    rng = lambda op, lo, hi: struct.pack("<HBB", op, lo, hi)
    # Instrument zones: a global one (a loop), then low keys on the pair, two
    # velocity layers of the mono sample up top, the loud one 6 dB down.
    igen = [
        gen(54, 1),
        rng(43, 0, 59), gen(58, 48), gen(51, -13), gen(53, 0),
        rng(43, 60, 127), rng(44, 0, 63), gen(53, 2),
        rng(43, 60, 127), rng(44, 64, 127), gen(48, 60), gen(53, 2),
        gen(0, 0),
    ]
    ibag = [(0, 0), (1, 0), (5, 0), (8, 0), (12, 0)]
    pgen = [gen(52, 10), rng(43, 0, 100), gen(41, 0), gen(0, 0)]
    pbag = [(0, 0), (1, 0), (3, 0)]
    pdta = lst(
        b"pdta",
        chunk(b"phdr", struct.pack("<20sHHHIII", name("Five"), 5, 0, 0, 0, 0, 0)
              + struct.pack("<20sHHHIII", name("EOP"), 0, 0, 2, 0, 0, 0)),
        chunk(b"pbag", b"".join(struct.pack("<HH", *b) for b in pbag)),
        chunk(b"pmod", b"\0" * 10),
        chunk(b"pgen", b"".join(pgen)),
        chunk(b"inst", struct.pack("<20sH", name("Inst"), 0) + struct.pack("<20sH", name("EOI"), 4)),
        chunk(b"ibag", b"".join(struct.pack("<HH", *b) for b in ibag)),
        chunk(b"imod", b"\0" * 10),
        chunk(b"igen", b"".join(igen)),
        chunk(b"shdr", shdr),
    )
    body = b"sfbk" + lst(b"INFO", chunk(b"ifil", struct.pack("<HH", 2, 1))) + lst(b"sdta", chunk(b"smpl", pcm.tobytes())) + pdta
    return b"RIFF" + struct.pack("<I", len(body)) + body


class Sf2(unittest.TestCase):
    def setUp(self):
        self.bank = fs.parse_sf2(sf2_bytes())

    def test_a_bank_reads_its_presets_instruments_and_samples(self):
        self.assertEqual([(p["name"], p["program"], p["bank"]) for p in self.bank["presets"]], [("Five", 5, 0)])
        self.assertEqual([s["name"] for s in self.bank["samples"]], ["L", "R", "M"])
        self.assertEqual(len(self.bank["pcm"]), 166)
        with self.assertRaises(ValueError):
            fs.parse_sf2(b"RIFF\0\0\0\0WAVE")
        with self.assertRaises(ValueError):
            fs.sf2_regions(self.bank, 6)

    def test_regions_combine_the_preset_and_instrument_zones(self):
        r = fs.sf2_regions(self.bank, 5)
        self.assertEqual(len(r), 3)
        low, soft, loud = r
        # The preset's key range narrows the instrument's; its fine tune adds.
        self.assertEqual((low["keyLo"], low["keyHi"], soft["keyHi"]), (0, 59, 100))
        # -13 semitones and 10 cents: the root moves 12 up, -110 + 10 cents is left.
        self.assertEqual((low["root"], low["tune"]), (60, -90))
        # The mono sample's own root and pitch correction, its loop from its start.
        self.assertEqual((soft["root"], soft["tune"], soft["loop"]), (72, 5, 1))
        self.assertEqual((soft["loopStart"], soft["loopEnd"], soft["rate"]), (10, 30, 44100))
        self.assertEqual((soft["attenuation"], loud["attenuation"]), (0, 60))

    def test_curation_keeps_one_layer_over_every_velocity(self):
        c = fs.curate(fs.sf2_regions(self.bank, 5))
        self.assertEqual([(z["keyLo"], z["velLo"], z["velHi"], z["attenuation"]) for z in c],
                         [(0, 1, 127, 0), (60, 1, 127, 60)])

    def test_a_stereo_pair_is_mixed_and_attenuation_baked_in(self):
        low, _, loud = fs.sf2_regions(self.bank, 5)
        self.assertEqual(set(fs.sf2_pcm(self.bank, low)), {200})
        self.assertEqual(set(fs.sf2_pcm(self.bank, {**loud, "gain": 0.5})), {500})

    def test_a_pair_linked_to_sample_0_finds_its_twin_by_name(self):
        # #670: FluidR3 links every pair to sample 0; the left side here is
        # sample 0 itself, so a link alone would mix it with itself.
        bank = fs.parse_sf2(sf2_bytes(("Pipe(L)", "Pipe(R)"), (0, 0)))
        low = fs.sf2_regions(bank, 5)[0]
        self.assertEqual(set(fs.sf2_pcm(bank, low)), {200})
        self.assertEqual(fs.twin_of(bank, 1)["name"], "Pipe(L)")

    def test_a_side_with_no_twin_stays_as_it_is(self):
        bank = fs.parse_sf2(sf2_bytes(("A", "B"), (2, 2)))
        low = fs.sf2_regions(bank, 5)[0]
        self.assertIsNone(fs.twin_of(bank, 0))
        self.assertEqual(set(fs.sf2_pcm(bank, low)), {100})

    def test_the_pinned_checksum_covers_the_programs(self):
        pack = {"sha256": "a" * 64, "programs": {"gm-a": 1}}
        self.assertNotEqual(fs.pinned(pack), fs.pinned({**pack, "programs": {"gm-a": 2}}))


class Ledger(unittest.TestCase):
    def test_every_pack_is_pinned_and_licensed(self):
        import json
        packs = json.loads(fs.LEDGER.read_text())["packs"]
        ids = [p["id"] for p in packs]
        self.assertEqual(len(ids), len(set(ids)))
        for p in packs:
            # A pack of single files pins each one (#184).
            for f in p.get("files", [p]):
                self.assertRegex(f["sha256"], r"^[0-9a-f]{64}$", p["id"])
                # The checksum, not the transport, is what guards a download (one kit is only on http).
                self.assertTrue(f["url"].startswith(("https://", "http://")), p["id"])
            self.assertTrue(p["license"] and p["credit"], p["id"])
            # Any other license keeps its notice (CMU), or the pack is for this machine only (#573).
            if p["license"] not in {"CC0-1.0", "CC-BY-3.0", "CC-BY-4.0"}:
                self.assertTrue("notice" in p or p.get("redistribute") is False, p["id"])
            # A SoundFont's programs are instruments of their own (#652).
            for pid in p.get("programs", {}):
                self.assertRegex(pid, r"^gm-[a-z0-9-]+$", p["id"])
                self.assertNotIn(pid, ids, p["id"])


def wav_bytes(n: int = 1600, rate: int = 16000) -> bytes:
    """A short mono 16-bit WAV, as the CMU ARCTIC files are."""
    return fs.write_mono_wav(rate, 1, array.array("h", [i % 200 - 100 for i in range(n)]), 60, None)


class Loudness(unittest.TestCase):
    """The balance of a drum-machine kit (#585)."""

    def test_the_loudest_window_of_known_signals(self):
        square = array.array("h", [32767, -32767] * 4000)
        self.assertAlmostEqual(fs.loudness(8000, 1, square), 0.0, places=1)
        sine = array.array("h", (round(32767 * math.sin(i * 2 * math.pi / 16)) for i in range(8000)))
        self.assertAlmostEqual(fs.loudness(8000, 1, sine), -3.0, places=1)
        # A short burst in silence: the window finds it, a mean over the file would not.
        burst = array.array("h", [0] * 4000 + [32767, -32767] * 200 + [0] * 4000)
        self.assertAlmostEqual(fs.loudness(8000, 1, burst), 0.0, places=1)
        stereo = array.array("h", [32767, 32767, -32767, -32767] * 2000)
        self.assertAlmostEqual(fs.loudness(8000, 2, stereo), 0.0, places=1)

    def test_a_level_reaches_the_target_within_the_engines_range(self):
        at = lambda db: array.array("h", [round(32767 * 10 ** (db / 20)), -round(32767 * 10 ** (db / 20))] * 4000)
        self.assertEqual(fs.balanced_level(0, 8000, 1, at(-7.0)), fs.PAD_LEVEL, "a kick at its target")
        self.assertAlmostEqual(fs.balanced_level(2, 8000, 1, at(-15.5)), round(fs.PAD_LEVEL * 2, 2), places=2)
        self.assertEqual(fs.balanced_level(6, 8000, 1, at(-40.0)), 2.0)
        self.assertEqual(fs.balanced_level(5, 8000, 1, at(0.0)), 0.25, "a spare pad aims at -10")
        self.assertEqual(fs.balanced_level(0, 8000, 1, array.array("h", [1] * 400)), 2.0)


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

    def test_a_file_may_name_its_pad_and_choke_group(self):
        self.pack["files"] = [{**self.pack["files"][0], "pad": 6, "choke": 1}, {**self.pack["files"][1], "pad": 10, "choke": 1}]
        fs.build_voice(self.pack, fetch=self.fake_fetch)
        kit = json.loads((fs.OUT / "kits" / "voices" / "kit.json").read_text())
        self.assertEqual([(p["pad"], p["choke"]) for p in kit["pads"]], [(6, 1), (10, 1)])
        self.assertEqual(kit["pads"][1]["sample"], "kits/voices/11-Phrase_1_said.wav")

    def test_a_balanced_pack_sets_each_pad_to_its_voices_loudness(self):
        loud = fs.write_mono_wav(16000, 1, array.array("h", [16000, -16000] * 1600), 60, None)
        def fetch(pack, i, entry):
            path = self.root / f"in-{i}.wav"
            path.write_bytes(loud if i == 0 else wav_bytes())
            return path
        self.pack["files"] = [{**self.pack["files"][0], "pad": 0}, {**self.pack["files"][1], "pad": 6}]
        fs.build_voice({**self.pack, "balance": True}, fetch=fetch)
        levels = [p["level"] for p in json.loads((fs.OUT / "kits" / "voices" / "kit.json").read_text())["pads"]]
        self.assertLess(levels[0], fs.PAD_LEVEL, "a -6 dB kick comes down to -7")
        self.assertEqual(levels[1], 2.0, "a hit near -50 dB comes up as far as the engine goes")
        fs.build_voice(self.pack, fetch=fetch)
        levels = [p["level"] for p in json.loads((fs.OUT / "kits" / "voices" / "kit.json").read_text())["pads"]]
        self.assertEqual(levels, [fs.PAD_LEVEL] * 2, "an unbalanced pack keeps its level")

    def test_the_notice_and_the_change_reach_the_credits(self):
        fs.build_voice(self.pack, fetch=self.fake_fetch)
        fs.write_manifest()
        credits = (fs.OUT / "CREDITS.txt").read_text()
        self.assertIn("Voices: BSD-style. Somebody. Modified: rewritten as mono WAV.", credits)
        self.assertIn("Keep this notice.", credits)

    def test_manifest_only_indexes_what_is_built(self):
        fs.build_voice(self.pack, fetch=self.fake_fetch)
        saved, fs.LEDGER = fs.LEDGER, self.root / "no-ledger.json"
        try:
            fs.main(["--manifest"])
        finally:
            fs.LEDGER = saved
        manifest = json.loads((fs.OUT / "manifest.json").read_text())
        self.assertEqual([k["id"] for k in manifest["kits"]], ["voices"], "built, so listed, without the ledger")
        self.assertTrue((fs.OUT / "CREDITS.txt").is_file())

    def test_the_pin_covers_every_file(self):
        other = json.loads(json.dumps(self.pack))
        other["files"][5]["sha256"] = "f" * 64
        self.assertNotEqual(fs.pinned(other), fs.pinned(self.pack))
        self.assertEqual(fs.pinned({"sha256": "abc"}), "abc", "an archive keeps its own")
        self.assertNotEqual(fs.pinned({**self.pack, "balance": True}), fs.pinned(self.pack), "balancing rebuilds")

    def test_a_wrong_checksum_stops_the_build(self):
        src = self.root / "a.wav"
        src.write_bytes(wav_bytes())
        good = {"url": src.as_uri(), "sha256": fs.sha256(src), "name": "Fine."}
        self.assertTrue(fs.fetch_file(self.pack, 0, good).is_file())
        bad = {**good, "sha256": "0" * 64}
        with self.assertRaises(SystemExit):
            fs.fetch_file(self.pack, 1, bad)


class VoiceLedger(unittest.TestCase):
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


class KitLedger(unittest.TestCase):
    def test_every_kit_of_files_is_pinned_on_its_own_pads(self):
        ledger = json.loads(fs.LEDGER.read_text())
        kits = [p for p in ledger["packs"] if p.get("kind") == "drums" and "files" in p]
        self.assertTrue(kits)
        for p in kits:
            pads = [f.get("pad", i) for i, f in enumerate(p["files"])]
            self.assertEqual(len(pads), len(set(pads)), p["id"])
            self.assertTrue(all(0 <= pad < fs.PADS for pad in pads), p["id"])
            for f in p["files"]:
                self.assertRegex(f["sha256"], r"^[0-9a-f]{64}$", p["id"])
                self.assertTrue(f["url"].startswith("https://") and f["name"], p["id"])

    def test_the_jazz_kit_is_for_this_machine_only(self):
        # Dirt-Samples has no license (#661): fetched for personal use, like the tidal kits.
        jazz = next(p for p in json.loads(fs.LEDGER.read_text())["packs"] if p["id"] == "jazz")
        self.assertIs(jazz["redistribute"], False)
        self.assertIn("do not commit, publish or redistribute", jazz["license"])
        self.assertTrue(all("/c74fc80f8db8038f6a33648ffef5ac00a07ad402/jazz/" in f["url"] for f in jazz["files"]))
        by_pad = {f["pad"]: f["url"].rsplit("/", 1)[1] for f in jazz["files"]}
        self.assertEqual([by_pad[k] for k in (0, 2, 6, 10)], ["000_BD.wav", "007_SN.wav", "003_HH.wav", "004_OH.wav"])


class Untracked(unittest.TestCase):
    def test_no_fetched_file_is_committed(self):
        """What `make samples` writes is gitignored and stays out of git (#771):
        a force-added file comes back on every checkout and rebase."""
        root = Path(__file__).resolve().parent.parent
        out = subprocess.run(["git", "ls-files", "-ci", "--exclude-standard"], cwd=root,
                             capture_output=True, text=True, check=True).stdout
        self.assertEqual(out.split(), [])


if __name__ == "__main__":
    unittest.main()
