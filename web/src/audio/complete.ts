// Completion in the song editor (#595): the engine says which words fit at
// the caret (crates/dsp/src/song/complete.rs), on the lexer's main-thread
// instance; this reads its answer and decides when the list shows.
import { CLASSES } from './lex'

/** A word that fits: its class (a lexer class, for colour), and what it is. */
export interface Item { cls: string; word: string; detail: string }
/** The words that may replace the text from `from` to `to` (UTF-16 units). */
export interface Completion { from: number; to: number; items: Item[] }
export type Completer = (text: string, caret: number) => Completion

/** The engine's answer: a `from\tto` line, then `class\tword\tdetail` lines. */
export function readCompletion(raw: string): Completion {
  const [head = '', ...lines] = raw.split('\n')
  const [from, to] = head.split('\t').map(Number)
  const items: Item[] = []
  for (const line of lines) {
    const [cls, word, detail = ''] = line.split('\t')
    if (word) items.push({ cls: CLASSES[Number(cls)] ?? '', word, detail })
  }
  return Number.isFinite(from) && Number.isFinite(to) && from <= to ? { from, to, items } : { from: 0, to: 0, items: [] }
}

/** Typing these after nothing opens the list: what follows them is a word to pick. */
const TRIGGERS = ['.', '(', ':', '[', ',']

/**
 * Whether the list opens after `typed` was typed (Ctrl+Space: `null`). A word
 * being typed opens it, and so does a trigger; a space does not, so Enter
 * after it still applies the line (#588). A word typed in full closes it,
 * for the same reason.
 */
export function opens(c: Completion, text: string, typed: string | null): boolean {
  if (!c.items.length) return false
  if (typed === null) return true
  const word = text.slice(c.from, c.to)
  if (c.items.some((i) => i.word === word)) return false
  return word.length > 0 || TRIGGERS.includes(typed)
}

/**
 * The line and column of `caret` in `text`, from 0, for placing the list:
 * the column as shown, a tab taking it on to the next stop of `tab`.
 */
export function caretAt(text: string, caret: number, tab = 2): { line: number; col: number } {
  const head = text.slice(0, caret)
  const start = head.lastIndexOf('\n') + 1
  let line = 0
  for (let i = 0; i < start; i++) if (head[i] === '\n') line++
  let col = 0
  for (const c of head.slice(start)) col = c === '\t' ? col - (col % tab) + tab : col + 1
  return { line, col }
}
