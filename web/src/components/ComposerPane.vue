<script setup lang="ts">
// The composer (spec 003 Req 5, ADR-0012): the song's drum fragments as a step
// grid beside the song's text. The engine holds the song: a click sends
// `setStep`, an edited text is sent to be parsed, and both redraw from what the
// engine sends back. Nothing here parses or plays.
import { computed, onMounted, watch } from 'vue'
import {
  MUTE, loadSong, params, requestSong, routeTrack, setStep, song, status, stripName, synthColour, synths, type Route,
} from '../audio/engine'
import { modelDef } from '../audio/models'
import { Model, Pad, Param } from '../audio/params'

// A first beat to start from when the song is empty.
const STARTER = `tempo 120
swing 50
track kit drums

frag beat = kit /16
  bd x...x...x...x...
  sn ....x.......x...
  ch x.x.x.x.x.x.x.x.
`

const padName = (id: number) => Object.entries(Pad).find(([, v]) => v === id)?.[0] ?? '?'
const isKit = (s: number) => params.values[s]?.[Param.Model] === Model.Tr808

// A track plays on one of the shown synths, or is muted.
const choices = computed<{ label: string; value: Route }[]>(() => [
  ...synths.list.map((s) => ({ label: `${stripName(s)} (${modelDef(params.values[s]?.[Param.Model] ?? 0).name})`, value: s })),
  { label: 'Mute', value: MUTE },
])
const noKit = computed(() => song.tracks.length > 0 && !synths.list.some(isKit))
const dirty = computed(() => song.draft !== song.text)

// Off → hit → accent → off.
const cycle = (f: number, l: number, s: number, level: number) => setStep(f, l, s, (level + 1) % 3)
// The step a lane plays now: each lane loops on its own length.
const playing = (len: number) => (song.step < 0 ? -1 : song.step % len)

function apply() {
  loadSong(song.draft)
}
function onKey(e: KeyboardEvent) {
  // Keys typed here are text, not notes for the synths.
  e.stopPropagation()
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    apply()
  }
}

onMounted(() => {
  if (status.running) requestSong()
})
watch(() => status.running, (on) => on && requestSong())
</script>

<template>
  <section class="pane composer">
    <div class="pane-head">
      <span>Composer</span>
      <span v-if="!status.running">Power on to compose</span>
      <span v-else>{{ song.frags.length }} {{ song.frags.length === 1 ? 'frag' : 'frags' }} · {{ song.tempo }} BPM</span>
    </div>
    <p v-if="noKit" class="notice">No synth is a TR-808: add one with + Synth › Drums, then pick it for the track.</p>
    <div class="body">
      <div class="grid">
        <p v-if="status.running && !song.frags.length" class="muted">
          The song has no fragments yet.
          <button @click="loadSong(STARTER)">Start a beat</button>
        </p>
        <div v-for="(frag, f) in song.frags" :key="f" class="frag">
          <div class="frag-head">
            <b>{{ frag.name }}</b>
            <span class="muted">on {{ song.tracks[frag.track]?.name }}</span>
            <select
              :value="song.tracks[frag.track]?.synth ?? MUTE"
              @change="routeTrack(frag.track, Number(($event.target as HTMLSelectElement).value) as Route)"
            >
              <option v-for="c in choices" :key="c.value" :value="c.value">{{ c.label }}</option>
            </select>
          </div>
          <div v-for="(lane, l) in frag.lanes" :key="l" class="lane">
            <span class="pad">{{ padName(lane.pad) }}</span>
            <div class="steps">
              <button
                v-for="(level, s) in lane.steps" :key="s"
                class="step" :class="[`l${level}`, { beat: s % 4 === 0, now: s === playing(lane.steps.length) }]"
                :style="{ '--hit': synthColour(song.tracks[frag.track]?.synth ?? 0) }"
                :title="`${padName(lane.pad)} step ${s + 1}`"
                @click="cycle(f, l, s, level)"
              />
            </div>
          </div>
        </div>
      </div>
      <div class="text">
        <textarea
          v-model="song.draft" spellcheck="false" :disabled="!status.running"
          aria-label="Song text" @keydown="onKey"
        />
        <div class="text-foot">
          <button :disabled="!dirty || !status.running" title="Ctrl+Enter" @click="apply">Apply</button>
          <span v-if="song.error" class="error">line {{ song.error.line }}, col {{ song.error.col }}: {{ song.error.msg }}</span>
          <span v-else-if="dirty" class="muted">edited, not applied</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.composer { display: flex; flex-direction: column; min-height: 0; }
.notice { margin: 4px 12px; color: var(--accent); }
.body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(260px, 34%); gap: 12px; padding: 8px 12px; }
.grid { overflow: auto; display: flex; flex-direction: column; gap: 16px; }
.frag-head { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
.lane { display: flex; align-items: center; gap: 8px; margin: 3px 0; }
.pad { width: 2.2em; font-family: var(--mono, monospace); color: var(--muted); }
.steps { display: flex; gap: 3px; }
.step { width: 26px; height: 26px; padding: 0; border-radius: 3px; background: var(--panel-2); border: 1px solid var(--line); }
.step.beat { margin-left: 6px; }
.step.l1 { background: color-mix(in srgb, var(--hit) 55%, var(--panel-2)); }
.step.l2 { background: var(--hit); }
.step.now { outline: 2px solid var(--accent); outline-offset: 1px; }
.text { display: flex; flex-direction: column; min-height: 0; gap: 6px; }
.text textarea {
  flex: 1; min-height: 160px; resize: none; font-family: var(--mono, monospace); font-size: 13px;
  background: var(--bg); color: inherit; border: 1px solid var(--line); border-radius: 4px; padding: 8px;
}
.text-foot { display: flex; align-items: center; gap: 10px; }
.error { color: var(--accent); }
.muted { color: var(--muted); }
</style>
