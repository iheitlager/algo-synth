<script setup lang="ts">
// A text editor with line numbers and highlighting, shared by the song text
// (#203) and the Modular code (#329). Typing goes into a plain textarea;
// under it, the same text in colours from the engine's lexer (audio/lex.ts),
// which the caller picks. The parse error, if any, marks its line and column.
// Given a completer (#595), a list of the words that fit opens as you type.
import { computed, nextTick, ref, useId, watch } from 'vue'
import { keepCaret } from '../audio/caret'
import { toggleComment } from '../audio/comment'
import { caretAt, opens, type Completer, type Completion } from '../audio/complete'
import { litLines, paint, type Lexer } from '../audio/lex'

const text = defineModel<string>({ required: true })
const props = withDefaults(
  defineProps<{
    lexer: Lexer | null
    label: string
    disabled?: boolean
    error?: { line: number; col: number; msg: string } | null
    /** What Tab inserts. */
    indent?: string
    /** What Ctrl+/ puts before a line to comment it out (#583); none, no toggle. */
    comment?: string
    /** Words playing now (#205), `[start, len]` in UTF-16 units over the text. */
    lit?: readonly (readonly [number, number])[]
    /** The words that fit at the caret (#595); none, no list. */
    completer?: Completer | null
  }>(),
  { disabled: false, error: null, indent: '  ', lit: () => [], completer: null },
)

const lines = computed(() => paint(text.value, props.lexer?.(text.value) ?? []))
// Lit apart from the colours, so the playhead's 20 ms updates never relex.
const lit = computed(() => litLines(text.value, props.lit))

const area = ref<HTMLTextAreaElement>()
const code = ref<HTMLElement>()
const gutter = ref<HTMLElement>()
function onScroll() {
  const a = area.value
  if (!a) return
  if (code.value) {
    code.value.scrollTop = a.scrollTop
    code.value.scrollLeft = a.scrollLeft
  }
  if (gutter.value) gutter.value.scrollTop = a.scrollTop
  if (menu.value) place()
}

// Text replaced from outside while editing (autocommit, ADR-0027) keeps the
// caret and the scroll where they were, instead of jumping to the end (#458).
watch(text, (next) => {
  const a = area.value
  if (!a || a.value === next || document.activeElement !== a) return
  const before = a.value
  const [from, to, top, left] = [a.selectionStart, a.selectionEnd, a.scrollTop, a.scrollLeft]
  void nextTick(() => {
    a.setSelectionRange(keepCaret(before, next, from), keepCaret(before, next, to))
    a.scrollTop = top
    a.scrollLeft = left
    onScroll()
  })
}, { flush: 'pre' })

// The list of words that fit (#595): where it shows, and which is picked.
const menu = ref<(Completion & { sel: number }) | null>(null)
const where = ref<Record<string, string>>({})
const list = ref<HTMLElement>()
const listId = useId()

/** Ask the engine what fits at the caret; `typed` as `opens` takes it. */
function suggest(typed: string | null) {
  const a = area.value
  const c = a && props.completer && a.selectionStart === a.selectionEnd ? props.completer(a.value, a.selectionStart) : null
  const shown = a && c ? opens(c, a.value, typed) : null
  menu.value = shown ? { ...shown, sel: 0 } : null
  if (menu.value) place()
}

// A character's width in the editor's font, measured once.
let chPx = 0
function charWidth(a: HTMLElement): number {
  if (!chPx) {
    const g = document.createElement('canvas').getContext('2d')
    if (g) g.font = getComputedStyle(a).font
    chPx = g?.measureText('0').width || 8
  }
  return chPx
}

// Under the word, or above it when the editor has no room below; kept
// inside the editor on the right.
function place() {
  const a = area.value
  const m = menu.value
  if (!a || !m) return
  const lh = parseFloat(getComputedStyle(a).lineHeight) || 19
  const { line, col } = caretAt(a.value, m.from)
  const top = 8 + line * lh - a.scrollTop
  const room = a.clientHeight - top - lh
  const x = 8 + col * charWidth(a) - a.scrollLeft
  const left = `${Math.max(0, x)}px`
  const rows = Math.min(8, m.items.length) + (m.help ? 2 : 0) + (m.preview ? 1 : 0)
  where.value = room < rows * lh + 8 && top > room
    ? { left, bottom: `${a.clientHeight - top}px` }
    : { left, top: `${top + lh}px` }
  void nextTick(() => {
    const w = list.value?.offsetWidth ?? 0
    if (menu.value === m && x + w > a.clientWidth) where.value = { ...where.value, left: `${Math.max(0, a.clientWidth - w - 4)}px` }
  })
}

function onInput(e: Event) {
  if (!props.completer) return
  const ie = e as InputEvent
  if (ie.inputType === 'insertText' && ie.data) suggest(ie.data.slice(-1))
  else if (menu.value && ie.inputType?.startsWith('delete')) suggest('')
  else menu.value = null
}

/** Put item `i` in for the word typed so far; as typing, so Ctrl+Z takes it back. */
function accept(i: number) {
  const a = area.value
  const m = menu.value
  const it = m?.items[i]
  menu.value = null
  if (!a || !m || !it) return
  a.focus()
  a.setSelectionRange(m.from, m.to)
  if (!document.execCommand('insertText', false, it.word)) a.setRangeText(it.word, m.from, m.to, 'end')
  text.value = a.value
}

function move(by: number) {
  const m = menu.value
  if (!m) return
  m.sel = (m.sel + by + m.items.length) % m.items.length
  void nextTick(() => list.value?.querySelectorAll('li')[m.sel]?.scrollIntoView({ block: 'nearest' }))
}

/**
 * With the list open, Up/Down pick, Tab or Enter put the word in (and do not
 * apply the line: the key is taken here), Esc closes it; Ctrl+Space opens it.
 * A call's help (#644) or preview (#645) alone takes only Esc.
 */
function onMenuKey(e: KeyboardEvent): boolean {
  if (props.completer && e.key === ' ' && e.ctrlKey) {
    suggest(null)
    return true
  }
  const m = menu.value
  if (!m) return false
  const plain = !e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey && m.items.length > 0
  if (plain && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) move(e.key === 'ArrowDown' ? 1 : -1)
  else if (plain && (e.key === 'Enter' || e.key === 'Tab')) accept(m.sel)
  else if (e.key === 'Escape') menu.value = null
  else {
    if (!['Shift', 'Control', 'Alt', 'Meta'].includes(e.key) && e.key.length > 1 && e.key !== 'Backspace' && e.key !== 'Delete') menu.value = null
    return false
  }
  return true
}

// Tab indents instead of leaving the text; Ctrl+/ (Cmd+/) comments lines out or in.
function onKey(e: KeyboardEvent) {
  if (onMenuKey(e)) {
    e.preventDefault()
    return
  }
  const a = area.value
  if (a && props.comment && (e.ctrlKey || e.metaKey) && (e.key === '/' || e.code === 'Slash')) {
    e.preventDefault()
    const t = toggleComment(a.value, a.selectionStart, a.selectionEnd, props.comment)
    if (t.text === a.value.slice(t.start, t.end)) return
    // As typing, so Ctrl+Z takes it back.
    a.setSelectionRange(t.start, t.end)
    if (!document.execCommand('insertText', false, t.text)) a.setRangeText(t.text, t.start, t.end)
    a.setSelectionRange(t.from, t.to)
    text.value = a.value
    return
  }
  if (e.key !== 'Tab' || e.shiftKey || !a) return
  e.preventDefault()
  a.setRangeText(props.indent, a.selectionStart, a.selectionEnd, 'end')
  text.value = a.value
}
</script>

<template>
  <div class="editor" :class="{ disabled }">
    <div ref="gutter" class="gutter" aria-hidden="true">
      <div v-for="(_, i) in lines" :key="i" class="lno" :class="{ err: error?.line === i + 1 }">{{ i + 1 }}</div>
    </div>
    <div class="code-wrap">
      <pre ref="code" class="code" aria-hidden="true"><div
        v-for="(line, i) in lines" :key="i" class="ln" :class="{ err: error?.line === i + 1 }"
      ><span
        v-for="(l, k) in lit[i] ?? []" :key="`lit${k}`" class="lit" :style="{ left: `${l.col}ch`, width: `${l.len}ch` }"
      /><span v-for="(p, k) in line" :key="k" :class="p.cls">{{ p.text }}</span><span
        v-if="error?.line === i + 1" class="mark" :style="{ left: `${error.col - 1}ch` }" :title="error.msg"
      /></div></pre>
      <textarea
        ref="area" v-model="text" wrap="off" spellcheck="false" autocapitalize="off" autocomplete="off"
        :disabled="disabled" :aria-label="label"
        :aria-autocomplete="completer ? 'list' : undefined" :aria-controls="menu?.items.length ? listId : undefined"
        :aria-expanded="completer ? !!menu?.items.length : undefined"
        :aria-activedescendant="menu?.items.length ? `${listId}-${menu.sel}` : undefined"
        :aria-describedby="menu?.help ? `${listId}-help` : undefined"
        @scroll="onScroll" @keydown="onKey" @input="onInput" @mousedown="menu = null" @blur="menu = null"
      />
      <div v-if="menu" ref="list" class="complete" :style="where" @mousedown.prevent>
        <p v-if="menu.help" :id="`${listId}-help`" class="help">{{ menu.help }}</p>
        <p v-if="menu.preview" class="preview">→ {{ menu.preview }}</p>
        <ul v-if="menu.items.length" :id="listId" role="listbox">
          <li
            v-for="(it, i) in menu.items" :id="`${listId}-${i}`" :key="it.word" role="option"
            :aria-selected="i === menu.sel" :class="{ sel: i === menu.sel }" @click="accept(i)"
          ><span class="word" :class="it.cls">{{ it.word }}</span><span class="detail">{{ it.detail }}</span></li>
        </ul>
      </div>
    </div>
  </div>
</template>

<style scoped>
.editor {
  --lh: 19px;
  flex: 1; min-height: 160px; display: flex; overflow: hidden;
  font-family: var(--font-mono); font-size: 13px; line-height: var(--lh);
  background: var(--bg); border: 1px solid var(--line); border-radius: 4px;
}
.editor:focus-within { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
.editor.disabled { opacity: 0.6; }
.gutter {
  flex: none; overflow: hidden; padding: 8px 8px 8px 10px; text-align: right; user-select: none;
  color: color-mix(in srgb, var(--muted) 60%, transparent); border-right: 1px solid var(--line); min-width: 3.2em;
}
.lno { height: var(--lh); }
.lno.err { color: var(--accent); }
.code-wrap { position: relative; flex: 1; min-width: 0; }
.code, textarea {
  position: absolute; inset: 0; margin: 0; padding: 8px; border: 0;
  font: inherit; line-height: inherit; white-space: pre; tab-size: 2; letter-spacing: normal;
}
.code { overflow: hidden; color: var(--text); pointer-events: none; }
.ln { position: relative; z-index: 0; min-height: var(--lh); }
.ln.err { background: color-mix(in srgb, var(--accent) 10%, transparent); }
/* A word playing now (#205), under the text. */
.lit {
  position: absolute; top: 1px; bottom: 1px; border-radius: 3px; z-index: -1;
  background: color-mix(in srgb, var(--accent) 35%, transparent);
  box-shadow: 0 0 6px color-mix(in srgb, var(--accent) 45%, transparent);
}
.mark {
  position: absolute; bottom: 1px; width: 1ch; height: 2px; background: var(--accent);
}
textarea {
  width: 100%; height: 100%; resize: none; overflow: auto; outline: none;
  background: transparent; color: transparent; caret-color: var(--text);
}
textarea::selection { background: color-mix(in srgb, var(--accent) 30%, transparent); color: transparent; }

/* The words that fit (#595), under or over the word typed. */
.complete {
  position: absolute; z-index: 20; padding: 4px;
  width: max-content; max-width: min(72ch, calc(100% - 8px));
  background: #0f1217; border: 1px solid #3a424e; border-radius: 4px; box-shadow: 0 10px 24px #000c;
}
.complete ul { margin: 0; padding: 0; list-style: none; max-height: calc(8 * var(--lh)); overflow-y: auto; }
/* How the call is written (#644), over the list. */
.complete .help { margin: 0; padding: 0 6px; white-space: normal; color: var(--text); }
/* What the call gives (#645): a rhythm, chords, notes or roots. */
.complete .preview { margin: 0; padding: 0 6px; white-space: normal; color: #8fd694; }
.complete .help + .preview { margin-top: 2px; }
.complete .help + ul, .complete .preview + ul { margin-top: 4px; padding-top: 4px; border-top: 1px solid var(--line); }
.complete li {
  display: flex; gap: 2ch; justify-content: space-between; height: var(--lh); padding: 0 6px;
  border-radius: 3px; cursor: pointer; white-space: nowrap;
}
.complete li.sel { background: color-mix(in srgb, var(--accent) 22%, transparent); }
.complete .word { flex: none; }
.complete .detail { min-width: 0; overflow: hidden; text-overflow: ellipsis; color: var(--muted); }

/* The engine's classes (song::lex::Class). */
.kw { color: var(--accent); }
.name { color: #9cc4ff; }
.num { color: var(--score); }
.note { color: #8fd694; }
.pad { color: var(--text); font-weight: 500; }
.step { color: var(--mono); }
.rest { color: color-mix(in srgb, var(--muted) 70%, transparent); }
.call { color: #cf9cf2; }
.param { color: #6fd3d3; }
.punct { color: var(--muted); }
.comment { color: #6b7280; font-style: italic; }
</style>
