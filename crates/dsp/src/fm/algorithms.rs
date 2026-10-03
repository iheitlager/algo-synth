//! The DX7's 32 algorithms (spec 006 Req 13): how its six operators feed each
//! other and the output. The table is the DX7's own routing, as a set of flags per
//! operator, in the form of Google's "music-synthesizer-for-android" (Apache-2.0,
//! see NOTICE). Operator 0 is the DX7's operator 6, which is evaluated first.

/// An operator writes bus 1.
pub const OUT_BUS_ONE: u8 = 1 << 0;
/// An operator writes bus 2.
pub const OUT_BUS_TWO: u8 = 1 << 1;
/// An operator adds to its bus (the main output when it writes none) instead of replacing it.
pub const OUT_BUS_ADD: u8 = 1 << 2;
/// An operator reads bus 1 as the phase modulation of its oscillator.
pub const IN_BUS_ONE: u8 = 1 << 4;
/// An operator reads bus 2.
pub const IN_BUS_TWO: u8 = 1 << 5;
/// An operator takes its feedback in, and out.
pub const FB_IN: u8 = 1 << 6;
pub const FB_OUT: u8 = 1 << 7;

pub const ALGORITHMS: [[u8; 6]; 32] = [
    [0xc1, 0x11, 0x11, 0x14, 0x01, 0x14],
    [0x01, 0x11, 0x11, 0x14, 0xc1, 0x14],
    [0xc1, 0x11, 0x14, 0x01, 0x11, 0x14],
    [0x41, 0x11, 0x94, 0x01, 0x11, 0x14],
    [0xc1, 0x14, 0x01, 0x14, 0x01, 0x14],
    [0x41, 0x94, 0x01, 0x14, 0x01, 0x14],
    [0xc1, 0x11, 0x05, 0x14, 0x01, 0x14],
    [0x01, 0x11, 0xc5, 0x14, 0x01, 0x14],
    [0x01, 0x11, 0x05, 0x14, 0xc1, 0x14],
    [0x01, 0x05, 0x14, 0xc1, 0x11, 0x14],
    [0xc1, 0x05, 0x14, 0x01, 0x11, 0x14],
    [0x01, 0x05, 0x05, 0x14, 0xc1, 0x14],
    [0xc1, 0x05, 0x05, 0x14, 0x01, 0x14],
    [0xc1, 0x05, 0x11, 0x14, 0x01, 0x14],
    [0x01, 0x05, 0x11, 0x14, 0xc1, 0x14],
    [0xc1, 0x11, 0x02, 0x25, 0x05, 0x14],
    [0x01, 0x11, 0x02, 0x25, 0xc5, 0x14],
    [0x01, 0x11, 0x11, 0xc5, 0x05, 0x14],
    [0xc1, 0x14, 0x14, 0x01, 0x11, 0x14],
    [0x01, 0x05, 0x14, 0xc1, 0x14, 0x14],
    [0x01, 0x14, 0x14, 0xc1, 0x14, 0x14],
    [0xc1, 0x14, 0x14, 0x14, 0x01, 0x14],
    [0xc1, 0x14, 0x14, 0x01, 0x14, 0x04],
    [0xc1, 0x14, 0x14, 0x14, 0x04, 0x04],
    [0xc1, 0x14, 0x14, 0x04, 0x04, 0x04],
    [0xc1, 0x05, 0x14, 0x01, 0x14, 0x04],
    [0x01, 0x05, 0x14, 0xc1, 0x14, 0x04],
    [0x04, 0xc1, 0x11, 0x14, 0x01, 0x14],
    [0xc1, 0x14, 0x01, 0x14, 0x04, 0x04],
    [0x04, 0xc1, 0x11, 0x14, 0x04, 0x04],
    [0xc1, 0x14, 0x04, 0x04, 0x04, 0x04],
    [0xc4, 0x04, 0x04, 0x04, 0x04, 0x04],
];

/// The routing of algorithm `n` (0..=31; out of range is algorithm 1).
pub fn algorithm(n: usize) -> &'static [u8; 6] {
    ALGORITHMS.get(n).unwrap_or(&ALGORITHMS[0])
}

/// Which operators (in the same order) are carriers: they write the main output.
pub fn carriers(n: usize) -> [bool; 6] {
    let a = algorithm(n);
    std::array::from_fn(|i| a.get(i).is_some_and(|f| f & 3 == 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every algorithm has a carrier, and an operator only reads a bus that an
    /// earlier operator writes (the table is evaluated in order).
    #[test]
    fn every_algorithm_is_a_sound_routing() {
        for (n, alg) in ALGORITHMS.iter().enumerate() {
            let mut written = [true, false, false];
            for (op, flags) in alg.iter().enumerate() {
                let inbus = usize::from((flags >> 4) & 3);
                assert!(
                    written[inbus],
                    "algorithm {} operator {op} reads an empty bus",
                    n + 1
                );
                // A feedback loop reads itself, and a bus-reading operator has none.
                let outbus = usize::from(flags & 3);
                written[outbus] = true;
            }
            assert!(
                carriers(n).iter().any(|c| *c),
                "algorithm {} has no carrier",
                n + 1
            );
            // The last operator (the DX7's operator 1) always reaches the output.
            assert_eq!(alg[5] & 3, 0, "algorithm {}", n + 1);
        }
    }

    /// The well-known counts of carriers: algorithm 32 is six in parallel, 1 is two stacks.
    #[test]
    fn carriers_match_the_known_algorithms() {
        let count = |n: usize| carriers(n - 1).iter().filter(|c| **c).count();
        assert_eq!(count(1), 2);
        assert_eq!(count(5), 3);
        assert_eq!(count(22), 4);
        assert_eq!(count(31), 5);
        assert_eq!(count(32), 6);
    }

    /// Every algorithm has one feedback loop: it enters and leaves at one operator, except in
    /// algorithms 4 and 6, whose loop runs through three.
    #[test]
    fn every_algorithm_has_one_feedback_loop() {
        for (n, alg) in ALGORITHMS.iter().enumerate() {
            let ins: Vec<usize> = alg
                .iter()
                .enumerate()
                .filter(|(_, f)| **f & FB_IN != 0)
                .map(|(i, _)| i)
                .collect();
            let outs: Vec<usize> = alg
                .iter()
                .enumerate()
                .filter(|(_, f)| **f & FB_OUT != 0)
                .map(|(i, _)| i)
                .collect();
            assert_eq!((ins.len(), outs.len()), (1, 1), "algorithm {}", n + 1);
            if matches!(n + 1, 4 | 6) {
                assert_ne!(
                    ins,
                    outs,
                    "algorithm {} loops through three operators",
                    n + 1
                );
            } else {
                assert_eq!(ins, outs, "algorithm {}", n + 1);
            }
        }
    }

    /// The view draws its diagrams from a copy of the table in `web/src/audio/dx7.ts`
    /// (ADR-0004's rule for ids, applied to this table): it must be the same one.
    #[test]
    fn the_typescript_copy_matches() {
        let ts = include_str!("../../../../web/src/audio/dx7.ts");
        let start = ts.find("export const ALGORITHMS").expect("the table");
        let body = &ts[start
            ..ts[start..]
                .find("\n]\n")
                .map(|i| start + i)
                .expect("its end")];
        let rows: Vec<Vec<u8>> = body
            .lines()
            .filter(|l| l.trim_start().starts_with("[0x"))
            .map(|l| {
                l.split("0x")
                    .skip(1)
                    .map(|h| u8::from_str_radix(&h[..2], 16).expect("a hex byte"))
                    .collect()
            })
            .collect();
        assert_eq!(rows.len(), 32);
        for (n, (ts_row, rust)) in rows.iter().zip(ALGORITHMS.iter()).enumerate() {
            assert_eq!(ts_row.as_slice(), rust.as_slice(), "algorithm {}", n + 1);
        }
    }

    #[test]
    fn an_unknown_algorithm_is_the_first() {
        assert_eq!(algorithm(99), algorithm(0));
    }
}
