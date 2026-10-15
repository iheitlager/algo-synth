<script setup lang="ts">
// The song text with line numbers and highlighting (#203), from the song lexer.
// It says when a line is done (#588): on Enter, or when the caret leaves a line
// it was typed on (arrow keys, a click, leaving the editor).
import { lexer } from '../audio/lex'
import CodeArea from './CodeArea.vue'

const text = defineModel<string>({ required: true })
defineProps<{
  disabled?: boolean
  error?: { line: number; col: number; msg: string } | null
  /** Words playing now (#205), over the text shown. */
  lit?: readonly (readonly [number, number])[]
}>()
const emit = defineEmits<{ done: [] }>()

// The line last typed on, until it is done.
let edited: number | null = null
const lineOf = (a: HTMLTextAreaElement) => a.value.slice(0, a.selectionStart).split('\n').length - 1
const area = (e: Event) => (e.target instanceof HTMLTextAreaElement ? e.target : null)

function done() {
  edited = null
  emit('done')
}
function onInput(e: Event) {
  const a = area(e)
  if (a) edited = lineOf(a)
}
function onKey(e: KeyboardEvent) {
  // Enter ends the line; the new line is typed into after the key, so the
  // text is read once it is in. Ctrl+Enter is the composer's own.
  if (e.key === 'Enter' && !e.ctrlKey && !e.metaKey && !e.altKey && area(e)) setTimeout(done)
}
function onCaret(e: Event) {
  const a = area(e)
  if (a && edited !== null && (e.type === 'focusout' || lineOf(a) !== edited)) done()
}
</script>

<template>
  <CodeArea
    v-model="text" :lexer="lexer" label="Song text" comment="#" :disabled="disabled" :error="error" :lit="lit"
    @input="onInput" @keydown="onKey" @keyup="onCaret" @mouseup="onCaret" @focusout="onCaret"
  />
</template>
