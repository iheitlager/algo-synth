import { describe, expect, it } from 'vitest'
import { inline, parseMarkdown } from './markdown'

// An Assistant answer as it came (#459).
const ANSWER = `**1. Classic notation (simplest).** \`:2.\` is a dotted half (36 ticks), so four of them fill exactly three bars:

\`\`\`
clip teng = bass
  c#2:2. d2:2. d2:2. d2:2.
\`\`\`

**2. Mini-notation slowed down.** Write the four notes as one even bar, then stretch it by 3:

\`\`\`song
clip teng = bass .slow(3)
  "c#2 d2 d2 d2"
\`\`\`

**Two things to watch:**
- **Scene lengths:** a 3-bar clip loops inside each scene. Your \`riddim\` (8 bars) isn't a multiple of 3,
  so the riff will be cut off mid-cycle.
- **The g2 → f#2 ending:** add it on as a fourth bar, for example \`r:2 g2:4 f#2:4\`.`

describe('markdown (#459)', () => {
  it('reads an answer into paragraphs, code blocks and a list', () => {
    const blocks = parseMarkdown(ANSWER)
    expect(blocks.map((b) => b.t)).toEqual(['p', 'code', 'p', 'code', 'p', 'ul'])
    const [first, code, , song, , list] = blocks
    expect(first).toEqual({
      t: 'p',
      kids: [
        { t: 'b', kids: [{ t: 'text', text: '1. Classic notation (simplest).' }] },
        { t: 'text', text: ' ' },
        { t: 'code', text: ':2.' },
        { t: 'text', text: ' is a dotted half (36 ticks), so four of them fill exactly three bars:' },
      ],
    })
    expect(code).toEqual({ t: 'code', lang: '', text: 'clip teng = bass\n  c#2:2. d2:2. d2:2. d2:2.' })
    expect(song).toMatchObject({ t: 'code', lang: 'song' })
    if (list.t !== 'ul') throw new Error('a list')
    expect(list.items).toHaveLength(2)
    expect(list.items[0][0]).toEqual({ t: 'b', kids: [{ t: 'text', text: 'Scene lengths:' }] })
    expect(list.items[0].at(-1)).toEqual({ t: 'text', text: " (8 bars) isn't a multiple of 3, so the riff will be cut off mid-cycle." })
  })

  it('reads headings, numbered lists and italics', () => {
    expect(parseMarkdown('## Options\n1. one\n2. *two*\n')).toEqual([
      { t: 'h', level: 2, kids: [{ t: 'text', text: 'Options' }] },
      { t: 'ol', start: 1, items: [[{ t: 'text', text: 'one' }], [{ t: 'i', kids: [{ t: 'text', text: 'two' }] }]] },
    ])
  })

  it('keeps a marker without an end, and stars inside words, as text', () => {
    expect(inline('a ** b')).toEqual([{ t: 'text', text: 'a ** b' }])
    expect(inline('2*3*4 and `x')).toEqual([{ t: 'text', text: '2*3*4 and `x' }])
  })

  it('leaves HTML in a message as text', () => {
    expect(parseMarkdown('<img src=x onerror=alert(1)>')).toEqual([
      { t: 'p', kids: [{ t: 'text', text: '<img src=x onerror=alert(1)>' }] },
    ])
  })
})
