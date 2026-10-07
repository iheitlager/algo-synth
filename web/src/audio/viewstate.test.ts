import { describe, expect, it } from 'vitest'
import { VIEW_KEY, keepView, lastView } from './viewstate'

const memory = () => {
  const items = new Map<string, string>()
  const store = { getItem: (k: string) => items.get(k) ?? null, setItem: (k: string, v: string) => void items.set(k, v) }
  return () => store
}
const broken = () => {
  throw new Error('SecurityError')
}

describe('the view kept in the browser (ADR-0027)', () => {
  it('gives back the groups, layout and typed names it kept', () => {
    const store = memory()
    expect(lastView(store)).toBeNull()
    const view = { groups: [16, 18], order: [2, 16, 0], collapsed: [16], hidden: [5], names: { 2: 'Lead' } }
    keepView(view, store)
    expect(lastView(store)).toEqual(view)
  })

  it('reads a damaged entry as none, and drops what is not a number or a name', () => {
    const store = memory()
    store().setItem(VIEW_KEY, '{oops')
    expect(lastView(store)).toBeNull()
    store().setItem(VIEW_KEY, JSON.stringify({ groups: [16, 'x'], order: 3, names: { 1: 'a', b: 'c', 2: 4 } }))
    expect(lastView(store)).toEqual({ groups: [16], order: [], collapsed: [], hidden: [], names: { 1: 'a' } })
  })

  it('survives storage that is unavailable', () => {
    expect(() => keepView({ groups: [], order: [], collapsed: [], hidden: [], names: {} }, broken)).not.toThrow()
    expect(lastView(broken)).toBeNull()
  })
})
