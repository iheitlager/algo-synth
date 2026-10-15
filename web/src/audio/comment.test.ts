import { describe, expect, it } from 'vitest'
import { toggleComment } from './comment'

// The text after the toggle, with the selection marked as [ and ].
const toggled = (text: string, from: number, to = from, mark = '#') => {
  const t = toggleComment(text, from, to, mark)
  const out = text.slice(0, t.start) + t.text + text.slice(t.end)
  return out.slice(0, t.to).slice(0, t.from) + '[' + out.slice(t.from, t.to) + ']' + out.slice(t.to)
}

describe('Ctrl+/ comments lines (#583)', () => {
  const song = 'tempo 120\nclip beat = kit\n  bd x...\n  sn ..x.\n'

  it('comments the caret\'s line at its indent, and back', () => {
    const at = song.indexOf('bd') + 3
    expect(toggled(song, at)).toBe('tempo 120\nclip beat = kit\n  # bd []x...\n  sn ..x.\n')
    const t = toggleComment(song, at, at)
    const once = song.slice(0, t.start) + t.text + song.slice(t.end)
    expect(toggled(once, t.from)).toBe('tempo 120\nclip beat = kit\n  bd []x...\n  sn ..x.\n')
  })

  it('comments every selected line at the least indent, skipping blank ones', () => {
    const text = 'clip beat = kit\n\n  bd x...\n'
    expect(toggled(text, 0, text.length)).toBe('[# clip beat = kit\n\n#   bd x...\n]')
  })

  it('leaves out the line a selection ends at the start of', () => {
    const from = song.indexOf('  bd')
    const to = song.indexOf('  sn')
    expect(toggled(song, from, to)).toBe('tempo 120\nclip beat = kit\n[  # bd x...\n]  sn ..x.\n')
  })

  it('uncomments only when every line is commented; a mixed block is commented', () => {
    const mixed = '# tempo 120\nswing 50'
    expect(toggled(mixed, 0, mixed.length)).toBe('[# # tempo 120\n# swing 50]')
    const all = '# tempo 120\n  #swing 50'
    expect(toggled(all, 0, all.length)).toBe('[tempo 120\n  swing 50]')
  })

  it('takes another mark, as SuperCollider\'s //', () => {
    expect(toggled('SinOsc.ar(440)', 0, 0, '//')).toBe('// []SinOsc.ar(440)')
  })
})
