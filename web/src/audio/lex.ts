// Highlighting for the song editor (#203): the engine lexes the text
// (crates/dsp/src/song/lex.rs), this only cuts it into coloured pieces. The
// lexer is a second instance of dsp.wasm on the main thread, so typing never
// costs the audio thread anything.
import { shallowRef } from 'vue'

/** The engine's span classes by number (song::lex::Class), as CSS class names. */
export const CLASSES = ['', 'kw', 'name', 'num', 'note', 'pad', 'step', 'rest', 'call', 'param', 'punct', 'comment'] as const

export interface Span { start: number; len: number; cls: string }
export type Lexer = (text: string) => Span[]

interface LexExports {
  memory: WebAssembly.Memory
  lex_buf(len: number): number
  lex(): number
  lex_ptr(): number
}

/** The lexer, once dsp.wasm is compiled; until then the text is plain. */
export const lexer = shallowRef<Lexer | null>(null)

export function wasmLexer(module: WebAssembly.Module): Lexer {
  const w = new WebAssembly.Instance(module, {}).exports as unknown as LexExports
  const encoder = new TextEncoder()
  return (text) => {
    const bytes = encoder.encode(text)
    const ptr = w.lex_buf(bytes.length)
    if (!ptr) return []
    new Uint8Array(w.memory.buffer, ptr, bytes.length).set(bytes)
    const n = w.lex()
    // Read after `lex`: memory may have grown, which detaches older views.
    const raw = new Uint32Array(w.memory.buffer, w.lex_ptr(), n * 3)
    const out: Span[] = []
    for (let i = 0; i < n; i++) out.push({ start: raw[i * 3], len: raw[i * 3 + 1], cls: CLASSES[raw[i * 3 + 2]] ?? '' })
    return out
  }
}

/** A run of text and its class ('' for plain). */
export interface Piece { text: string; cls: string }

/** The text as lines of pieces: the spans' text classed, the gaps plain. */
export function paint(text: string, spans: Span[]): Piece[][] {
  const lines: Piece[][] = [[]]
  const add = (s: string, cls: string) => {
    s.split('\n').forEach((part, i) => {
      if (i > 0) lines.push([])
      if (part) lines[lines.length - 1].push({ text: part, cls })
    })
  }
  let at = 0
  for (const s of spans) {
    if (s.start < at || s.start + s.len > text.length) continue
    add(text.slice(at, s.start), '')
    add(text.slice(s.start, s.start + s.len), s.cls)
    at = s.start + s.len
  }
  add(text.slice(at), '')
  return lines
}
