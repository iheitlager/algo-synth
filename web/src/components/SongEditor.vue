<script setup lang="ts">
// The song text with line numbers and highlighting (#203), from the song lexer.
// It says when a line is done (#588): on Enter, or when the caret leaves a line
// it was typed on (arrow keys, a click, leaving the editor).
import { completer, lexer } from '../audio/lex'
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
  if (!a) return
  // Enter ends the line: applied as the line break goes in, so what is typed
  // after it is not (a timer read it late when typing ran on). Ctrl+Enter is
  // the composer's own, and an Enter that took a completion (#595) puts no
  // line break in.
  if ((e as InputEvent).inputType === 'insertLineBreak') done()
  else edited = lineOf(a)
}
function onCaret(e: Event) {
  const a = area(e)
  if (a && edited !== null && (e.type === 'focusout' || lineOf(a) !== edited)) done()
}
</script>

<template>
  <CodeArea
    v-model="text" :lexer="lexer" :completer="completer" label="Song text" comment="#" :disabled="disabled" :error="error" :lit="lit"
    @input="onInput" @keyup="onCaret" @mouseup="onCaret" @focusout="onCaret"
  />
</template>
