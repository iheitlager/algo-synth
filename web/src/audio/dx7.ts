// What the DX7 faceplate draws (spec 003 Req 9, spec 006 Req 13): the algorithm
// diagram, from the same routing table the engine uses, and the four-rate,
// four-level envelope. Pure functions, no Vue and no engine. The table below is a
// copy of `crates/dsp/src/fm/algorithms.rs`; a Rust test fails if the two differ.

/** Flags per operator: output bus (1, 2), add (4), input bus (0x10, 0x20), feedback in/out (0x40, 0x80). */
export const ALGORITHMS: readonly (readonly number[])[] = [
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
]

export interface AlgoNode {
  /** The operator as the DX7's panel numbers it, 1..6 (the table's first entry is operator 6). */
  op: number
  /** Row 0 is the carriers at the bottom, a modulator is one row above what it feeds. */
  rank: number
  /** Position in its row, 0.. from the left. */
  col: number
  carrier: boolean
  /** The operator feeds itself (or a loop of operators runs through it). */
  feedback: boolean
}

export interface AlgoLayout {
  nodes: AlgoNode[]
  /** Modulation: operator `from` into operator `to` (panel numbers). */
  links: { from: number; to: number }[]
  rows: number
  /** The widest row. */
  cols: number
}

/**
 * The diagram of algorithm `n` (0..31): who feeds whom, from the routing flags.
 * An operator reads a bus from the operators that wrote it before it, back to the
 * last one that replaced it rather than added to it.
 */
export function algoLayout(n: number): AlgoLayout {
  const flags = ALGORITHMS[Math.min(31, Math.max(0, Math.round(n)))] as readonly number[]
  const writers: number[][] = [[], [], []]
  const feeds: number[][] = flags.map(() => [])
  flags.forEach((f, i) => {
    const inbus = (f >> 4) & 3
    const outbus = f & 3
    if (inbus) for (const w of writers[inbus] as number[]) (feeds[w] as number[]).push(i)
    if (outbus) {
      if (f & 4) (writers[outbus] as number[]).push(i)
      else writers[outbus] = [i]
    }
  })
  // The feedback loop: the operator that has it in, and the one that has it out.
  const rank = flags.map(() => 0)
  for (let i = flags.length - 1; i >= 0; i--) {
    const f = flags[i] as number
    if ((f & 3) !== 0) rank[i] = 1 + Math.max(0, ...(feeds[i] as number[]).map((k) => rank[k] as number))
  }
  const byRank = new Map<number, number[]>()
  flags.forEach((_, i) => byRank.set(rank[i] as number, [...(byRank.get(rank[i] as number) ?? []), i]))
  const nodes: AlgoNode[] = flags.map((f, i) => {
    const row = byRank.get(rank[i] as number) as number[]
    return {
      op: 6 - i,
      rank: rank[i] as number,
      col: row.indexOf(i),
      carrier: (f & 3) === 0,
      feedback: (f & 0xc0) !== 0,
    }
  })
  const links = flags.flatMap((_, from) => (feeds[from] as number[]).map((to) => ({ from: 6 - from, to: 6 - to })))
  const rows = Math.max(...rank) + 1
  const cols = Math.max(...[...byRank.values()].map((r) => r.length))
  return { nodes, links, rows, cols }
}

/** The carriers of algorithm `n`, as panel numbers. */
export const carriersOf = (n: number) => algoLayout(n).nodes.filter((x) => x.carrier).map((x) => x.op).sort()

// --- the four-rate, four-level envelope ---------------------------------------------

/** How long a segment of rate `r` (0..99) takes, in arbitrary units: faster rates are much shorter. */
const segment = (r: number) => 1 / (0.15 + (Math.min(99, Math.max(0, r)) / 99) ** 1.5 * 6)

/**
 * The envelope as points in a `w` × `h` box (level 99 at the top): from level 4 up or down to level 1
 * at rate 1, on to levels 2 and 3, a held stretch (the key is down), then rate 4 back to level 4.
 */
export function egPoints(rates: readonly number[], levels: readonly number[], w: number, h: number, pad = 3): [number, number][] {
  const [r1 = 0, r2 = 0, r3 = 0, r4 = 0] = rates
  const [l1 = 0, l2 = 0, l3 = 0, l4 = 0] = levels
  const widths = [segment(r1), segment(r2), segment(r3), 0.5, segment(r4)]
  const total = widths.reduce((a, b) => a + b, 0)
  const inner = w - 2 * pad
  const y = (l: number) => h - pad - (Math.min(99, Math.max(0, l)) / 99) * (h - 2 * pad)
  const xs: number[] = [pad]
  for (const wd of widths) xs.push((xs[xs.length - 1] as number) + (wd / total) * inner)
  const lv = [l4, l1, l2, l3, l3, l4]
  return xs.map((x, i) => [x, y(lv[i] as number)] as [number, number])
}

export const egPath = (rates: readonly number[], levels: readonly number[], w: number, h: number) =>
  egPoints(rates, levels, w, h)
    .map(([x, y], i) => `${i ? 'L' : 'M'}${x.toFixed(1)} ${y.toFixed(1)}`)
    .join('')
