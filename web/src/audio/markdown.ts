// The Assistant's messages are markdown (#459): this parses the part of it a
// model writes (paragraphs, headings, lists, fenced code, bold, italic and
// code spans) into a tree that Markdown.vue draws with templates. No HTML is
// made from the text, so nothing in a message can inject any.

export type Inline =
  | { t: 'text'; text: string }
  | { t: 'code'; text: string }
  | { t: 'b'; kids: Inline[] }
  | { t: 'i'; kids: Inline[] }

export type Block =
  | { t: 'p'; kids: Inline[] }
  | { t: 'h'; level: number; kids: Inline[] }
  | { t: 'ul'; items: Inline[][] }
  | { t: 'ol'; start: number; items: Inline[][] }
  | { t: 'code'; lang: string; text: string }

/** The spans of one line or paragraph: `code`, **bold**, *italic* or _italic_; a marker with no end is text. */
export function inline(src: string): Inline[] {
  const out: Inline[] = []
  let text = ''
  const flush = () => {
    if (text) out.push({ t: 'text', text })
    text = ''
  }
  let i = 0
  while (i < src.length) {
    const c = src[i]
    if (c === '`') {
      const end = src.indexOf('`', i + 1)
      if (end > i + 1) {
        flush()
        out.push({ t: 'code', text: src.slice(i + 1, end) })
        i = end + 1
        continue
      }
    } else if (src.startsWith('**', i)) {
      const end = src.indexOf('**', i + 2)
      if (end > i + 2) {
        flush()
        out.push({ t: 'b', kids: inline(src.slice(i + 2, end)) })
        i = end + 2
        continue
      }
    } else if ((c === '*' || c === '_') && src[i + 1] !== ' ' && src[i + 1] !== undefined) {
      // An italic marker opens at a word's start and closes before a space or the end.
      const open = i === 0 || /[\s(]/.test(src[i - 1])
      const end = src.indexOf(c, i + 1)
      if (open && end > i + 1 && src[end - 1] !== ' ' && !/\w/.test(src[end + 1] ?? '')) {
        flush()
        out.push({ t: 'i', kids: inline(src.slice(i + 1, end)) })
        i = end + 1
        continue
      }
    }
    text += c
    i++
  }
  flush()
  return out
}

const BULLET = /^\s*[-*+]\s+(.*)$/
const NUMBER = /^\s*(\d+)[.)]\s+(.*)$/
const HEADING = /^(#{1,6})\s+(.*)$/
const FENCE = /^\s*(```|~~~)\s*([\w-]*)\s*$/

/** The blocks of `src`: a blank line ends a paragraph or list; an indented line goes on with the item above. */
export function parseMarkdown(src: string): Block[] {
  const lines = src.replace(/\r\n?/g, '\n').split('\n')
  const out: Block[] = []
  let para: string[] = []
  let list = null as { t: 'ul' | 'ol'; start: number; items: string[] } | null
  const endPara = () => {
    if (para.length) out.push({ t: 'p', kids: inline(para.join(' ')) })
    para = []
  }
  const endList = () => {
    if (!list) return
    const items = list.items.map(inline)
    out.push(list.t === 'ul' ? { t: 'ul', items } : { t: 'ol', start: list.start, items })
    list = null
  }
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]
    const fence = FENCE.exec(line)
    if (fence) {
      endPara()
      endList()
      const body: string[] = []
      while (++i < lines.length && !lines[i].trim().startsWith(fence[1])) body.push(lines[i])
      out.push({ t: 'code', lang: fence[2], text: body.join('\n') })
      continue
    }
    if (!line.trim()) {
      endPara()
      endList()
      continue
    }
    const heading = HEADING.exec(line)
    if (heading) {
      endPara()
      endList()
      out.push({ t: 'h', level: heading[1].length, kids: inline(heading[2]) })
      continue
    }
    const bullet = BULLET.exec(line)
    const number = bullet ? null : NUMBER.exec(line)
    if (bullet || number) {
      endPara()
      const t = bullet ? 'ul' : 'ol'
      if (list?.t !== t) {
        endList()
        list = { t, start: number ? Number(number[1]) : 1, items: [] }
      }
      list.items.push(bullet ? bullet[1] : (number?.[2] ?? ''))
      continue
    }
    if (list && /^\s+/.test(line)) {
      list.items[list.items.length - 1] += ` ${line.trim()}`
      continue
    }
    endList()
    para.push(line.trim())
  }
  endPara()
  endList()
  return out
}
