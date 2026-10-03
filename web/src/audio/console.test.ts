import { describe, expect, it } from 'vitest'
import {
  SWEEP, arcPath, bandDb, compOutDb, dbText, dbToPos, dragValue, eqDb, knobAngle, knobArc, ledSegments, levelToPos,
  logMap, logPos, polar, posToDb, posToLevel,
} from './console'

describe('knob geometry', () => {
  it('turns 270 degrees, from lower left to lower right', () => {
    expect(knobAngle(0)).toBe(-SWEEP)
    expect(knobAngle(0.5)).toBe(0)
    expect(knobAngle(1)).toBe(SWEEP)
    expect(knobAngle(7)).toBe(SWEEP)
  })

  it('puts points clockwise from straight up', () => {
    const [x, y] = polar(10, 10, 5, 0)
    expect(x).toBeCloseTo(10)
    expect(y).toBeCloseTo(5)
    const [rx, ry] = polar(10, 10, 5, 90)
    expect(rx).toBeCloseTo(15)
    expect(ry).toBeCloseTo(10)
  })

  it('draws no arc for no travel, and a large arc past half a turn', () => {
    expect(arcPath(20, 20, 17, 30, 30)).toBe('')
    expect(arcPath(20, 20, 17, -135, 135)).toContain('A17 17 0 1 1')
    expect(arcPath(20, 20, 17, -135, -90)).toContain('A17 17 0 0 1')
  })

  it('grows a bipolar arc from the centre, left or right', () => {
    expect(knobArc(20, 20, 17, 0.5, true)).toBe('')
    expect(knobArc(20, 20, 17, 0.9, true)).toContain(' 1 ')
    expect(knobArc(20, 20, 17, 0.1, true)).toContain('0 0 0 ')
  })

  it('drags 170 px for the whole range, and slower with shift', () => {
    expect(dragValue(0.2, 34, false)).toBeCloseTo(0.4)
    expect(dragValue(0.2, 34, true)).toBeCloseTo(0.2 + 34 / 700)
    expect(dragValue(0.9, 500, false)).toBe(1)
    expect(dragValue(0.1, -500, false)).toBe(0)
  })
})

describe('ranges', () => {
  it('maps 0..1 evenly in octaves and back', () => {
    const f = logMap(20, 20_000)
    expect(f(0)).toBeCloseTo(20)
    expect(f(1)).toBeCloseTo(20_000)
    expect(f(0.5)).toBeCloseTo(632.46, 1)
    for (const t of [0, 0.1, 0.5, 0.9, 1]) expect(logPos(20, 20_000)(f(t))).toBeCloseTo(t, 6)
  })
})

describe('fader', () => {
  it('has 0 dB at the top and maps to a linear level of 1', () => {
    expect(posToDb(1)).toBeCloseTo(0)
    expect(posToLevel(1)).toBeCloseTo(1)
  })

  it('is off at the bottom', () => {
    expect(posToDb(0)).toBe(-Infinity)
    expect(posToLevel(0)).toBe(0)
    expect(posToLevel(0.005)).toBe(0)
  })

  it('goes down about 6 dB at three quarters', () => {
    expect(posToDb(0.75)).toBeCloseTo(-6.25, 1)
  })

  it('round-trips between position, dB and level', () => {
    for (const p of [0.05, 0.2, 0.5, 0.8, 1]) {
      expect(dbToPos(posToDb(p))).toBeCloseTo(p, 6)
      expect(levelToPos(posToLevel(p))).toBeCloseTo(p, 6)
    }
  })

  it('is monotonic', () => {
    let prev = -1
    for (let i = 0; i <= 100; i++) {
      const l = posToLevel(i / 100)
      expect(l).toBeGreaterThanOrEqual(prev)
      prev = l
    }
  })
})

describe('text and meters', () => {
  it('prints dB the way a console does', () => {
    expect(dbText(0)).toBe('0.0')
    expect(dbText(-6.25)).toBe('−6.3')
    expect(dbText(3.5)).toBe('+3.5')
    expect(dbText(-Infinity)).toBe('−∞')
    expect(dbText(-70)).toBe('−∞')
  })

  it('lights none at −54 dB and all at 0 dB', () => {
    expect(ledSegments(0, 28)).toBe(0)
    expect(ledSegments(1, 28)).toBe(28)
    expect(ledSegments(10 ** (-27 / 20), 28)).toBeCloseTo(14, 5)
    expect(ledSegments(4, 28)).toBe(28)
  })
})

describe('equalizer response', () => {
  const peak = { type: 'peak' as const, freq: 1000, gainDb: 12, q: 1 }

  it('reaches the set gain at the centre of a peak and is flat far away', () => {
    expect(bandDb(peak, 1000)).toBeCloseTo(12, 1)
    expect(Math.abs(bandDb(peak, 30))).toBeLessThan(0.3)
    expect(Math.abs(bandDb(peak, 18_000))).toBeLessThan(1)
  })

  it('cuts as well as boosts', () => {
    expect(bandDb({ ...peak, gainDb: -9 }, 1000)).toBeCloseTo(-9, 1)
  })

  it('narrows with Q', () => {
    expect(bandDb({ ...peak, q: 4 }, 1500)).toBeLessThan(bandDb({ ...peak, q: 0.5 }, 1500) - 3)
  })

  it('has a shelf reach its gain far from the corner, and half of it at the corner', () => {
    const low = { type: 'low' as const, freq: 400, gainDb: 9, q: 0.7 }
    expect(bandDb(low, 30)).toBeCloseTo(9, 0)
    expect(Math.abs(bandDb(low, 12_000))).toBeLessThan(0.3)
    expect(bandDb(low, 400)).toBeCloseTo(4.5, 0)
    const high = { type: 'high' as const, freq: 3000, gainDb: -10, q: 0.7 }
    expect(bandDb(high, 18_000)).toBeCloseTo(-10, 0)
    expect(Math.abs(bandDb(high, 100))).toBeLessThan(0.3)
  })

  it('is exactly flat at 0 dB and sums its bands', () => {
    expect(bandDb({ ...peak, gainDb: 0 }, 1000)).toBe(0)
    expect(eqDb([peak, { ...peak, gainDb: -6 }], 1000)).toBeCloseTo(bandDb(peak, 1000) + bandDb({ ...peak, gainDb: -6 }, 1000))
  })
})

describe('compressor curve', () => {
  it('passes what is under the threshold and divides the rise above it', () => {
    expect(compOutDb(-30, -20, 4, 0)).toBe(-30)
    expect(compOutDb(-20, -20, 4, 0)).toBe(-20)
    expect(compOutDb(0, -20, 4, 0)).toBe(-15)
    expect(compOutDb(0, -20, 1, 0)).toBe(0)
  })

  it('adds make-up gain everywhere', () => {
    expect(compOutDb(-40, -20, 4, 6)).toBe(-34)
    expect(compOutDb(0, -20, 4, 6)).toBe(-9)
  })
})
