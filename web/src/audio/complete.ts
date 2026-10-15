// Completion in the song editor (#595): the engine says which words fit at
// the caret (crates/dsp/src/song/complete.rs), on the lexer's main-thread
// instance; this reads its answer and decides when the list shows.
import { CLASSES } from './lex'

/** A word that fits: its class (a lexer class, for colour), and what it is. */
export interface Item { cls: string; word: string; detail: string }
/**
 * The words that may replace the text from `from` to `to` (UTF-16 units), how
 * the call the caret is in is written (#644) and what it gives (#645), or ''.
 */
export interface Completion { from: number; to: number; items: Item[]; help: string; preview: string }
export type Completer = (text: string, caret: number) => Completion

/** The engine's answer: a `from\tto\thelp\tpreview` line, then `class\tword\tdetail` lines. */
export function readCompletion(raw: string): Completion {
  const [head = '', ...lines] = raw.split('\n')
  const [a, b, help = '', preview = ''] = head.split('\t')
  const [from, to] = [a, b].map(Number)
  const items: Item[] = []
  for (const line of lines) {
    const [cls, word, detail = ''] = line.split('\t')
    if (word) items.push({ cls: CLASSES[Number(cls)] ?? '', word, detail })
  }
  return Number.isFinite(from) && Number.isFinite(to) && from <= to ? { from, to, items, help, preview } : { from: 0, to: 0, items: [], help: '', preview: '' }
}

/**
 * Typing these after nothing opens the list: what follows them is a word to
 * pick; after `\` a Modular filter's voicing (#647).
 */
const TRIGGERS = ['.', '(', ':', '[', ',', '\\']

/**
 * What shows after `typed` was typed (Ctrl+Space: `null`), or nothing. A word
 * being typed opens the list, and so does a trigger; a space does not, so
 * Enter after it still applies the line (#588). A word typed in full closes
 * it, for the same reason. A call's help and preview show alone when no
 * list does: they take no keys.
 */
export function opens(c: Completion, text: string, typed: string | null): Completion | null {
  const word = text.slice(c.from, c.to)
  const list = c.items.length > 0 && (typed === null ||
    (!c.items.some((i) => i.word === word) && (word.length > 0 || TRIGGERS.includes(typed))))
  if (list) return c
  return c.help || c.preview ? { ...c, items: [] } : null
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
