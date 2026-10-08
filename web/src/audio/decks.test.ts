import { describe, expect, it } from 'vitest'
import { decks, loadDeck, onDecks, setCrossfade, setDeck } from './decks'
import { DeckSide } from './params'

describe('decks', () => {
  it('start with deck A as the song and B–D empty, all at full level, none on the crossfader', () => {
    expect(decks.list.map((d) => d.name)).toEqual(['This song', '', '', ''])
    expect(decks.list.every((d) => d.level === 1 && d.side === DeckSide.Thru)).toBe(true)
  })

  it('take the worklet\'s peaks and dropped blocks', () => {
    onDecks([0.5, 0.25, 0, 0], [0, 3, 0, 0])
    expect(decks.list.map((d) => d.peak)).toEqual([0.5, 0.25, 0, 0])
    expect(decks.list[1]?.dropped).toBe(3)
  })

  it('keep a level, a side and the crossfader without audio on', () => {
    setDeck(2, 'level', 0.4)
    setDeck(2, 'side', DeckSide.Right)
    setCrossfade(0.7)
    expect(decks.list[2]).toMatchObject({ level: 0.4, side: DeckSide.Right })
    expect(decks.crossfade).toBe(0.7)
  })

  it('say why a song can\'t load before audio is on or without isolation', async () => {
    await loadDeck(1, new File(['tempo 120\n'], 'x.song'))
    expect(decks.list[1]?.error).toBe(decks.isolated ? 'Power on first.' : 'Decks need a cross-origin isolated page.')
    expect(decks.list[1]?.loaded).toBe(false)
  })
})
