import { describe, expect, it } from 'vitest'
import { keepCaret } from './caret'

describe('keepCaret (#458)', () => {
  const typed = 'tempo 120\ntrack kit drums\ntrack lead synth\nfrag a = kit\n'
  const printed = 'tempo 120\nswing 50\ntrack kit drums Tr808 Kit808\ntrack lead synth ProOne ProLead\nfrag a = kit /16\n'

  it('keeps a caret before the change, and one after it counted from the end', () => {
    expect(keepCaret('abcXdef', 'abcYYdef', 2)).toBe(2)
    expect(keepCaret('abcXdef', 'abcYYdef', 5)).toBe(6)
    expect(keepCaret('abcXdef', 'abcYYdef', 7)).toBe(8)
  })

  it('puts a caret inside the change at its end', () => {
    expect(keepCaret('abcXdef', 'abcYYdef', 4)).toBe(5)
  })

  it('leaves a caret where the text is the same', () => {
    expect(keepCaret(typed, typed, 20)).toBe(20)
  })

  it('a caret at the end of a typed line stays near it when the engine prints the song', () => {
    const at = typed.indexOf('track lead synth') + 'track lead synth'.length
    const kept = keepCaret(typed, printed, at)
    const line = printed.slice(0, kept).split('\n').length
    expect(line).toBeLessThanOrEqual(5)
    expect(kept).toBeLessThan(printed.length)
  })
})
