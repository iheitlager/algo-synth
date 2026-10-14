# 011: Chips

The C64's MOS 6581/8580 SID and the NES's Ricoh 2A03 as synth models of a Chips family (#112, ADR-0034), and their parts as Modular UGens. Decisions: ADR-0001, ADR-0002, ADR-0004, ADR-0007, ADR-0013, ADR-0024, ADR-0025, ADR-0034.

Common to every requirement: `render` follows ADR-0002, every parameter, model and preset id is mirrored in `web/src/audio/params.ts` (ADR-0004), and a chip is an emulation of its character from public documentation, not cycle-exact: each requirement names the property of the chip it must have, and is tested on that. Tests render offline at 48 kHz.

### Requirement 1: The SID's oscillator [MUST]

A `Sid` synth's note SHALL play a whole chip: three oscillators, each a 24-bit accumulator advanced by a 16-bit frequency register, `f = Fn · clock / 2^24`, at a PAL (985 248 Hz) or NTSC (1 022 727 Hz) clock (`SidClock`), stepped once per output sample by the chip cycles it holds, in fixed point. A note SHALL play the register value nearest its pitch, up to 0xFFFF. The waveforms (`Sid1Wave`…, bits 1 triangle, 2 saw, 4 pulse, 8 noise) SHALL be 12 bits: the saw the accumulator's top 12 bits, the triangle its bits 22..12 folded by bit 23, the pulse high where the top 12 bits reach the 12-bit width (`Sid1Pw`). The noise SHALL be the 23-bit LFSR, shifted on each rise of accumulator bit 19 and read as 8 bits, the low four always clear. Combined waveforms SHALL be the AND of their bits, and noise with another waveform SHALL write the AND back into the LFSR until it locks silent. Ring modulation SHALL flip the triangle where the source oscillator's MSB is set, and sync SHALL reset the accumulator where the source's MSB rises; oscillator 1's source SHALL be 3, 2's be 1 and 3's be 2.

**Implementation:** `crates/dsp/src/chip/sid.rs::Osc`, `crates/dsp/src/chip/sid.rs::freq_register`, `crates/dsp/src/chip/sid_voice.rs::SidVoice`

#### Scenario: the frequency register

- GIVEN a SID triangle on the 8580 on note 69
- WHEN it plays for a second
- THEN it crosses zero upward 440.03 times (Fn 7493 on PAL), within two

**Tests:** `crates/dsp/src/engine/tests.rs::the_sid_plays_its_frequency_register`, `crates/dsp/src/chip/sid.rs::tests::the_frequency_register_is_16_bits`, `crates/dsp/src/chip/sid.rs::tests::the_accumulator_keeps_the_chip_pitch`, `crates/dsp/src/chip/sid.rs::tests::waveforms_are_12_bits`, `crates/dsp/src/chip/sid.rs::tests::ring_modulation_flips_the_triangle`, `crates/dsp/src/chip/sid.rs::tests::noise_is_an_8_bit_lfsr`, `crates/dsp/src/chip/sid.rs::tests::combined_noise_locks_up`, `crates/dsp/src/chip/sid.rs::tests::sync_resets_the_accumulator`

### Requirement 2: The SID's envelope and DACs [MUST]

Each oscillator SHALL have the chip's ADSR: 4-bit attack, decay and release rates into a 15-bit rate counter against the period table (9 to 31 251 cycles a step: attack 2 ms to 8 s, decay and release 6 ms to 24 s), an 8-bit counter rising linearly and falling through the exponential divider (1, 2, 4, 8, 16, 30 steps below 0xFF, 0x5D, 0x36, 0x1A, 0x0E, 0x06), and a 4-bit sustain at `S · 0x11`. A rate lowered under the rate counter's count SHALL wait for its 15-bit wrap, the "ADSR bug"; `SidHardRestart` SHALL clear the counters on each note. The waveform and envelope SHALL go through each chip's DAC: an ideal R-2R ladder on the 8580, and on the 6581 a ladder of 2R/R = 2.2 without termination, whose codes are not evenly spaced nor always rising; the 6581's waveforms SHALL swing about 0x380, the 8580's about 0x800.

**Implementation:** `crates/dsp/src/chip/sid.rs::Env`, `crates/dsp/src/chip/sid.rs::wave_dac`, `crates/dsp/src/chip/sid.rs::env_dac`

#### Scenario: the ADSR bug

- GIVEN an envelope whose rate counter counted 10 000 cycles under a slow release
- WHEN it is gated with attack 0
- THEN it does not move until the counter has wrapped, (32 768 − 10 000) cycles later; after a hard restart it moves at once

**Tests:** `crates/dsp/src/chip/sid.rs::tests::the_adsr_bug_stalls_the_envelope`, `crates/dsp/src/chip/sid.rs::tests::the_attack_follows_the_rate_table`, `crates/dsp/src/chip/sid.rs::tests::decay_stops_at_the_sustain_level`, `crates/dsp/src/chip/sid.rs::tests::release_is_exponential_and_ends_at_zero`, `crates/dsp/src/chip/sid.rs::tests::the_8580_dac_is_linear`, `crates/dsp/src/chip/sid.rs::tests::the_6581_dac_is_not`

### Requirement 3: The SID's filter, band-limiting and player [MUST]

Oscillators routed to it (`Sid1Filter`…) SHALL go through one state-variable filter of an 11-bit cutoff (`SidCutoff`) and 4-bit resonance, its low-, band- and high-pass mixed by the mode bits, the cutoff on each chip's curve (8580: 30 Hz to 12 kHz near-linear; 6581: an S from about 220 Hz to 18 kHz, offset by `SidFilterBias`), its coefficients worked out only when they change. A 4-bit volume SHALL scale the mix, and `SidVoice3Off` mute oscillator 3 when unfiltered. With `SidBandLimit` the saw and pulse edges and every sync reset SHALL be band-limited with the BLEP table at their sub-sample time (ADR-0007); without, the raw steps play. Once a video frame (50.12 Hz PAL, 59.83 Hz NTSC) the player SHALL write the frequencies, add the pulse sweep (wrapping in 12 bits) and the cutoff sweep (clamped in 11), step the chord arpeggio (`SidArp`, every `SidArpSpeed` frames) and run a triangle vibrato after its delay. A `Sid` synth SHALL play up to eight chips, three notes of one oscillator each being the chip's own polyphony.

**Implementation:** `crates/dsp/src/chip/sid.rs::Filter`, `crates/dsp/src/chip/sid.rs::render_osc`, `crates/dsp/src/chip/sid_voice.rs::SidVoice::frame`, `crates/dsp/src/synth/sid.rs::DEF`

#### Scenario: band-limiting

- GIVEN a SID saw on note 96, raw and band-limited
- WHEN both play
- THEN the fundamental is the same within 10 %, and the energy between the harmonics is under a third of the raw one's

**Tests:** `crates/dsp/src/engine/tests.rs::the_sid_band_limits_its_edges`, `crates/dsp/src/engine/tests.rs::the_sid_filter_closes`, `crates/dsp/src/engine/tests.rs::the_sid_arpeggio_steps_by_frames`, `crates/dsp/src/chip/sid.rs::tests::the_cutoff_curves_span_each_chip`, `crates/dsp/src/chip/sid.rs::tests::the_filter_passes_and_stops`

### Requirement 4: The NES's channels [MUST]

A `Nes` synth's note SHALL play a whole 2A03 at an NTSC (1 789 773 Hz) or PAL (1 662 607 Hz) CPU clock (`NesRegion`):

- **Two pulses:** an 11-bit timer clocking the 8-step duty sequence (12.5, 25, 50, 75 %) every `2 (t + 1)` cycles, `f = CPU / 16 (t + 1)`. Each is muted under t = 8 and when its sweep would pass 0x7FF. Each has a 4-bit constant volume or a decay envelope clocked on quarter frames (looping on request). The sweep (shift, sign, period) is clocked on half frames, pulse 1 negating in ones' complement, pulse 2 in two's.
- **A triangle:** 32 4-bit steps every `t + 1` cycles, without volume. A linear counter in quarter frames holds its step when it runs out; it starts at a 0 step.
- **A noise:** the 15-bit LFSR at one of the 16 periods, its short mode tapping bit 6 (93 steps), with the pulses' envelope; its period may follow the note.
- **The DMC:** a 7-bit counter moved ±2 for each bit at one of the 16 rates, within 0..127, or set to 7-bit levels (`NesDmcDirect`). Its bits come from a pad's sample (`NesDmcSample`, a pad of the synth's kit, laid out by `pad` lines on its track, which a `Nes` synth track may have), stopped with the note, delta-encoded as it plays, read at the speed the note asks of the sample's root, for at most 4081 bytes' worth of bits.

The levels SHALL mix through the console's non-linear mixer, `95.88 / (8128 / (p1 + p2) + 100)` and `159.79 / (1 / (t/8227 + n/12241 + d/22638) + 100)`, then a Famicom's high-pass (37 Hz), a NES's (90 and 440 Hz high-pass, 14 kHz low-pass) or a DC blocker (`NesOutput`). With `NesBandLimit` the mixed output SHALL be band-limited at every channel's step. Once a frame-counter sequence the driver SHALL write the timers and step the arpeggio and vibrato as the SID's player does; a sweep SHALL own the pulse's pitch once the note starts.

**Implementation:** `crates/dsp/src/chip/nes.rs`, `crates/dsp/src/chip/nes_voice.rs::NesVoice`, `crates/dsp/src/synth/nes.rs::DEF`

#### Scenario: the timers

- GIVEN a NES pulse and, separately, a triangle on note 69 and 57
- WHEN each plays for a second
- THEN they cross zero upward `CPU / 16 · 254` (440.4) and `CPU / 32 · 254` (220.2) times, within two

**Tests:** `crates/dsp/src/engine/tests.rs::the_nes_plays_its_timers`, `crates/dsp/src/engine/tests.rs::a_nes_note_ends`, `crates/dsp/src/engine/tests.rs::the_nes_dmc_plays_a_sample`, `crates/dsp/src/chip/nes.rs::tests::timers_are_11_bits`, `crates/dsp/src/chip/nes.rs::tests::the_duties_are_the_chips`, `crates/dsp/src/chip/nes.rs::tests::the_pulse_mutes`, `crates/dsp/src/chip/nes.rs::tests::the_sweeps_negate_differently`, `crates/dsp/src/chip/nes.rs::tests::the_envelope_decays_and_loops`, `crates/dsp/src/chip/nes.rs::tests::the_triangle_has_32_steps`, `crates/dsp/src/chip/nes.rs::tests::the_noise_runs_its_lengths`, `crates/dsp/src/chip/nes.rs::tests::the_dmc_steps_by_two`, `crates/dsp/src/chip/nes.rs::tests::the_dmc_keeps_its_rate`, `crates/dsp/src/chip/nes.rs::tests::the_mixer_is_non_linear`, `crates/dsp/src/chip/nes.rs::tests::the_frame_counter_keeps_time`

### Requirement 5: NES drums [SHOULD]

With `NesKit` (the `NesKit` preset) a General MIDI drum key SHALL play the drum NES scores make of the channels:

- **Kick:** a triangle diving from 220 Hz over six frames, with a noise click.
- **Snare:** noise, and a short triangle.
- **Hats, cymbals and rim:** noise at their periods and decays, the ride and rim in short mode.
- **Toms:** a triangle falling to its pitch.

A drum key whose pad (a `samples` kit's or a `pad` line's, as on the pad sampler) holds a sample SHALL play it through the DMC instead, at its pad's tune. Any other key SHALL play the `NesDmcSample` pad, or noise at its pitch without one. A `drums` track SHALL take a `Nes` synth, so its lanes play it.

**Implementation:** `crates/dsp/src/chip/nes_voice.rs::Drum`, `crates/dsp/src/song.rs::fits`

#### Scenario: from lanes

- GIVEN `track kit drums Nes NesKit` with `bd x...x...x...x...`
- WHEN the song plays
- THEN the kick sounds, and a hat crosses zero more than four times as often as a kick

**Tests:** `crates/dsp/src/engine/tests.rs::the_nes_kit_plays_from_lanes`, `crates/dsp/src/engine/tests.rs::the_nes_kit_plays_pad_samples_through_the_dmc`, `crates/dsp/src/chip/nes_voice.rs::tests::drums_come_from_general_midi`

### Requirement 6: Chips in the Modular [MUST]

The Modular (spec 005 Req 11) SHALL reach the chips' parts as UGens running the models' code, every number a knob at its register's width:

- `SidOsc.ar(freq, wave, width, chip, sync, ring)`: a SID oscillator on a PAL chip's frequency register, the chip's DAC; a rising `sync` resets it, a positive `ring` flips its triangle.
- `SidEnv.kr(gate, attack, decay, sustain, release, chip)`: the SID's envelope, 0..1.
- `SidFilter.ar(in, cutoff, res, mode, chip, bias)`: the SID's filter.
- `NesPulse.ar(freq, duty, volume)`, `NesTri.ar(freq)` and `NesNoise.ar(period, short, volume)`: an NTSC 2A03's channels, swinging about half their volume.
- `NesDmc.ar(in, rate, direct)`: the DMC delta-encoding its input.

A voice SHALL hold at most 16 chip parts. The `ModularSid` preset SHALL play a SID patch built of them.

**Implementation:** `crates/dsp/src/modular.rs::Ugen` (`SidOsc`, `SidEnv`, `SidFilter`, `NesPulse`, `NesTri`, `NesNoise`, `NesDmc`), `crates/dsp/src/modular/sc.rs::Builder::ugen`, `crates/dsp/src/synth/modular.rs::DEF`

#### Scenario: the chip's pitch in a graph

- GIVEN `SidOsc.ar(freq, 1, 2048, 1) * SidEnv.kr(gate, 0, 0, 15, 0)` and, apart, `NesPulse.ar(freq, 2, 15)` on note 69
- WHEN each plays for a second
- THEN they cross zero upward 440.03 and 440.4 times, within two

**Tests:** `crates/dsp/src/engine/tests.rs::a_modular_voice_plays_the_sid_parts`, `crates/dsp/src/engine/tests.rs::a_modular_voice_plays_the_nes_channels`, `crates/dsp/tests/render_no_alloc.rs::a_busy_song_renders_without_allocating`

### Requirement 7: Panels [SHOULD]

The view SHALL list `Sid` and `Nes` under a Chips family, their panels drawn from `models.ts` with each register at its width, and the live arpeggiator beside them as on the poly synths.

**Implementation:** `web/src/audio/models.ts` (`sid`, `nes`, `FAMILIES`)

**Tests:** `web/src/audio/models.test.ts`, `crates/dsp/src/mono/preset.rs::tests::the_views_presets_per_model_match`
