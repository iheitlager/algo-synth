<script setup lang="ts">
// The Modular faceplate's code (ADR-0024): the synth's SuperCollider SynthDef,
// written here and sent with `setCode`; the engine builds it, or keeps the
// synth as it was and says where it went wrong. The code lives on the synth,
// like a patch; saving the track's sound as a setting puts it in the song.
import { computed, ref, watch } from 'vue'
import { codes, setCode, status } from '../../audio/engine'

const props = defineProps<{ s: number }>()

const code = computed(() => codes[props.s])
const draft = ref('')
// A fresh text from the engine replaces the draft unless the draft is the one being fixed.
watch(
  () => code.value?.text,
  (text) => {
    if (text !== undefined && !code.value?.error) draft.value = text
  },
  { immediate: true },
)

const apply = () => setCode(props.s, draft.value)
function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    apply()
  }
}
</script>

<template>
  <div class="code" @keydown="onKey">
    <textarea v-model="draft" spellcheck="false" aria-label="SynthDef" :disabled="!status.running" />
    <div class="row">
      <p v-if="code?.error" class="err" role="alert">
        Line {{ code.error.line }}, column {{ code.error.col }}: {{ code.error.msg }}
      </p>
      <button :disabled="!status.running" title="Build the SynthDef on this synth (Ctrl+Enter)" @click="apply">Apply</button>
    </div>
  </div>
</template>

<style scoped>
.code { flex: 1 1 100%; display: flex; flex-direction: column; gap: 6px; min-width: 0; height: 260px; }
textarea {
  flex: 1; resize: none; min-height: 0; padding: 8px;
  background: var(--con-inset); color: var(--con-paper);
  border: 1px solid var(--con-line); border-radius: 4px;
  font: 400 12px/1.5 var(--con-font-mono); tab-size: 4;
}
.row { display: flex; align-items: center; gap: 10px; }
.row button { margin-left: auto; }
.err { margin: 0; color: var(--c); font-size: 12px; }
</style>
