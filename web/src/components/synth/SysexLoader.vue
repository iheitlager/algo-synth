<script setup lang="ts">
// Load DX7 voices from a SysEx file (spec 006 Req 14). The engine parses the bytes
// and sets the parameters; this only forwards a file and a choice.
import { ref } from 'vue'
import { applySysex, capturePreset, loadSysex, status, sysex } from '../../audio/engine'
import { savePreset } from '../../audio/library'

const props = defineProps<{ s: number }>()

async function pick(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (file) await loadSysex(await file.arrayBuffer(), file.name)
}
const voice = ref(-1)
const saved = ref('')
function choose(e: Event) {
  const i = Number((e.target as HTMLSelectElement).value)
  voice.value = i
  saved.value = ''
  if (i >= 0) applySysex(props.s, i)
}
/** Keep the chosen voice, as it sounds now, as a user synth preset (ADR-0014). */
function keep() {
  const name = sysex.names[voice.value]?.trim() || `Voice ${voice.value + 1}`
  savePreset(capturePreset(name, { kind: 'synth', s: props.s }))
  saved.value = name
}
</script>

<template>
  <div class="sysex">
    <label class="file">
      Load .syx
      <input type="file" accept=".syx,.SYX" aria-label="Load DX7 SysEx file" @change="pick" />
    </label>
    <select v-if="sysex.names.length" aria-label="DX7 voice" :disabled="!status.running" @change="choose">
      <option value="-1" selected disabled>{{ sysex.fileName || 'voice' }} ({{ sysex.names.length }})</option>
      <option v-for="(n, i) in sysex.names" :key="i" :value="i">{{ String(i + 1).padStart(2, '0') }} {{ n }}</option>
    </select>
    <button v-if="voice >= 0" class="file" :disabled="!status.running" title="Save this voice as a user preset" @click="keep">Save to library</button>
    <small v-if="saved">Saved “{{ saved }}”</small>
    <p v-if="sysex.error" class="err" role="alert">{{ sysex.error }}</p>
  </div>
</template>

<style scoped>
.sysex { display: flex; flex-direction: column; gap: 6px; font-size: 12px; }
.file { cursor: pointer; padding: 4px 8px; border: 1px solid var(--trim); border-radius: 3px; text-align: center; color: var(--c); }
.file input { display: none; }
select { background: var(--plate); color: var(--con-silk); border: 1px solid var(--trim); font: inherit; max-width: 160px; }
.err { margin: 0; color: #e66; }
button.file { background: none; font: inherit; }
small { color: var(--con-silk-dim); }
</style>
