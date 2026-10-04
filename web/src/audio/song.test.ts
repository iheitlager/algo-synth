import { describe, expect, it } from 'vitest'
import { applySong, song } from './engine'

const bytes = (s: string) => new TextEncoder().encode(s)

describe('the song the worklet sends (spec 003 Req 5)', () => {
  const text = 'tempo 120\nswing 50\ntrack kit drums\n\nfrag beat = kit /16\n  bd x.X.\n'
  const summary = {
    ok: true,
    text: bytes(text),
    error: null,
    tracks: [{ name: bytes('kit'), synth: 2 }],
    frags: [{ name: bytes('beat'), track: 0, lanes: [{ pad: 0, steps: new Uint8Array([1, 0, 2, 0]) }] }],
    tempo: 120,
    swing: 50,
  }

  it('decodes the text, the tracks and the grid, and takes the text as the draft', () => {
    song.draft = 'old'
    applySong(summary)
    expect(song.text).toBe(text)
    expect(song.draft).toBe(text)
    expect(song.tracks).toEqual([{ name: 'kit', synth: 2 }])
    expect(song.frags).toEqual([{ name: 'beat', track: 0, lanes: [{ pad: 0, steps: [1, 0, 2, 0] }] }])
    expect(song.error).toBeNull()
  })

  it('keeps the draft and shows where a bad text failed', () => {
    song.draft = 'loop'
    applySong({ ...summary, ok: false, error: { line: 1, col: 1, msg: bytes('a line starts with tempo, swing, track or frag') } })
    expect(song.draft).toBe('loop')
    expect(song.error).toEqual({ line: 1, col: 1, msg: 'a line starts with tempo, swing, track or frag' })
    expect(song.frags).toHaveLength(1)
  })
})
