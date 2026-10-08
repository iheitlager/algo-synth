// A line diff of two song texts (#387), to show what the assistant would
// change before it is applied. Text, not music: the engine still parses the
// result. The longest common subsequence of the lines, after the common head
// and tail are taken off, which is all a song of a few hundred lines needs.

export type DiffOp = ' ' | '+' | '-'
export interface DiffLine { op: DiffOp; text: string }
/** A run of unchanged lines left out of a shown diff. */
export interface DiffSkip { op: '…'; count: number }

const lines = (text: string) => (text === '' ? [] : text.replace(/\r\n?/g, '\n').replace(/\n$/, '').split('\n'))

/** `b` against `a`, line by line: kept, removed and added lines in order. */
export function diffLines(a: string, b: string): DiffLine[] {
  const x = lines(a)
  const y = lines(b)
  let head = 0
  while (head < x.length && head < y.length && x[head] === y[head]) head++
  let tail = 0
  while (tail < x.length - head && tail < y.length - head && x[x.length - 1 - tail] === y[y.length - 1 - tail]) tail++
  const xs = x.slice(head, x.length - tail)
  const ys = y.slice(head, y.length - tail)
  const n = xs.length
  const m = ys.length
  // lcs[i][j]: the common lines of xs[i..] and ys[j..].
  const lcs = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1))
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      lcs[i]![j] = xs[i] === ys[j] ? lcs[i + 1]![j + 1]! + 1 : Math.max(lcs[i + 1]![j]!, lcs[i]![j + 1]!)
    }
  }
  const mid: DiffLine[] = []
  let i = 0
  let j = 0
  while (i < n || j < m) {
    if (i < n && j < m && xs[i] === ys[j]) {
      mid.push({ op: ' ', text: xs[i]! })
      i++
      j++
    } else if (i < n && (j === m || lcs[i + 1]![j]! >= lcs[i]![j + 1]!)) {
      // A removed line before the line that replaces it, as diffs read.
      mid.push({ op: '-', text: xs[i++]! })
    } else {
      mid.push({ op: '+', text: ys[j++]! })
    }
  }
  return [
    ...x.slice(0, head).map((text) => ({ op: ' ' as const, text })),
    ...mid,
    ...x.slice(x.length - tail).map((text) => ({ op: ' ' as const, text })),
  ]
}

/** The diff with unchanged runs longer than twice `context` cut down to `context` lines each side. */
export function collapse(diff: DiffLine[], context = 3): (DiffLine | DiffSkip)[] {
  const out: (DiffLine | DiffSkip)[] = []
  let k = 0
  while (k < diff.length) {
    if (diff[k]!.op !== ' ') {
      out.push(diff[k++]!)
      continue
    }
    let end = k
    while (end < diff.length && diff[end]!.op === ' ') end++
    const keepBefore = k === 0 ? 0 : context
    const keepAfter = end === diff.length ? 0 : context
    if (end - k > keepBefore + keepAfter) {
      out.push(...diff.slice(k, k + keepBefore))
      out.push({ op: '…', count: end - k - keepBefore - keepAfter })
      out.push(...diff.slice(end - keepAfter, end))
    } else {
      out.push(...diff.slice(k, end))
    }
    k = end
  }
  return out
}

/** How many lines a diff adds and removes. */
export const changes = (diff: DiffLine[]) => ({
  added: diff.filter((d) => d.op === '+').length,
  removed: diff.filter((d) => d.op === '-').length,
})
