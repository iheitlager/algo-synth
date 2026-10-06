<script setup lang="ts">
// The Modular faceplate's voice (#292): the song voice this synth plays,
// written here as on the Sound screen (ADR-0020) and sent with `editVoice`;
// the engine puts it into the song, and its ctl knobs sit under it. A synth
// playing a factory voice has none in the song to edit, so it says how to add one.
import { computed, ref, watch } from 'vue'
import { editVoice, song, status } from '../../audio/engine'
import { exp, lin } from '../../audio/console'
import { playedVoice } from '../../audio/faceplate'
import { Param, type ParamId } from '../../audio/params'
import SongEditor from '../SongEditor.vue'
import ParamKnob from '../console/ParamKnob.vue'

const props = defineProps<{ s: number }>()

const index = computed(() => playedVoice(song.tracks, props.s))
const voice = computed(() => song.voices[index.value])
const draft = ref('')
// A fresh text from the engine replaces the draft unless the draft is the one being fixed.
watch(
  () => voice.value?.text,
  (text) => {
    if (text !== undefined && !song.voiceError) draft.value = text
  },
  { immediate: true },
)

const apply = () => editVoice(index.value, draft.value.endsWith('\n') ? draft.value : `${draft.value}\n`)
function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    apply()
  }
}

const fmt = (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 10 ? v.toFixed(1) : v.toFixed(2))
const knobs = computed(() =>
  (voice.value?.ctls ?? []).map((c, i) => ({
    ...c,
    id: (Param.Ctl1 + i) as ParamId,
    scale: c.exp ? exp(c.lo, c.hi) : lin(c.lo, c.hi),
  })),
)
</script>

<template>
  <div v-if="voice" class="voice" @keydown="onKey">
    <SongEditor v-model="draft" :error="song.voiceError" :disabled="!status.running" />
    <div class="row">
      <p v-if="song.voiceError" class="err" role="alert">
        Line {{ song.voiceError.line }}, column {{ song.voiceError.col }}: {{ song.voiceError.msg }}
      </p>
      <button :disabled="!status.running" title="Put the voice into the song (Ctrl+Enter)" @click="apply">Apply</button>
    </div>
    <div v-if="knobs.length" class="knobs">
      <ParamKnob v-for="k in knobs" :key="k.name" :synth="s" :id="k.id" :label="k.name" :scale="k.scale" :def="k.def" :text="fmt" />
    </div>
  </div>
  <p v-else class="hint">
    This synth plays a factory voice. To write your own, add <code>voice lead = { saw(freq) |&gt; svf(lp, 1800) }</code>
    to the song and play it with <code>track lead synth Modular lead</code>.
  </p>
</template>

<style scoped>
.voice { flex: 1 1 100%; display: flex; flex-direction: column; gap: 6px; min-width: 0; height: 220px; }
.row { display: flex; align-items: center; gap: 10px; }
.row button { margin-left: auto; }
.err { margin: 0; color: var(--c); font-size: 12px; }
.knobs { display: flex; flex-wrap: wrap; gap: 12px; }
.hint { margin: 0; max-width: 520px; font: 400 11px/1.5 var(--con-font-mono); }
code { color: var(--con-paper); }
</style>
