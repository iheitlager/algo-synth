<script setup lang="ts">
// The Modular faceplate's code (ADR-0024): the synth's SuperCollider SynthDef,
// written here and sent with `setCode`; the engine builds it, or keeps the
// synth as it was and says where it went wrong. It builds as it is typed, half
// a second after typing stops (Ctrl+Enter at once), and the engine folds it
// into the track's setting (ADR-0027). Numbered and highlighted by the
// engine's SuperCollider lexer (#329).
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { APPLY_AFTER_MS, codes, setCode, status } from '../../audio/engine'
import { scLexer } from '../../audio/lex'
import CodeArea from '../CodeArea.vue'

const props = defineProps<{ s: number }>()

const code = computed(() => codes[props.s])
const draft = ref('')
// The engine's text the draft was last taken from: while the draft is still
// that, a new text (a preset, a song, a knob turned) replaces it; an edit not
// yet applied, or one being fixed after an error, is kept.
let taken: string | null = null
watch(
  () => code.value?.text,
  (text) => {
    if (text === undefined) return
    const clean = taken === null || draft.value === taken
    if (clean && !code.value?.error) draft.value = text
    taken = text
  },
  { immediate: true },
)

let typing: ReturnType<typeof setTimeout> | undefined
const apply = () => {
  clearTimeout(typing)
  setCode(props.s, draft.value)
}
// An edit builds once typing rests; a text the engine sent is not an edit.
watch(draft, (text) => {
  clearTimeout(typing)
  if (text !== taken) typing = setTimeout(apply, APPLY_AFTER_MS)
})
onBeforeUnmount(() => clearTimeout(typing))
function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    apply()
  }
}
</script>

<template>
  <div class="code-editor" @keydown="onKey">
    <div class="bar">
      <p v-if="code?.error" class="err" role="alert">
        Line {{ code.error.line }}, column {{ code.error.col }}: {{ code.error.msg }}
      </p>
      <span v-else class="hint">builds when you pause · Ctrl+Enter now</span>
    </div>
    <CodeArea
      v-model="draft" :lexer="scLexer" label="SynthDef" indent="    " :disabled="!status.running"
      :error="code?.error ?? null"
    />
  </div>
</template>

<style scoped>
.code-editor { flex: 1 1 100%; display: flex; flex-direction: column; gap: 6px; min-width: 0; min-height: 300px; }
.bar { display: flex; align-items: center; gap: 10px; }
.err { margin: 0; color: var(--c); font-size: 12px; }
.hint { font-size: 11px; color: var(--con-silk-dim); }
</style>
