use super::*;
use crate::mono::osc::Blep;
use crate::voice::sine_table;

const SR: f32 = 48_000.0;

/// A fresh kit's output for one hit of `pad`, `seconds` long.
fn hit(pad: Pad, p: PadParams, accent: bool, seconds: f32) -> (Vec<f32>, Kit) {
    hit_with(Kit::new(SR), pad, p, accent, seconds)
}

fn hit_with(mut kit: Kit, pad: Pad, p: PadParams, accent: bool, seconds: f32) -> (Vec<f32>, Kit) {
    let sine = sine_table();
    kit.set_params(pad, p);
    kit.trigger(pad, 1.0, accent);
    let mut out = vec![0.0; (seconds * SR) as usize];
    for block in out.chunks_mut(128) {
        kit.render(&sine, &Blep::new(), block);
    }
    (out, kit)
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0, |m, s| m.max(s.abs()))
}

/// Rising zero crossings per second.
fn hz(x: &[f32]) -> f32 {
    let ups = x.windows(2).filter(|w| w[0] < 0.0 && w[1] >= 0.0).count();
    ups as f32 * SR / x.len() as f32
}

/// Goertzel power at `freq`.
fn power(x: &[f32], freq: f32) -> f32 {
    let w = 2.0 * std::f32::consts::PI * freq / SR;
    let c = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0_f32, 0.0_f32);
    for v in x {
        let s0 = v + c * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    s1 * s1 + s2 * s2 - c * s1 * s2
}

fn ms(t: f32) -> usize {
    (t * SR / 1000.0) as usize
}

#[test]
fn every_pad_at_every_extreme_is_finite_bounded_without_dc_and_ends() {
    for machine in [Machine::Tr808, Machine::Tr909] {
        for (pad, name) in Pad::ALL {
            for tune in [-12.0, 0.0, 12.0] {
                for decay in [0.25, 4.0] {
                    for tone in [0.0, 1.0] {
                        let p = PadParams {
                            tune,
                            decay,
                            tone,
                            level: 1.0,
                        };
                        // The loudest hit: full level, accented by the most.
                        let mut kit = Kit::of(machine, SR);
                        kit.set_accent(1.0);
                        kit.set_params(machine.voice(pad), p);
                        kit.trigger(pad, 1.0, true);
                        // Render until the pad falls silent (at most 4 s), then 300 ms more.
                        let (sine, blep) = (sine_table(), Blep::new());
                        let mut out = Vec::new();
                        let mut block = [0.0_f32; 128];
                        while kit.active() && out.len() < ms(4000.0) {
                            block.fill(0.0);
                            kit.render(&sine, &blep, &mut block);
                            out.extend_from_slice(&block);
                        }
                        let at =
                            format!("{machine:?} {name} tune {tune} decay {decay} tone {tone}");
                        assert!(!kit.active(), "{at}: still sounding after 4 s");
                        let mut tail = vec![0.0_f32; ms(300.0)];
                        kit.render(&sine, &blep, &mut tail);
                        assert!(tail.iter().all(|s| *s == 0.0), "{at}: tail");
                        assert!(out.iter().all(|s| s.is_finite()), "{at}: not finite");
                        assert!(peak(&out) <= 1.0, "{at}: peak {}", peak(&out));
                        assert!(peak(&out) > 0.05, "{at}: too quiet");
                        // The mean over 4 s, as the hit and its silence after.
                        let mean = out.iter().sum::<f32>() / ms(4000.0) as f32;
                        assert!(mean.abs() < 1.0e-3, "{at}: DC {mean}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_kick_falls_to_its_tune() {
    let (out, _) = hit(Pad::Bd, PadParams::default(), false, 0.6);
    let early = hz(&out[..ms(15.0)]);
    let late = hz(&out[ms(200.0)..ms(600.0)]);
    assert!(early > 1.5 * late, "early {early} Hz, late {late} Hz");
    assert!((late - 50.0).abs() < 5.0, "settles near 50 Hz, got {late}");
}

#[test]
fn tune_moves_the_pitch_by_semitones() {
    let up = PadParams {
        tune: 12.0,
        ..PadParams::default()
    };
    let (base, _) = hit(Pad::Bd, PadParams::default(), false, 0.6);
    let (oct, _) = hit(Pad::Bd, up, false, 0.6);
    let ratio = hz(&oct[ms(200.0)..ms(600.0)]) / hz(&base[ms(200.0)..ms(600.0)]);
    assert!((ratio - 2.0).abs() < 0.15, "an octave up, got ×{ratio}");
    let (lt, _) = hit(Pad::Lt, PadParams::default(), false, 0.5);
    let (ht, _) = hit(Pad::Ht, PadParams::default(), false, 0.5);
    let settled = |x: &[f32]| hz(&x[ms(150.0)..ms(250.0)]);
    assert!(settled(&ht) > 1.4 * settled(&lt));
}

#[test]
fn the_hats_sit_high() {
    for pad in [Pad::Ch, Pad::Oh] {
        let (out, _) = hit(pad, PadParams::default(), false, 0.05);
        let x = &out[..ms(40.0)];
        assert!(power(x, 8000.0) > 30.0 * power(x, 300.0), "{}", pad.name());
    }
}

#[test]
fn the_closed_hat_chokes_the_open_hat() {
    let sine = sine_table();
    let mut kit = Kit::new(SR);
    kit.trigger(Pad::Oh, 1.0, false);
    let mut buf = vec![0.0; ms(10.0)];
    kit.render(&sine, &Blep::new(), &mut buf);
    kit.trigger(Pad::Ch, 1.0, false);
    let mut rest = vec![0.0; ms(200.0)];
    kit.render(&sine, &Blep::new(), &mut rest);
    assert!(!kit.active(), "only the closed hat's short decay was left");
    let (alone, open) = hit(Pad::Oh, PadParams::default(), false, 0.21);
    assert!(
        open.active() && peak(&alone[ms(150.0)..]) > 0.01,
        "unchoked it rings on"
    );
}

#[test]
fn the_clap_comes_in_bursts() {
    let (out, _) = hit(Pad::Cp, PadParams::default(), false, 0.05);
    // Each burst restarts the envelope: the end of a gap is quieter than
    // the start of the next burst.
    for b in 1..=3 {
        let at = ms(10.0 * b as f32);
        let before = peak(&out[at - ms(1.5)..at]);
        let after = peak(&out[at..at + ms(1.5)]);
        assert!(after > 1.5 * before, "burst {b}: {before} then {after}");
    }
}

#[test]
fn an_accent_is_louder_by_the_amount() {
    for (pad, _) in Pad::ALL {
        let (plain, _) = hit(pad, PadParams::default(), false, 0.3);
        let (acc, _) = hit(pad, PadParams::default(), true, 0.3);
        let r = peak(&acc) / peak(&plain);
        assert!((r - 1.5).abs() < 1.0e-3, "{}: ×{r}", pad.name());
    }
}

#[test]
fn the_same_hit_gives_the_same_samples() {
    for (pad, _) in Pad::ALL {
        let (a, _) = hit(pad, PadParams::default(), false, 0.2);
        let (b, _) = hit(pad, PadParams::default(), false, 0.2);
        assert_eq!(a, b);
    }
}

#[test]
fn silent_hits_and_idle_kits_add_nothing() {
    let sine = sine_table();
    let mut kit = Kit::new(SR);
    let mut out = vec![0.25; 256];
    kit.render(&sine, &Blep::new(), &mut out);
    kit.trigger(Pad::Bd, 0.0, false);
    kit.trigger(Pad::Sn, f32::NAN, true);
    kit.render(&sine, &Blep::new(), &mut out);
    assert!(out.iter().all(|s| *s == 0.25));
    assert!(!kit.active());
}

#[test]
fn names_notes_and_knobs() {
    for (pad, name) in Pad::ALL {
        assert_eq!(pad.name(), name);
        assert_eq!(Pad::from_name(name), Some(pad));
        assert_eq!(Pad::from_note(pad.note()), Some(pad));
    }
    assert_eq!(Pad::from_name("xx"), None);
    assert_eq!(Pad::from_note(60), None);
    let mut kit = Kit::new(SR);
    let wild = PadParams {
        tune: 99.0,
        decay: f32::NAN,
        tone: -1.0,
        level: 7.0,
    };
    kit.set_params(Pad::Sn, wild);
    let p = kit.params(Pad::Sn);
    assert_eq!((p.tune, p.decay, p.tone, p.level), (12.0, 1.0, 0.0, 1.0));
}

#[test]
fn every_key_plays_a_pad_and_general_midi_its_own() {
    let gm = [
        (35, Pad::Bd),
        (36, Pad::Bd),
        (38, Pad::Sn),
        (39, Pad::Cp),
        (40, Pad::Sn),
        (42, Pad::Ch),
        (44, Pad::Ch),
        (46, Pad::Oh),
        (45, Pad::Lt),
        (50, Pad::Ht),
        (56, Pad::Cb),
    ];
    for (note, pad) in gm {
        assert_eq!(Pad::from_gm(note), pad, "note {note}");
    }
    for (pad, _) in Pad::ALL {
        assert_eq!(Pad::from_gm(pad.note()), pad);
    }
    let added = [
        (37, Pad::Rs),
        (47, Pad::Mt),
        (49, Pad::Cr),
        (51, Pad::Rd),
        (52, Pad::Cy),
        (62, Pad::Hc),
        (63, Pad::Mc),
        (64, Pad::Lc),
        (70, Pad::Ma),
        (75, Pad::Cl),
    ];
    for (note, pad) in added {
        assert_eq!(Pad::from_gm(note), pad, "note {note}");
    }
    // Outside the map, the octave from 36 repeats.
    assert_eq!(Pad::from_gm(24), Pad::Bd);
    assert_eq!(Pad::from_gm(66), Pad::Ch);
    assert_eq!(Pad::from_gm(127), Pad::Lt);
}

/// #140: the 808's cowbell, as its circuit has it: the 540 and 800 Hz squares
/// through a band-pass near 850 Hz, a fast decay and then a slow one.
#[test]
fn the_cowbell_sits_in_its_band_and_falls_in_two_stages() {
    let (out, _) = hit(Pad::Cb, PadParams::default(), false, 0.6);
    let body = &out[..ms(60.0)];
    let band = power(body, 800.0).max(power(body, 540.0));
    for far in [3000.0, 5000.0, 9000.0] {
        assert!(band > 1000.0 * power(body, far), "{far} Hz is 30 dB down");
    }
    // The first 50 ms fall far faster than the tail does.
    let at = |t: f32| peak(&out[ms(t)..ms(t + 10.0)]);
    let fast = at(0.0) / at(45.0);
    let slow = at(150.0) / at(195.0);
    assert!(fast > 2.0 * slow, "fast ×{fast}, slow ×{slow}");
}

#[test]
fn the_claves_ring_at_their_tune_and_stop_short() {
    for (tune, hz) in [(0.0, 2500.0), (12.0, 5000.0)] {
        let p = PadParams {
            tune,
            ..PadParams::default()
        };
        let (out, kit) = hit(Pad::Cl, p, false, 0.1);
        let ring = &out[..ms(15.0)];
        assert!(
            power(ring, hz) > 30.0 * power(ring, hz * 0.6),
            "{tune}: rings at {hz} Hz"
        );
        assert!(
            peak(ring) > 0.1,
            "{tune}: audible, the kHz nulls of a long strike avoided"
        );
        assert!(
            out[ms(60.0)..].iter().all(|s| *s == 0.0),
            "{tune}: gone within 60 ms"
        );
        assert!(!kit.active());
    }
}

#[test]
fn the_congas_sit_above_the_toms() {
    let pitch = |pad: Pad| {
        let (out, _) = hit(pad, PadParams::default(), false, 0.3);
        hz(&out[ms(40.0)..ms(140.0)])
    };
    let [lt, mt, ht, lc, mc, hc] =
        [Pad::Lt, Pad::Mt, Pad::Ht, Pad::Lc, Pad::Mc, Pad::Hc].map(pitch);
    assert!(
        lt < mt && mt < ht && ht < lc && lc < mc && mc < hc,
        "{lt} {mt} {ht} {lc} {mc} {hc}"
    );
}

fn hit_on(machine: Machine, pad: Pad, seconds: f32) -> Vec<f32> {
    hit_with(
        Kit::of(machine, SR),
        pad,
        PadParams::default(),
        false,
        seconds,
    )
    .0
}

/// The frequency of the first full cycle that starts after `from_ms`.
fn cycle_hz(x: &[f32], from_ms: f32) -> f32 {
    let ups: Vec<usize> = x
        .windows(2)
        .enumerate()
        .filter(|(i, w)| *i >= ms(from_ms) && w[0] < 0.0 && w[1] >= 0.0)
        .map(|(i, _)| i)
        .take(2)
        .collect();
    SR / (ups[1] - ups[0]) as f32
}

/// #148: the 909's kick reaches its tune sooner than the 808's, and settles there.
#[test]
fn the_909_kick_settles_faster_at_its_tune() {
    let tr808 = hit_on(Machine::Tr808, Pad::Bd, 0.6);
    let tr909 = hit_on(Machine::Tr909, Pad::Bd, 0.6);
    let left_808 = cycle_hz(&tr808, 15.0) / 50.0;
    let left_909 = cycle_hz(&tr909, 15.0) / 52.0;
    assert!(
        left_909 < left_808,
        "less sweep left after 15 ms: {left_909} vs {left_808}"
    );
    assert!(left_909 < 1.1, "the 909 is all but there: {left_909}");
    let late = hz(&tr909[ms(200.0)..ms(600.0)]);
    assert!((late - 52.0).abs() < 5.0, "settles near 52 Hz, got {late}");
}

#[test]
fn the_909s_hats_and_cymbals_sit_high() {
    for pad in [Pad::Ch, Pad::Oh, Pad::Cr, Pad::Rd] {
        let x = hit_on(Machine::Tr909, pad, 0.05);
        let x = &x[..ms(40.0)];
        assert!(power(x, 8000.0) > 30.0 * power(x, 300.0), "{}", pad.name());
    }
}

#[test]
fn each_machine_stands_in_its_nearest_voice() {
    assert_eq!(Machine::Tr808.voice(Pad::Cr), Pad::Cy);
    assert_eq!(Machine::Tr808.voice(Pad::Rd), Pad::Cy);
    assert_eq!(Machine::Tr909.voice(Pad::Cl), Pad::Rs);
    assert_eq!(Machine::Tr909.voice(Pad::Lc), Pad::Lt);
    assert_eq!(Machine::Tr909.voice(Pad::Cy), Pad::Cr);
    for (pad, name) in Pad::ALL {
        for machine in [Machine::Tr808, Machine::Tr909] {
            assert!(
                peak(&hit_on(machine, pad, 0.3)) > 0.05,
                "{machine:?} {name} is heard"
            );
        }
    }
}

#[test]
fn the_909s_closed_hat_chokes_its_open_hat() {
    let sine = sine_table();
    let blep = Blep::new();
    let mut kit = Kit::of(Machine::Tr909, SR);
    kit.trigger(Pad::Oh, 1.0, false);
    let mut buf = vec![0.0; ms(10.0)];
    kit.render(&sine, &blep, &mut buf);
    kit.trigger(Pad::Ch, 1.0, false);
    let mut rest = vec![0.0; ms(200.0)];
    kit.render(&sine, &blep, &mut rest);
    assert!(!kit.active(), "only the closed hat's short decay was left");
}
