import { describe, expect, it } from 'vitest'
import { caretAt, opens, readCompletion, type Completion } from './complete'

describe('reading the engine\'s completion (#595)', () => {
  it('reads the range and the items with their classes', () => {
    const c = readCompletion('8\t11\t\n1\tscale\tscale <root> <mode>\n9\tCutoff\t20–20000\n')
    expect(c).toEqual({
      from: 8,
      to: 11,
      help: '',
      items: [
        { cls: 'kw', word: 'scale', detail: 'scale <root> <mode>' },
        { cls: 'param', word: 'Cutoff', detail: '20–20000' },
      ],
    })
  })

  it('reads a call\'s help (#644)', () => {
    expect(readCompletion('4\t4\tprog takes a number of bars and a seed: prog(4,7)\n').help)
      .toBe('prog takes a number of bars and a seed: prog(4,7)')
  })

  it('reads nothing from an empty or broken answer', () => {
    expect(readCompletion('')).toEqual({ from: 0, to: 0, items: [], help: '' })
    expect(readCompletion('4\t4\n')).toEqual({ from: 4, to: 4, items: [], help: '' })
    expect(readCompletion('9\t2\n1\tx\t')).toEqual({ from: 0, to: 0, items: [], help: '' })
  })
})

describe('when the list opens (#595)', () => {
  const item = (word: string) => ({ cls: 'kw', word, detail: '' })
  const at = (text: string, words: string[]): Completion => {
    const n = text.length - (text.match(/[\w-]*$/)?.[0].length ?? 0)
    return { from: n, to: text.length, items: words.map(item), help: '' }
  }
  const shows = (c: Completion | null) => (c ? c.items.map((i) => i.word) : null)

  it('opens on a word being typed, and after a trigger', () => {
    expect(shows(opens(at('sc', ['scale', 'scene']), 'sc', 'c'))).toEqual(['scale', 'scene'])
    expect(shows(opens(at('clip c = lead .', ['fast']), 'clip c = lead .', '.'))).toEqual(['fast'])
    expect(shows(opens(at('arp(', ['riff']), 'arp(', '('))).toEqual(['riff'])
  })

  it('not after a space, so Enter still applies the line (#588)', () => {
    expect(opens(at('track a ', ['drums']), 'track a ', ' ')).toBeNull()
  })

  it('not once a word is typed in full', () => {
    expect(opens(at('scale c minor', ['minor', 'minor-blues']), 'scale c minor', 'r')).toBeNull()
  })

  it('not when nothing fits, and always on Ctrl+Space when something does', () => {
    expect(opens(at('x', []), 'x', 'x')).toBeNull()
    expect(shows(opens(at('track a ', ['drums']), 'track a ', null))).toEqual(['drums'])
  })
})

describe('a call\'s help (#644)', () => {
  const c = (items: string[]): Completion => ({ from: 7, to: 7, items: items.map((word) => ({ cls: '', word, detail: '' })), help: 'prog takes…' })

  it('shows alone, without the list, when no word is offered', () => {
    expect(opens(c([]), 'prog(4,', ',')).toEqual({ ...c([]), items: [] })
    expect(opens(c([]), 'prog(4, ', ' ')).toEqual({ ...c([]), items: [] })
  })

  it('shows over the list when one opens', () => {
    expect(opens(c(['riff']), 'mutate(', '(')).toEqual(c(['riff']))
  })
})

describe('where the caret is', () => {
  it('counts lines and columns from 0', () => {
    expect(caretAt('tempo 1\nscale c', 15)).toEqual({ line: 1, col: 7 })
    expect(caretAt('', 0)).toEqual({ line: 0, col: 0 })
  })

  it('takes a tab to the next stop', () => {
    expect(caretAt('clip a = b\n\tc4', 14)).toEqual({ line: 1, col: 4 })
    expect(caretAt('a\tb', 2, 4)).toEqual({ line: 0, col: 4 })
  })
})
