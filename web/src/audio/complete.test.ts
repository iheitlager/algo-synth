import { describe, expect, it } from 'vitest'
import { caretAt, opens, readCompletion, type Completion } from './complete'

describe('reading the engine\'s completion (#595)', () => {
  it('reads the range and the items with their classes', () => {
    const c = readCompletion('8\t11\n1\tscale\tscale <root> <mode>\n9\tCutoff\t20–20000\n')
    expect(c).toEqual({
      from: 8,
      to: 11,
      items: [
        { cls: 'kw', word: 'scale', detail: 'scale <root> <mode>' },
        { cls: 'param', word: 'Cutoff', detail: '20–20000' },
      ],
    })
  })

  it('reads nothing from an empty or broken answer', () => {
    expect(readCompletion('')).toEqual({ from: 0, to: 0, items: [] })
    expect(readCompletion('4\t4\n')).toEqual({ from: 4, to: 4, items: [] })
    expect(readCompletion('9\t2\n1\tx\t')).toEqual({ from: 0, to: 0, items: [] })
  })
})

describe('when the list opens (#595)', () => {
  const item = (word: string) => ({ cls: 'kw', word, detail: '' })
  const at = (text: string, words: string[]): Completion => {
    const n = text.length - (text.match(/[\w-]*$/)?.[0].length ?? 0)
    return { from: n, to: text.length, items: words.map(item) }
  }

  it('opens on a word being typed, and after a trigger', () => {
    expect(opens(at('sc', ['scale', 'scene']), 'sc', 'c')).toBe(true)
    expect(opens(at('clip c = lead .', ['fast']), 'clip c = lead .', '.')).toBe(true)
    expect(opens(at('arp(', ['riff']), 'arp(', '(')).toBe(true)
  })

  it('not after a space, so Enter still applies the line (#588)', () => {
    expect(opens(at('track a ', ['drums']), 'track a ', ' ')).toBe(false)
  })

  it('not once a word is typed in full', () => {
    expect(opens(at('scale c minor', ['minor', 'minor-blues']), 'scale c minor', 'r')).toBe(false)
  })

  it('not when nothing fits, and always on Ctrl+Space when something does', () => {
    expect(opens(at('x', []), 'x', 'x')).toBe(false)
    expect(opens(at('track a ', ['drums']), 'track a ', null)).toBe(true)
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
