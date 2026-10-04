import { describe, expect, it } from 'vitest'
import { paint, type Span } from './lex'

const span = (start: number, len: number, cls: string): Span => ({ start, len, cls })

describe('painting the song text (#203)', () => {
  it('cuts the text into lines of classed spans and plain gaps', () => {
    const text = 'tempo 120\n  bd x...'
    const lines = paint(text, [span(0, 5, 'kw'), span(6, 3, 'num'), span(12, 2, 'pad'), span(15, 1, 'step'), span(16, 3, 'rest')])
    expect(lines).toEqual([
      [{ text: 'tempo', cls: 'kw' }, { text: ' ', cls: '' }, { text: '120', cls: 'num' }],
      [{ text: '  ', cls: '' }, { text: 'bd', cls: 'pad' }, { text: ' ', cls: '' }, { text: 'x', cls: 'step' }, { text: '...', cls: 'rest' }],
    ])
  })

  it('keeps every line, empty ones and a trailing newline included', () => {
    expect(paint('a\n\nb\n', [])).toEqual([[{ text: 'a', cls: '' }], [], [{ text: 'b', cls: '' }], []])
    expect(paint('', [])).toEqual([[]])
  })

  it('shows the text plain where spans are out of order or out of bounds', () => {
    const text = 'frag a = k'
    const lines = paint(text, [span(5, 1, 'name'), span(0, 4, 'kw'), span(9, 5, 'name')])
    expect(lines.flat().map((p) => p.text).join('')).toBe(text)
    expect(lines[0].find((p) => p.cls === 'name')?.text).toBe('a')
  })
})
