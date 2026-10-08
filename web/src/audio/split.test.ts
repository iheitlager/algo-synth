import { describe, expect, it } from 'vitest'
import { LIMITS, SPLIT_KEY, STEP, clampSize, dragged, keepSplits, lastSplits, stepped } from './split'

const memory = () => {
  const items = new Map<string, string>()
  const store = { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v) }
  return () => store
}
const broken = () => {
  throw new Error('SecurityError')
}

describe('the composer dividers (#373)', () => {
  it('keep a pane between its least size and its share of the room', () => {
    expect(clampSize(400, LIMITS.code, 1000)).toBe(400)
    expect(clampSize(100, LIMITS.code, 1000)).toBe(LIMITS.code)
    expect(clampSize(900, LIMITS.code, 1000)).toBe(700)
    // Too little room for both: the pane keeps its least size.
    expect(clampSize(500, LIMITS.arranger, 100)).toBe(LIMITS.arranger)
  })

  it('grow the pane after the divider as it moves back', () => {
    expect(dragged(300, 800, 750)).toBe(350)
    expect(dragged(300, 800, 860)).toBe(240)
  })

  it('move by a step on the arrow keys, and ignore other keys', () => {
    expect(stepped(300, 'ArrowLeft')).toBe(300 + STEP)
    expect(stepped(300, 'ArrowUp')).toBe(300 + STEP)
    expect(stepped(300, 'ArrowRight')).toBe(300 - STEP)
    expect(stepped(300, 'ArrowDown')).toBe(300 - STEP)
    expect(stepped(300, 'Enter')).toBeNull()
  })

  it('are kept in the browser, and read back as the defaults when damaged or missing', () => {
    const store = memory()
    expect(lastSplits(store)).toEqual({ code: null, arranger: null, assistant: null })
    keepSplits({ code: 420, arranger: 260, assistant: 380 }, store)
    expect(lastSplits(store)).toEqual({ code: 420, arranger: 260, assistant: 380 })
    store().setItem(SPLIT_KEY, JSON.stringify({ code: 'wide', arranger: -3 }))
    expect(lastSplits(store)).toEqual({ code: null, arranger: null, assistant: null })
    store().setItem(SPLIT_KEY, '{oops')
    expect(lastSplits(store)).toEqual({ code: null, arranger: null, assistant: null })
  })

  it('survive storage that is unavailable', () => {
    expect(lastSplits(broken)).toEqual({ code: null, arranger: null, assistant: null })
    expect(() => keepSplits({ code: 1, arranger: 1, assistant: 1 }, broken)).not.toThrow()
  })
})
