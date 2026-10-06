import { describe, expect, it } from 'vitest'

// Vue scopes only a selector's last part, so `.composer .x` in a component also
// matches through any ancestor with that class, and App's layout div carries
// the view's name. That hid the composer's current step whenever it was open.
const LAYOUT = ['composer', 'mixer', 'sound']

const sources = import.meta.glob<string>('./**/*.vue', { query: '?raw', import: 'default', eager: true })

describe('component styles', () => {
  it('never start a descendant selector with a class the layout carries', () => {
    const bad: string[] = []
    for (const [file, text] of Object.entries(sources)) {
      const css = /<style scoped>([\s\S]*?)<\/style>/.exec(text)?.[1] ?? ''
      for (const rule of css.split('}')) {
        for (const selector of (rule.split('{')[0] ?? '').split(',').map((x) => x.trim())) {
          const [first, ...rest] = selector.split(/\s+/)
          if (rest.length && LAYOUT.some((c) => new RegExp(`\\.${c}(?![\\w-])`).test(first ?? ''))) bad.push(`${file}: ${selector}`)
        }
      }
    }
    expect(Object.keys(sources).length).toBeGreaterThan(10)
    expect(bad).toEqual([])
  })
})
