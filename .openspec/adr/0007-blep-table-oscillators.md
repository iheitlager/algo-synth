# 0007: BLEP-table oscillators

**Status:** Accepted · **Date:** 2026-10-01

## Context

Spec 004 Req 1 asks for band-limited saw and pulse waves with aliasing below −60 dB for a 5 kHz saw at 48 kHz, and for a band-limited hard-sync reset. The usual cheap method, a 2-point polyBLEP, corrects only the sample on each side of a step. Measured on a 5 kHz saw it leaves aliases at −22 dB, so it cannot meet the target. Oversampling would multiply the cost of every oscillator and still needs a decimation filter.

## Decision

**Oscillators correct every step with a table of the band-limited step (BLEP).** The table holds the integral of a windowed sinc: 16 taps (8 zero crossings each side), Kaiser window with β = 6, a cutoff of 0.375 × the sample rate (18 kHz at 48 kHz), and 64 phases per sample, interpolated linearly. It is built once in `Engine::new` and shared by every voice.

Each oscillator writes its naive waveform into a 32-sample ring, 8 samples ahead of the read position. On each step (a saw wrap, a pulse edge, a sync reset) it adds the step height times the table residual to the 16 surrounding samples. Output is delayed by 8 samples (0.17 ms at 48 kHz). The same code handles all three kinds of step, so sync is band-limited without special cases.

Pulse width is latched at each cycle start, so a changing width never adds or drops an edge mid-cycle. The triangle stays naive: its corners are slope changes, which would need a BLAMP table, and the spec does not ask for it.

## Consequences

- Measured offline: aliases at −81 dB for a 5 kHz saw (the 32-tap variant reaches −98 dB, at twice the cost and latency).
- Each edge costs 16 table reads and multiply-adds. `render` itself does no transcendental math (ADR-0002).
- Mono output lags the other sources by 8 samples, which is too short to hear.
- Sinc ringing overshoots a step by about 9% of its height (Gibbs). A ±1 saw or pulse steps by 2, so a VCO peaks near ±1.16. The oscillator tests bound it at ±1.2, and a synced sweep, where steps can fall within a sample of each other, at ±1.5 (spec 004 Req 1).
