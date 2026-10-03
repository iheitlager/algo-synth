import { describe, expect, it } from 'vitest'
import { ALGORITHMS, algoLayout, carriersOf, egPath, egPoints } from './dx7'

describe('algorithm diagrams', () => {
  it('has the 32 algorithms of six operators', () => {
    expect(ALGORITHMS).toHaveLength(32)
    for (const a of ALGORITHMS) expect(a).toHaveLength(6)
  })

  it('draws every operator once, with a carrier at the bottom', () => {
    for (let n = 0; n < 32; n++) {
      const l = algoLayout(n)
      expect(l.nodes.map((x) => x.op).sort(), `algorithm ${n + 1}`).toEqual([1, 2, 3, 4, 5, 6])
      expect(l.nodes.some((x) => x.carrier && x.rank === 0), `algorithm ${n + 1}`).toBe(true)
      // A carrier is on the bottom row; a modulator is above every operator it feeds.
      for (const x of l.nodes) expect(x.carrier).toBe(x.rank === 0)
      for (const k of l.links) {
        const from = l.nodes.find((x) => x.op === k.from)!
        const to = l.nodes.find((x) => x.op === k.to)!
        expect(from.rank, `algorithm ${n + 1}: ${k.from} into ${k.to}`).toBeGreaterThan(to.rank)
      }
    }
  })

  it('knows the carriers of the well-known algorithms', () => {
    expect(carriersOf(0)).toEqual([1, 3])
    expect(carriersOf(4)).toEqual([1, 3, 5])
    expect(carriersOf(31)).toEqual([1, 2, 3, 4, 5, 6])
  })

  it('draws algorithm 1 as two stacks: 6 into 5 into 4 into 3, and 2 into 1', () => {
    const l = algoLayout(0)
    const links = l.links.map((k) => `${k.from}>${k.to}`).sort()
    expect(links).toEqual(['2>1', '4>3', '5>4', '6>5'])
    expect(l.nodes.find((x) => x.op === 6)?.feedback).toBe(true)
    expect(l.rows).toBe(4)
  })

  it('draws a carrier fed by several modulators once per feed', () => {
    // Algorithm 22: operator 6 feeds 5, 4 and 3; operator 2 feeds 1.
    const links = algoLayout(21).links.map((k) => `${k.from}>${k.to}`).sort()
    expect(links).toEqual(['2>1', '6>3', '6>4', '6>5'])
  })

  it('clamps a bad algorithm number', () => {
    expect(algoLayout(99).nodes).toHaveLength(6)
    expect(algoLayout(-3).nodes).toHaveLength(6)
  })
})

describe('the four-rate envelope', () => {
  it('goes from level 4 up to level 1, through 2 and 3, holds, and returns to level 4', () => {
    const pts = egPoints([99, 50, 50, 50], [99, 70, 40, 0], 120, 44)
    expect(pts).toHaveLength(6)
    const [start, l1, l2, l3, hold, end] = pts as [number, number][]
    expect(start[1]).toBeCloseTo(41, 0) // level 4 is 0: the bottom
    expect(l1[1]).toBeCloseTo(3, 0) // level 1 is 99: the top
    expect(l2[1]).toBeGreaterThan(l1[1])
    expect(l3[1]).toBeGreaterThan(l2[1])
    expect(hold[1]).toBe(l3[1])
    expect(end[1]).toBe(start[1])
    for (let i = 1; i < pts.length; i++) expect((pts[i] as number[])[0]).toBeGreaterThan((pts[i - 1] as number[])[0] as number)
    expect(end[0]).toBeCloseTo(117, 0)
  })

  it('draws a slower rate as a longer segment', () => {
    const fast = egPoints([99, 50, 50, 50], [99, 70, 40, 0], 120, 44)
    const slow = egPoints([20, 50, 50, 50], [99, 70, 40, 0], 120, 44)
    expect((slow[1] as number[])[0]).toBeGreaterThan((fast[1] as number[])[0] as number)
  })

  it('is an SVG path, and survives out-of-range values', () => {
    const p = egPath([150, -5, 50, 50], [120, -3, 40, 0], 120, 44)
    expect(p.startsWith('M')).toBe(true)
    expect(p).not.toContain('NaN')
  })
})
