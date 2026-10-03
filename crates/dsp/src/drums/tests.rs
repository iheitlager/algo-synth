use super::*;
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
        kit.render(&sine, block);
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
                    let mut loudest = Kit::new(SR);
                    loudest.set_accent(1.0);
                    let (out, kit) = hit_with(loudest, pad, p, true, 4.0);
                    let at = format!("{name} tune {tune} decay {decay} tone {tone}");
                    assert!(out.iter().all(|s| s.is_finite()), "{at}: not finite");
                    assert!(peak(&out) <= 1.0, "{at}: peak {}", peak(&out));
                    assert!(peak(&out) > 0.05, "{at}: too quiet");
                    let mean = out.iter().sum::<f32>() / out.len() as f32;
                    assert!(mean.abs() < 1.0e-3, "{at}: DC {mean}");
                    assert!(!kit.active(), "{at}: still sounding after 4 s");
                    assert!(
                        out[out.len() - ms(300.0)..].iter().all(|s| *s == 0.0),
                        "{at}: tail"
                    );
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
    assert!(hz(&ht[ms(150.0)..]) > 1.4 * hz(&lt[ms(150.0)..]));
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
    kit.render(&sine, &mut buf);
    kit.trigger(Pad::Ch, 1.0, false);
    let mut rest = vec![0.0; ms(200.0)];
    kit.render(&sine, &mut rest);
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
    kit.render(&sine, &mut out);
    kit.trigger(Pad::Bd, 0.0, false);
    kit.trigger(Pad::Sn, f32::NAN, true);
    kit.render(&sine, &mut out);
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
