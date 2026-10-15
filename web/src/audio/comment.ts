// Ctrl+/ in an editor (#583): comment the selected lines out, or back in. Text,
// not music: the engine still reads the result, where a `#` at a line's start
// or after its indent starts a comment (language.md, "Comments").

/** The change a toggle makes: `text` replaces `[start, end)` of the old text,
 * and `[from, to)` is the selection after it. */
export interface Toggle { start: number; end: number; text: string; from: number; to: number }

/**
 * Toggle `mark` on the lines `[from, to)` touches; a selection ending at a
 * line's start leaves that line out. When every non-blank line is commented the
 * marks go, else each non-blank line gets `mark ` at the block's least indent.
 */
export function toggleComment(text: string, from: number, to: number, mark = '#'): Toggle {
  const start = text.lastIndexOf('\n', from - 1) + 1
  let last = to > from && text[to - 1] === '\n' ? to - 1 : to
  if (last < start) last = start
  const nl = text.indexOf('\n', last)
  const end = nl < 0 ? text.length : nl
  const lines = text.slice(start, end).split('\n')
  const indent = (l: string) => l.length - l.trimStart().length
  const filled = lines.filter((l) => l.trim() !== '')
  const commented = filled.length > 0 && filled.every((l) => l.trimStart().startsWith(mark))
  const least = Math.min(...filled.map(indent))
  // Each line's change: where it happens in the line, and by how many characters.
  const edits = lines.map((l) => {
    if (l.trim() === '') return { at: 0, by: 0, line: l }
    if (commented) {
      const at = indent(l)
      const cut = l.startsWith(`${mark} `, at) ? mark.length + 1 : mark.length
      return { at, by: -cut, line: l.slice(0, at) + l.slice(at + cut) }
    }
    return { at: least, by: mark.length + 1, line: `${l.slice(0, least)}${mark} ${l.slice(least)}` }
  })
  // A position moves by the changes before it on its line: past an added mark
  // when after it, or at it unless it starts a selection (which then takes the
  // mark in); back to a removed mark's start when inside it.
  const move = (pos: number, starts = false) => {
    let lineStart = start
    let shift = 0
    for (const [i, e] of edits.entries()) {
      const lineEnd = lineStart + (lines[i] as string).length
      if (pos <= lineEnd) {
        const col = pos - lineStart
        if (e.by > 0 && (col > e.at || (col === e.at && !starts))) shift += e.by
        if (e.by < 0 && col > e.at) shift -= Math.min(-e.by, col - e.at)
        return pos + shift
      }
      shift += e.by
      lineStart = lineEnd + 1
    }
    return pos + shift
  }
  return { start, end, text: edits.map((e) => e.line).join('\n'), from: move(from, from < to), to: move(to) }
}
