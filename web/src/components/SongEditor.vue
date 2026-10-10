<script setup lang="ts">
// The song text with line numbers and highlighting (#203), from the song lexer.
import { lexer } from '../audio/lex'
import CodeArea from './CodeArea.vue'

const text = defineModel<string>({ required: true })
defineProps<{
  disabled?: boolean
  error?: { line: number; col: number; msg: string } | null
  /** Words playing now (#205), over the text shown. */
  lit?: readonly (readonly [number, number])[]
}>()
</script>

<template>
  <CodeArea v-model="text" :lexer="lexer" label="Song text" :disabled="disabled" :error="error" :lit="lit" />
</template>
