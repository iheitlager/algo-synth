<script setup lang="ts">
// The composer (spec 003 Req 5, ADR-0012): the song's drum clips as a step
// grid and its note clips as piano rolls beside the song's text (SongEditor). The engine holds the song: a click sends
// `setStep`, an edited text is sent to be parsed, and both redraw from what the
// engine sends back. Nothing here parses or plays.
import { LIMITS, setSplit, splits } from '../audio/split'
import Splitter from './Splitter.vue'
import { computed, onMounted, watch } from 'vue'
import {
  MUTE, cueClip, loadSong, nextRatchet, params, requestSong, routeTrack, typeSong, setRatchet, setSongSwing, setSongTempo, setStep, song, songPosition as position,
  status, stripName, synthColour, synths, type Route,
} from '../audio/engine'
import { modelDef } from '../audio/models'
import { Model, Pad, Param } from '../audio/params'
import NoteRoll from './NoteRoll.vue'
import SongEditor from './SongEditor.vue'
import TrackStrip from './TrackStrip.vue'

const SONG_REFERENCE = 'https://github.com/iheitlager/algo-synth/blob/main/.openspec/language.md'

// A first beat to start from when the song is empty.
const STARTER = `tempo 120
swing 50
track kit drums

clip beat = kit /16
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
// The words playing light up (#205) only while the editor shows the text the
// engine plays: its spans point into that text, not into a draft.

// Off → hit → accent → off.
// A click cycles off → hit → accent → off; a ghost, flam or drag (written in the text) clicks off.
const cycle = (f: number, l: number, s: number, level: number) => setStep(f, l, s, level < 2 ? level + 1 : 0)
// Shift-click cycles a sounding step's ratchet, 1 → 2 → 3 → 4 → 1 hits in its span (#242).
const click = (e: MouseEvent, f: number, l: number, s: number, level: number, r: number) =>
  e.shiftKey ? setRatchet(f, l, s, nextRatchet(level, r)) : cycle(f, l, s, level)
// The scene playing now, in an arrangement.
const scene = computed(() => (song.entry >= 0 ? song.scenes[song.arrange[song.entry]] : undefined))
// The step a lane plays now, looping on its own length; in an arrangement it counts from the scene's start,
// and a clip the scene does not play has none.
// The lane step under the clock: a lane of `grid` steps a bar (#353) moves
// grid/16 steps per clock step.
const playing = (f: number, len: number, grid: number) => {
  // While a clip is cued (#375), only it plays.
  if (song.cued >= 0 && song.cued !== f) return -1
  if (song.entry >= 0 && !scene.value?.clips[f]) return -1
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
        <span v-else>{{ song.clips.length }} {{ song.clips.length === 1 ? 'clip' : 'clips' }}</span>
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
      <span v-if="song.cued >= 0" class="cue-note">clip {{ song.clips[song.cued]?.name }} alone</span>
    </div>
    <div class="body" :style="{ '--code': splits.code != null ? `${splits.code}px` : undefined }">
      <div class="grid">
        <p v-if="noKit" class="notice">No synth is a drum kit: add one with + Synth › Drums, then pick it for the track.</p>
        <TrackStrip v-if="status.running && song.tracks.length" class="strip" :synths="choices" />
        <p v-if="status.running && !song.clips.length" class="muted">
          The song has no clips yet.
          <button @click="loadSong(STARTER)">Start a beat</button>
        </p>
        <div v-for="(clip, f) in song.clips" :key="f" class="clip" :class="{ cued: song.cued === f }">
          <div class="clip-head">
            <!-- Play this clip alone, looping; again, or Stop, goes back to the song (#375). -->
            <button
              class="cue" :aria-pressed="song.cued === f" :disabled="!status.running"
              :aria-label="song.cued === f ? `Stop clip ${clip.name}` : `Play clip ${clip.name} alone`"
              :title="song.cued === f ? 'Stop this clip' : 'Play this clip alone, looping'"
              @click="cueClip(song.cued === f ? -1 : f)"
            >{{ song.cued === f ? '■' : '▶' }}</button>
            <b>{{ clip.name }}</b>
            <span class="muted">on {{ song.tracks[clip.track]?.name }}</span>
            <select
              class="picker" :style="{ '--c': synthColour(song.tracks[clip.track]?.synth ?? 0) }"
              :value="song.tracks[clip.track]?.synth ?? MUTE"
              @change="routeTrack(clip.track, Number(($event.target as HTMLSelectElement).value) as Route)"
            >
              <option v-for="c in choices" :key="c.value" :value="c.value">{{ c.label }}</option>
            </select>
          </div>
          <NoteRoll
            v-if="clip.notes" :clip="clip" :index="f"
            :colour="synthColour(song.tracks[clip.track]?.synth ?? 0)"
          />
          <div v-for="(lane, l) in clip.lanes" :key="l" class="lane">
            <span class="pad">{{ padName(lane.pad) }}</span>
            <div class="steps">
              <button
                v-for="(level, s) in lane.steps" :key="s"
                class="step" :class="[`l${level}`, { beat: s % (clip.grid / 4) === 0, now: s === playing(f, lane.steps.length, clip.grid) }]"
                :style="{ '--hit': synthColour(song.tracks[clip.track]?.synth ?? 0) }"
                :title="`${padName(lane.pad)} step ${s + 1}${(lane.ratchets[s] ?? 1) > 1 ? `, ${lane.ratchets[s]} hits` : ''} (shift-click: ratchet)`"
                @click="click($event, f, l, s, level, lane.ratchets[s] ?? 1)"
              >{{ (lane.ratchets[s] ?? 1) > 1 ? lane.ratchets[s] : '' }}</button>
            </div>
          </div>
        </div>
      </div>
      <Splitter
        between="columns" :size="splits.code" :min="LIMITS.code" label="Width of the song text"
        @resize="(v) => setSplit('code', v)"
      />
      <div class="text">
        <div class="text-head">
          <span v-if="song.error" class="error">line {{ song.error.line }}, col {{ song.error.col }}: {{ song.error.msg }}</span>
          <span v-else-if="dirty" class="muted">applies when you pause · Ctrl+Enter now</span>
        </div>
        <SongEditor
          :model-value="song.draft" :disabled="!status.running" :error="song.error"
          :lit="song.draft === song.text ? song.lit : []" @update:model-value="typeSong" @keydown="onKey"
        />
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
.body { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) 6px var(--code, minmax(260px, 34%)); grid-template-rows: minmax(0, 1fr); gap: 6px; padding: 8px 12px; }
.grid { overflow: auto; display: flex; flex-direction: column; gap: 16px; }
.grid .strip { padding: 0; }
.clip-head { display: flex; align-items: center; gap: 10px; margin-bottom: 6px; }
/* A clip playing alone (#375): its button lit and the clip outlined. */
.clip.cued { outline: 1px solid var(--accent); outline-offset: 4px; border-radius: 4px; }
.cue { width: 26px; height: 22px; padding: 0; line-height: 1; }
.cue[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
.cue-note { color: var(--accent); }
.lane { display: flex; align-items: center; gap: 8px; margin: 3px 0; }
.pad { width: 2.2em; font-family: var(--font-mono); color: var(--muted); }
.steps { display: flex; gap: 3px; }
.step { width: 26px; height: 26px; padding: 0; border-radius: 3px; background: var(--panel-2); border: 1px solid var(--line); font-size: 11px; font-weight: 600; color: var(--text); }
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
