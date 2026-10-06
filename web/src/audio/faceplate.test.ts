import { describe, expect, it } from 'vitest'
import {
  NEW_AMOUNT, amountToPos, playedVoice, jackName, stepped, envPath, envPoints, envWidths, findSlot, fmtUnit, freeSlot, nearestStep, posToAmount, pressCell, stepIndex, wavePath,
  type PatchSlot,
} from './faceplate'

const env = { a: 0.1, d: 0.4, s: 0.5, r: 0.8 }

describe('envelope curve', () => {
  it('rises to full over the attack, falls to the sustain, holds, and falls to zero', () => {
    const w = 200
    const h = 60
    const pad = 3
    const pts = envPoints(env, w, h, pad)
    const base = h - pad
    const top = pad
    const [wa, wd, ws, wr] = envWidths(env, w - 2 * pad)
    expect(pts[0]).toEqual([pad, base])
    const peak = pts.reduce((best, p) => (p[1] < best[1] ? p : best), pts[0] as [number, number])
    expect(peak[1]).toBeCloseTo(top)
    expect(peak[0]).toBeCloseTo(pad + wa, 1)
    const hold = pts.find((p) => Math.abs(p[0] - (pad + wa + wd)) < 0.01 && p[1] > top + 1)
    expect(hold?.[1]).toBeCloseTo(base - 0.5 * (base - top), 1)
    const last = pts[pts.length - 1] as [number, number]
    expect(last[1]).toBeCloseTo(base)
    expect(last[0]).toBeCloseTo(pad + wa + wd + ws + wr, 1)
  })

  it('goes left to right and stays inside its box', () => {
    for (const t of [env, { a: 0.001, d: 0.001, s: 0, r: 0.001 }, { a: 10, d: 10, s: 1, r: 10 }, { a: 2, d: 0.5, s: 7, r: -1 }]) {
      const pts = envPoints(t, 150, 56)
      for (let i = 1; i < pts.length; i++) expect((pts[i] as number[])[0]).toBeGreaterThanOrEqual((pts[i - 1] as number[])[0] as number)
      for (const [x, y] of pts) {
        expect(x).toBeGreaterThanOrEqual(0)
        expect(x).toBeLessThanOrEqual(150)
        expect(y).toBeGreaterThanOrEqual(0)
        expect(y).toBeLessThanOrEqual(56)
      }
    }
  })

  it('draws longer times as longer segments, and never loses a short one', () => {
    const [, , , rShort] = envWidths({ ...env, r: 0.05 }, 200)
    const [, , , rLong] = envWidths({ ...env, r: 4 }, 200)
    expect(rLong).toBeGreaterThan(rShort)
    const [a, d, s, r] = envWidths({ a: 0.001, d: 0.001, s: 0, r: 10 }, 200)
    expect(Math.min(a, d, s, r)).toBeGreaterThan(200 * 0.05)
    expect(a + d + s + r).toBeCloseTo(200)
  })

  it('is an SVG path of line segments', () => {
    const p = envPath(env, 150, 56)
    expect(p.startsWith('M')).toBe(true)
    expect(p).toContain('L')
    expect(p).not.toContain('NaN')
  })
})

describe('waveform icons', () => {
  it('draws the four waveforms, and nothing for a name that is not one', () => {
    for (const n of ['Saw', 'Pulse', 'Square', 'Triangle', 'Sine']) {
      const p = wavePath(n, 24, 14)
      expect(p, n).toMatch(/^M/)
      expect(p).not.toContain('NaN')
    }
    expect(wavePath('White', 24, 14)).toBeNull()
  })
})

describe('stepped selectors', () => {
  const options = [['Off', 0], ['⅓', Math.fround(1 / 3)], ['⅔', Math.fround(2 / 3)], ['Full', 1]] as const
  it('shows the option nearest what the engine reports', () => {
    expect(nearestStep(options, 0)).toBe(0)
    expect(nearestStep(options, 0.3333)).toBe(1)
    expect(nearestStep(options, 0.7)).toBe(2)
    expect(nearestStep(options, 5)).toBe(3)
  })
  it('steps with the arrows and jumps with Home and End, staying in range', () => {
    expect(stepIndex(0, 4, 'ArrowRight')).toBe(1)
    expect(stepIndex(3, 4, 'ArrowRight')).toBe(3)
    expect(stepIndex(0, 4, 'ArrowLeft')).toBe(0)
    expect(stepIndex(2, 4, 'Home')).toBe(0)
    expect(stepIndex(1, 4, 'End')).toBe(3)
    expect(stepIndex(1, 4, 'a')).toBeNull()
  })
})

describe('value readouts', () => {
  it('prints each unit as the panel would', () => {
    expect(fmtUnit('pct', 0.625)).toBe('63%')
    expect(fmtUnit('bip', 0.5)).toBe('+50%')
    expect(fmtUnit('bip', -0.25)).toBe('-25%')
    expect(fmtUnit('sec', 0.25)).toBe('250 ms')
    expect(fmtUnit('sec', 2.5)).toBe('2.50 s')
    expect(fmtUnit('hz', 4000)).toBe('4.0 k')
    expect(fmtUnit('rate', 5.5)).toBe('5.50 Hz')
    expect(fmtUnit('rate', 12)).toBe('12.0 Hz')
    expect(fmtUnit('st', 7)).toBe('+7 st')
    expect(fmtUnit('st', -12)).toBe('-12 st')
    expect(fmtUnit('ct', 0)).toBe('0 ct')
    expect(fmtUnit('int', 49.6)).toBe('50')
  })
})

describe('patch bay', () => {
  const none: PatchSlot = { source: 0, dest: 0, amount: 0 }
  const slots: PatchSlot[] = [{ source: 5, dest: 2, amount: 0.5 }, { source: 8, dest: 5, amount: -0.3 }, ...Array<PatchSlot>(6).fill(none)]

  it('finds the slot of a cell and the first free one', () => {
    expect(findSlot(slots, 8, 5)).toBe(1)
    expect(findSlot(slots, 8, 2)).toBe(-1)
    expect(freeSlot(slots)).toBe(2)
    expect(freeSlot(slots.map((s) => ({ ...s, source: 1 })))).toBe(-1)
  })

  it('removes a joined cell, and joins an empty one in the first free slot with a middle amount', () => {
    expect(pressCell(slots, 5, 2)).toEqual({ slot: 0, value: none })
    expect(pressCell(slots, 7, 5)).toEqual({ slot: 2, value: { source: 7, dest: 5, amount: NEW_AMOUNT } })
  })

  it('does nothing for a new cell when every slot is used', () => {
    expect(pressCell(slots.map(() => ({ source: 1, dest: 1, amount: 0 })), 7, 5)).toBeNull()
  })

  it('maps an amount to a knob position and back', () => {
    expect(amountToPos(-1)).toBe(0)
    expect(amountToPos(0)).toBe(0.5)
    expect(amountToPos(1)).toBe(1)
    expect(posToAmount(amountToPos(0.37))).toBeCloseTo(0.37)
    expect(posToAmount(2)).toBe(1)
  })
})

describe('stepped knob scale', () => {
  const coarse = stepped(-24, 24, 1)
  it('lands on whole steps across the range', () => {
    expect(coarse.toValue(0)).toBe(-24)
    expect(coarse.toValue(0.5)).toBe(0)
    expect(coarse.toValue(1)).toBe(24)
    expect(coarse.toValue(0.5 + 0.4 / 48)).toBe(0)
    expect(Number.isInteger(coarse.toValue(0.7312))).toBe(true)
    expect(coarse.toValue(9)).toBe(24)
  })
  it('puts a value back at its position', () => {
    expect(coarse.toPos(12)).toBeCloseTo(0.75)
    expect(coarse.toValue(coarse.toPos(-7))).toBe(-7)
  })
})

describe('jack names', () => {
  it('prints the engine ids as a panel would', () => {
    expect(jackName('Vco1')).toBe('VCO 1')
    expect(jackName('Vco2Pitch')).toBe('VCO 2 Pitch')
    expect(jackName('SampleHold')).toBe('S&H')
    expect(jackName('ModWheel')).toBe('Mod Wheel')
    expect(jackName('Cutoff')).toBe('Cutoff')
    expect(jackName('PulseWidth')).toBe('Pulse width')
    expect(jackName('Fenv')).toBe('Filter env')
    expect(jackName('Lfo2')).toBe('LFO 2')
    expect(jackName('Lfo2Rate')).toBe('LFO 2 rate')
    expect(jackName('HpCutoff')).toBe('HP cutoff')
    expect(jackName('Ramp')).toBe('Ramp')
  })
})

describe('modular voice', () => {
  it('is the song voice the first track on the synth plays, or none for a factory voice', () => {
    const tracks = [
      { synth: 0, voice: -1 },
      { synth: 2, voice: -1 },
      { synth: 2, voice: 1 },
      { synth: 3, voice: 0 },
    ]
    expect(playedVoice(tracks, 2)).toBe(1)
    expect(playedVoice(tracks, 3)).toBe(0)
    expect(playedVoice(tracks, 0)).toBe(-1)
    expect(playedVoice(tracks, 5)).toBe(-1)
  })
})
