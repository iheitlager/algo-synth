<script setup lang="ts">
// The composer (spec 003 Req 5, ADR-0012): the song's drum fragments as a step
// grid and its note fragments as piano rolls beside the song's text (SongEditor). The engine holds the song: a click sends
// `setStep`, an edited text is sent to be parsed, and both redraw from what the
// engine sends back. Nothing here parses or plays.
import { computed, onMounted, watch } from 'vue'
import {
  MUTE, loadSong, params, requestSong, routeTrack, typeSong, setSongSwing, setSongTempo, setStep, song, songPosition as position,
  status, stripName, synthColour, synths, type Route,
} from '../audio/engine'
import { modelDef } from '../audio/models'
import { Model, Pad, Param } from '../audio/params'
import NoteRoll from './NoteRoll.vue'
import SongEditor from './SongEditor.vue'
import TrackStrip from './TrackStrip.vue'

const SONG_REFERENCE = 'https://github.com/iheitlager/algo-synth/blob/main/docs/song.md'

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
// A drum track plays on a drum machine or a pad sampler.
const KITS: number[] = [Model.Tr808, Model.Tr909, Model.PadSampler]
const isKit = (s: number) => KITS.includes(params.values[s]?.[Param.Model] ?? -1)

// A track plays on one of the shown synths, or is muted.
const choices = computed<{ label: string; value: Route }[]>(() => [
  ...synths.list.map((s) => ({ label: `${stripName(s)} (${modelDef(params.values[s]?.[Param.Model] ?? 0).name})`, value: s })),
  { label: 'Mute', value: MUTE },
])
const noKit = computed(() => song.tracks.some((t) => t.kind === 'drums') && !synths.list.some(isKit))
const dirty = computed(() => song.draft !== song.text)

// Off → hit → accent → off.
// A click cycles off → hit → accent → off; a ghost, flam or drag (written in the text) clicks off.
const cycle = (f: number, l: number, s: number, level: number) => setStep(f, l, s, level < 2 ? level + 1 : 0)
// The section playing now, in an arrangement.
const section = computed(() => (song.entry >= 0 ? song.sections[song.arrange[song.entry]] : undefined))
// The step a lane plays now, looping on its own length; in an arrangement it counts from the section's start,
// and a frag the section does not play has none.
// The lane step under the clock: a lane of `grid` steps a bar (#353) moves
// grid/16 steps per clock step.
const playing = (f: number, len: number, grid: number) => {
  if (song.entry >= 0 && !section.value?.frags[f]) return -1
  const k = song.entry >= 0 ? song.local : song.step
  return k < 0 ? -1 : Math.floor((k * grid) / 16) % len
}

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
  <section class="pane composer" :class="{ playing: song.playing }">
    <div class="pane-head">
      <span>Composer</span>
      <span class="head-right">
        <span v-if="!status.running">Power on to compose</span>
        <span v-else>{{ song.frags.length }} {{ song.frags.length === 1 ? 'frag' : 'frags' }}</span>
        <!-- The song language's reference (#250), on GitHub: rendered, and one copy to keep current. -->
        <a class="ref" :href="SONG_REFERENCE" target="_blank" rel="noopener" title="The song language: every line, with examples">Song reference ↗</a>
      </span>
    </div>
    <!-- Play, pause and stop are the transport bar's (ADR-0022). -->
    <div class="controls">
      <label class="field" title="The song's tempo; its text follows">
        BPM <input class="num" type="number" min="20" max="300" step="1" :value="song.tempo" :disabled="!status.running"
          @change="setSongTempo(Number(($event.target as HTMLInputElement).value))" />
      </label>
      <label class="field" title="The song's swing: 50 is straight, 75 the most">
        Swing <input class="num" type="number" min="50" max="75" step="1" :value="song.swing" :disabled="!status.running"
          @change="setSongSwing(Number(($event.target as HTMLInputElement).value))" />
      </label>
      <span v-if="song.playing && position >= 0" class="muted">bar {{ Math.floor(position / 16) + 1 }} · step {{ (position % 16) + 1 }}</span>
    </div>
    <div class="body">
      <div class="grid">
        <p v-if="noKit" class="notice">No synth is a drum kit: add one with + Synth › Drums, then pick it for the track.</p>
        <TrackStrip v-if="status.running && song.tracks.length" class="strip" :synths="choices" />
        <p v-if="status.running && !song.frags.length" class="muted">
          The song has no fragments yet.
          <button @click="loadSong(STARTER)">Start a beat</button>
        </p>
        <div v-for="(frag, f) in song.frags" :key="f" class="frag">
          <div class="frag-head">
            <b>{{ frag.name }}</b>
            <span class="muted">on {{ song.tracks[frag.track]?.name }}</span>
            <select
              class="picker" :style="{ '--c': synthColour(song.tracks[frag.track]?.synth ?? 0) }"
              :value="song.tracks[frag.track]?.synth ?? MUTE"
              @change="routeTrack(frag.track, Number(($event.target as HTMLSelectElement).value) as Route)"
            >
              <option v-for="c in choices" :key="c.value" :value="c.value">{{ c.label }}</option>
            </select>
          </div>
          <NoteRoll
            v-if="frag.notes" :frag="frag" :index="f"
            :colour="synthColour(song.tracks[frag.track]?.synth ?? 0)"
          />
          <div v-for="(lane, l) in frag.lanes" :key="l" class="lane">
            <span class="pad">{{ padName(lane.pad) }}</span>
            <div class="steps">
              <button
                v-for="(level, s) in lane.steps" :key="s"
                class="step" :class="[`l${level}`, { beat: s % (frag.grid / 4) === 0, now: s === playing(f, lane.steps.length, frag.grid) }]"
                :style="{ '--hit': synthColour(song.tracks[frag.track]?.synth ?? 0) }"
                :title="`${padName(lane.pad)} step ${s + 1}`"
                @click="cycle(f, l, s, level)"
              />
            </div>
          </div>
        </div>
      </div>
      <div class="text">
        <div class="text-head">
          <span v-if="song.error" class="error">line {{ song.error.line }}, col {{ song.error.col }}: {{ song.error.msg }}</span>
          <span v-else-if="dirty" class="muted">applies when you pause · Ctrl+Enter now</span>
        </div>
        <SongEditor :model-value="song.draft" :disabled="!status.running" :error="song.error" @update:model-value="typeSong" @keydown="onKey" />
      </div>
    </div>
  </section>
</template>

<style scoped>
/* The pane never scrolls as a whole: the grid scrolls on the left, and the
   text with its Apply above it holds the right from top to bottom. */
.composer { display: flex; flex-direction: column; min-height: 0; overflow: hidden; }
.notice { margin: 0; color: var(--accent); }
.head-right { display: flex; align-items: center; gap: 12px; }
.ref { color: var(--accent); text-decoration: none; }
.ref:hover { text-decoration: underline; }
.controls { display: flex; align-items: center; gap: 12px; padding: 6px 12px 0; }
.controls .on { border-color: var(--accent); color: var(--accent); }
.field { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
.num { width: 4.5em; }
.body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) minmax(260px, 34%); grid-template-rows: minmax(0, 1fr); gap: 12px; padding: 8px 12px; }
.grid { overflow: auto; display: flex; flex-direction: column; gap: 16px; }
.grid .strip { padding: 0; }
.frag-head { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
.lane { display: flex; align-items: center; gap: 8px; margin: 3px 0; }
.pad { width: 2.2em; font-family: var(--font-mono); color: var(--muted); }
.steps { display: flex; gap: 3px; }
.step { width: 26px; height: 26px; padding: 0; border-radius: 3px; background: var(--panel-2); border: 1px solid var(--line); }
.step.beat { margin-left: 6px; }
.step.l1 { background: color-mix(in srgb, var(--hit) 55%, var(--panel-2)); }
.step.l2 { background: var(--hit); }
.step.l3 { background: color-mix(in srgb, var(--hit) 25%, var(--panel-2)); }
/* A flam and a drag (#353): a hit with one or two grace marks before it. */
.step.l4 { background: color-mix(in srgb, var(--hit) 55%, var(--panel-2)); box-shadow: inset 3px 0 0 var(--hit); }
.step.l5 { background: color-mix(in srgb, var(--hit) 55%, var(--panel-2)); box-shadow: inset 2px 0 0 var(--hit), inset 5px 0 0 var(--panel-2), inset 7px 0 0 var(--hit); }
.step.now { outline: 2px solid var(--accent); outline-offset: 1px; }
.text { display: flex; flex-direction: column; min-height: 0; gap: 6px; }
.text-head { display: flex; align-items: center; gap: 10px; }
.error { color: var(--accent); }
.muted { color: var(--muted); }
</style>
