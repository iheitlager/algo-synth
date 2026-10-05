import { describe, expect, it } from 'vitest'
import {
  INSERT_KNOBS, INSERT_SHORT, STRIPS, feedsTag, heardStrips, moveBefore, namedOuts, orderStrips, outChoices, outGroup, routeOk, PROC_KNOBS, SWEEP, arcPath, exp, hzText, lin, bandDb, compOutDb, dbText, dbToPos, dragValue, eqDb, knobAngle, knobArc, ledSegments, levelToPos,
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

describe('parameter scales', () => {
  it('maps a linear range and back', () => {
    const s = lin(-15, 15)
    expect(s.toValue(0.5)).toBe(0)
    expect(s.toValue(2)).toBe(15)
    expect(s.toPos(7.5)).toBeCloseTo(0.75)
    expect(s.toPos(-40)).toBe(0)
  })

  it('maps an exponential range and back', () => {
    const s = exp(10, 1000)
    expect(s.toValue(0.5)).toBeCloseTo(100)
    expect(s.toPos(100)).toBeCloseTo(0.5)
  })

  it('prints frequencies', () => {
    expect(hzText(440)).toBe('440 Hz')
    expect(hzText(2500)).toBe('2.5 k')
  })
})

describe('processor knobs', () => {
  it('has the knobs of each type, with echo time in milliseconds', () => {
    expect(PROC_KNOBS[0]).toEqual([])
    expect(PROC_KNOBS[1]?.map((k) => k.label)).toEqual(['Time', 'Fdbk', 'Tone', 'Ping-pong'])
    expect(PROC_KNOBS[2]?.map((k) => k.label)).toEqual(['Size', 'Damp', 'Pre'])
    expect(PROC_KNOBS[1]?.[0]?.text?.(0.5)).toBe('45 ms')
    expect(PROC_KNOBS[1]?.[1]?.text?.(1)).toBe('95%')
    expect(PROC_KNOBS[2]?.[0]?.text?.(1)).toBe('10.0 s')
    expect(PROC_KNOBS[1]?.[3]?.toggle).toBe(true)
  })

  it('has chorus and flanger with their real units', () => {
    expect(PROC_KNOBS[3]?.map((k) => k.label)).toEqual(['Rate', 'Depth', 'Delay', 'Spread', 'Tone'])
    expect(PROC_KNOBS[4]?.map((k) => k.label)).toEqual(['Rate', 'Depth', 'Manual', 'Fdbk', 'Tone'])
    expect(PROC_KNOBS[3]?.[0]?.text?.(1)).toBe('8.00 Hz')
    expect(PROC_KNOBS[3]?.[2]?.text?.(0)).toBe('5.0 ms')
    expect(PROC_KNOBS[3]?.[3]?.text?.(1)).toBe('180°')
    expect(PROC_KNOBS[4]?.[0]?.text?.(0)).toBe('0.05 Hz')
    expect(PROC_KNOBS[4]?.[2]?.text?.(1)).toBe('10.0 ms')
    // Feedback is signed: the middle is none, the ends are ±95%.
    expect(PROC_KNOBS[4]?.[3]?.text?.(0.5)).toBe('0%')
    expect(PROC_KNOBS[4]?.[3]?.text?.(1)).toBe('95%')
    expect(PROC_KNOBS[4]?.[3]?.text?.(0)).toBe('-95%')
  })
})

describe('insert knobs', () => {
  it('has the knobs of each type and a short name for each', () => {
    expect(INSERT_SHORT).toHaveLength(7)
    expect(INSERT_KNOBS[0]).toEqual([])
    for (const t of [1, 2, 3]) expect(INSERT_KNOBS[t]?.map((k) => k.label)).toEqual(['Amount', 'Tone', 'Level'])
    expect(INSERT_KNOBS[4]?.map((k) => k.label)).toEqual(['Low', 'Mid Hz', 'Mid', 'High', 'Mid Q'])
    expect(INSERT_KNOBS[5]?.map((k) => k.label)).toEqual(['Thresh', 'Ratio', 'Attack', 'Release', 'Make-up'])
    expect(INSERT_KNOBS[6]?.map((k) => k.label)).toEqual(['Shift', 'Release', 'Unvoiced', 'Width', 'Dry'])
    expect(INSERT_KNOBS[6]?.[0]?.text?.(0.5)).toBe('0 st')
    expect(INSERT_KNOBS[6]?.[0]?.text?.(1)).toBe('12 st')
  })

  it('reads the defaults as neutral', () => {
    const eq = INSERT_KNOBS[4] ?? []
    expect(eq[0]?.text?.(eq[0].def)).toBe('0.0 dB')
    expect(eq[2]?.text?.(eq[2].def)).toBe('0.0 dB')
    expect(eq[4]?.text?.(eq[4].def)).toBe('1.0')
    const comp = INSERT_KNOBS[5] ?? []
    expect(comp[4]?.text?.(comp[4].def)).toBe('+0.0 dB')
    expect(comp[0]?.text?.(1)).toBe('0 dB')
    expect(comp[1]?.text?.(0)).toBe('1.0:1')
  })

  it('shows the drive in dB and the tone in Hz', () => {
    const [amount, tone] = INSERT_KNOBS[1] ?? []
    expect(amount?.text?.(1)).toBe('+40 dB')
    expect(tone?.text?.(0)).toBe('200 Hz')
    expect(tone?.text?.(1)).toBe('20.0 k')
  })
})

describe('routing', () => {
  it('lets a synth go anywhere and a group only up', () => {
    expect(routeOk(0, 0)).toBe(true)
    expect(routeOk(5, 8)).toBe(true)
    expect(routeOk(5, 9)).toBe(true) // nowhere (#161)
    expect(routeOk(23, 9)).toBe(true)
    expect(routeOk(5, 10)).toBe(false)
    expect(routeOk(16, 1)).toBe(false) // group 1 to itself
    expect(routeOk(17, 1)).toBe(false) // group 2 to group 1
    expect(routeOk(16, 2)).toBe(true)
    expect(routeOk(23, 0)).toBe(true)
    expect(routeOk(23, 8)).toBe(false)
  })

  it('offers the master and the groups a strip can reach', () => {
    expect(outChoices(0, [0, 2]).map((c) => c.label)).toEqual(['Master', 'Group 1', 'Group 3', 'None'])
    expect(outChoices(17, [0, 1, 2]).map((c) => c.label)).toEqual(['Master', 'Group 3', 'None'])
  })

  // #217: None is out 9, one past the eight groups, and was named as the group after the last.
  it('names only the groups, not Master or None', () => {
    expect(outGroup(0)).toBe(-1)
    expect(outGroup(1)).toBe(0)
    expect(outGroup(8)).toBe(7)
    expect(outGroup(9)).toBe(-1)
    const name = (s: number) => (s === 16 ? 'Drums' : `G${s - 15}`)
    expect(namedOuts(0, [0, 2], name).map((c) => c.label)).toEqual(['Master', 'Drums', 'G3', 'None'])
    expect(namedOuts(0, [0], name).every((c) => c.label !== 'Group 9' && c.label !== 'G9')).toBe(true)
  })

  it('tags the group a strip feeds, and nothing for Master or None', () => {
    const name = (s: number) => `G${s - 15}`
    expect(feedsTag(0, name)).toBeUndefined()
    expect(feedsTag(9, name)).toBeUndefined()
    expect(feedsTag(3, name)?.label).toBe('G3')
    expect(feedsTag(8, name)?.label).toBe('G8')
  })
})

describe('heardStrips', () => {
  const mk = (over: Partial<Record<number, Partial<{ mute: boolean; solo: boolean; out: number }>>> = {}) =>
    Array.from({ length: STRIPS }, (_, i) => ({ mute: false, solo: false, out: 0, ...over[i] }))

  it("keeps the groups a soloed kit's individual outs feed (#162)", () => {
    const strips = mk({ 0: { solo: true } }).map((s, i) => (i === 0 ? { ...s, feeds: [2] } : s))
    const heard = heardStrips(strips)
    expect(heard[0]).toBe(true)
    expect(heard[18]).toBe(true)
    expect(heard[17]).toBe(false)
    expect(heard[1]).toBe(false)
  })

  it('hears everything with no solo, except what is muted', () => {
    expect(heardStrips(mk({ 3: { mute: true } })).filter((h) => !h)).toHaveLength(1)
    expect(heardStrips(mk())[3]).toBe(true)
  })

  it('hears a soloed strip through its groups and nothing else', () => {
    const h = heardStrips(mk({ 0: { out: 1, solo: true }, 16: { out: 2 } }))
    expect([h[0], h[1], h[16], h[17], h[18]]).toEqual([true, false, true, true, false])
  })

  it('hears a soloed group with what feeds it', () => {
    const h = heardStrips(mk({ 0: { out: 1 }, 16: { solo: true } }))
    expect([h[0], h[1], h[16]]).toEqual([true, false, true])
  })

  it('still mutes a soloed strip that is muted', () => {
    expect(heardStrips(mk({ 0: { solo: true, mute: true } }))[0]).toBe(false)
  })
})

describe('console order', () => {
  it('keeps the saved order of what is shown and appends the rest, synths first', () => {
    expect(orderStrips([5, 2, 16, 9], [0, 2, 5, 16, 17])).toEqual([5, 2, 16, 0, 17])
    expect(orderStrips([], [17, 3, 0, 16])).toEqual([0, 3, 16, 17])
    expect(orderStrips([2, 2, 1], [1, 2])).toEqual([2, 1])
  })

  it('moves a strip before another, or to the end', () => {
    expect(moveBefore([0, 1, 2, 3], 3, 1)).toEqual([0, 3, 1, 2])
    expect(moveBefore([0, 1, 2, 3], 0, -1)).toEqual([1, 2, 3, 0])
    expect(moveBefore([0, 1, 2], 1, 9)).toEqual([0, 1, 2])
    expect(moveBefore([0, 1, 2], 7, 1)).toEqual([0, 1, 2])
  })
})
