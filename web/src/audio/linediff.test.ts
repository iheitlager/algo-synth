import { describe, expect, it } from 'vitest'
import { changes, collapse, diffLines, type DiffLine } from './linediff'

/** A diff as `op text` lines, to read in a test. */
const show = (d: DiffLine[]) => d.map((x) => `${x.op}${x.text}`)

describe('the line diff', () => {
  it('keeps equal texts unchanged', () => {
    expect(show(diffLines('a\nb\n', 'a\nb'))).toEqual([' a', ' b'])
    expect(diffLines('', '')).toEqual([])
  })

  it('shows a changed line as removed, then added, between kept lines', () => {
    expect(show(diffLines('tempo 120\nswing 50\ntrack kit drums', 'tempo 128\nswing 50\ntrack kit drums')))
      .toEqual(['-tempo 120', '+tempo 128', ' swing 50', ' track kit drums'])
  })

  it('finds lines inserted and removed in the middle', () => {
    const a = 'clip beat = kit /16\n  bd x...x...\n  sn ....x...\n  ch x.x.x.x.'
    const b = 'clip beat = kit /16\n  bd x...x...\n  cp ....x...\n  sn ....x...'
    expect(show(diffLines(a, b))).toEqual([' clip beat = kit /16', '   bd x...x...', '+  cp ....x...', '   sn ....x...', '-  ch x.x.x.x.'])
    expect(changes(diffLines(a, b))).toEqual({ added: 1, removed: 1 })
  })

  it('handles a song from nothing, and to nothing', () => {
    expect(show(diffLines('', 'tempo 90\nswing 50'))).toEqual(['+tempo 90', '+swing 50'])
    expect(show(diffLines('tempo 90', ''))).toEqual(['-tempo 90'])
  })

  it('cuts long unchanged runs down to their context', () => {
    const a = Array.from({ length: 20 }, (_, i) => `l${i}`).join('\n')
    const b = a.replace('l10', 'L10')
    const shown = collapse(diffLines(a, b), 2)
    expect(shown.map((x) => (x.op === '…' ? `…${x.count}` : `${x.op}${x.text}`)))
      .toEqual(['…8', ' l8', ' l9', '-l10', '+L10', ' l11', ' l12', '…7'])
  })
})
