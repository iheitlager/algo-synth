<script setup lang="ts">
// The arranger (spec 002 Req 4, ADR-0015, #171): the song's arrangement as a
// timeline. Columns are the entries of `arrange` in order, as wide as their
// bars; rows are the clips, automation lanes and snapshots; a lit cell means
// that scene plays it. Every click is a message: the engine changes the song
// and sends it back as text, so the arranger, the composer and the text agree.
// A clip's row mutes and solos its track (#346, #355): the track's clips
// stop, its synth stays open for live keys and other tracks. The song text
// keeps it (`track … mute`); the synth rail and mixer mute the instrument.
import { computed, ref } from 'vue'
import { MUTE, arrange, files, launch, params, setTrackFlags, song, status, synthColour, synths, type SongScene } from '../audio/engine'
import { Param, Quantize } from '../audio/params'
import LaunchBar from './LaunchBar.vue'

const STEPS_PER_BAR = 16
/** Pixels per bar: wide enough to read a name, narrow enough for a song. */
const BAR = 48
/** Where the bars start: the grid's padding plus its label column, as the CSS has it. */
const GUTTER = 10 + 170

/** A row; a clip's carries its track. */
interface Row { kind: 0 | 1 | 2; item: number; label: string; colour: string; track?: number }
const rows = computed<Row[]>(() => [
  ...song.clips.map((f, i) => {
    const s = song.tracks[f.track]?.synth
    return {
      kind: 0 as const, item: i, label: `${f.name} · ${song.tracks[f.track]?.name ?? ''}`,
      colour: synthColour(s === undefined || s === MUTE ? 0 : s), track: f.track,
    }
  }),
  ...song.autos.map((a, i) => ({ kind: 1 as const, item: i, label: `~ ${a}`, colour: 'var(--accent)' })),
  ...song.snapshots.map((c, i) => ({ kind: 2 as const, item: i, label: `[${c}]`, colour: 'var(--score)' })),
])

const muted = (t: number) => song.tracks[t]?.mute ?? false
const soloed = (t: number) => song.tracks[t]?.solo ?? false
const toggleMute = (t: number) => setTrackFlags(t, !muted(t), soloed(t))
const toggleSolo = (t: number) => setTrackFlags(t, muted(t), !soloed(t))
const anySolo = computed(() => song.tracks.some((t) => t.solo))
// The engine reports the live strip values, automation included, so a lane that
// mutes an instrument greys its rows here too (#346).
const val = (s: number, id: number) => params.values[s]?.[id] ?? 0
/** The synth a track plays, when it has a real one. */
const synthOf = (t: number) => {
  const s = song.tracks[t]?.synth
  return s === undefined || s === MUTE ? undefined : s
}
const anySynthSolo = computed(() => synths.list.some((s) => val(s, Param.Solo) >= 0.5))
/** Whether a track's instrument is silent: muted, or unsoloed while another solos. */
const instrumentSilent = (t: number) => {
  const s = synthOf(t)
  return s !== undefined && (val(s, Param.Mute) >= 0.5 || (anySynthSolo.value && val(s, Param.Solo) < 0.5))
}
/** Whether a clip row's track is not heard, as the engine has it: while
 * any track is soloed only the soloed ones play, else the ones not muted; and
 * either way nothing is heard through a silenced instrument. */
const silenced = (r: Row) =>
  r.track != null && ((anySolo.value ? !soloed(r.track) : muted(r.track)) || instrumentSilent(r.track))

/** The entries with where each starts, in bars. */
const entries = computed(() => {
  let start = 0
  return song.arrange.map((s, place) => {
    const sec: SongScene | undefined = song.scenes[s]
    const e = { place, s, sec, start, bars: sec?.bars ?? 0 }
    start += e.bars
    return e
  })
})
const bars = computed(() => entries.value.reduce((n, e) => n + e.bars, 0))
const has = (sec: SongScene | undefined, r: Row) => !!(r.kind === 0 ? sec?.clips : r.kind === 1 ? sec?.autos : sec?.snapshots)?.[r.item]
/** Where the song is, in bars, while it plays in the arrangement. */
const head = computed(() => {
  const e = entries.value[song.entry]
  return song.playing && e && song.local >= 0 ? e.start + song.local / STEPS_PER_BAR : -1
})
const looped = (bar: number) => song.loop[0] > 0 && bar + 1 >= song.loop[0] && bar + 1 <= song.loop[1]
// A long arrangement scrolls past the playhead; this brings it back in view.
const pane = ref<HTMLElement>()
function goToHead() {
  const el = pane.value
  if (!el || head.value < 0) return
  // Centred in what the pinned label column leaves visible.
  el.scrollTo({ left: Math.max(0, head.value * BAR - (el.clientWidth - GUTTER) / 2), behavior: 'smooth' })
}

// Play a scene now, in place of the arrangement, as its Launch pad with Shift does.
function playScene(e: Event) {
  const select = e.target as HTMLSelectElement
  if (select.value !== '') launch.scene(Number(select.value), Quantize.Now)
  select.value = ''
}
function setBars(s: number, e: Event) {
  const v = Math.round(Number((e.target as HTMLInputElement).value))
  if (v >= 1 && v <= 256) arrange.setBars(s, v)
}
// Click a bar to go there; shift-click two bars to loop them, shift-click inside the loop to clear it.
let loopStart = 0
function onBar(bar: number, e: MouseEvent) {
  if (!e.shiftKey) return arrange.seekBar(bar)
  if (looped(bar) && !loopStart) return arrange.loop(0, 0)
  if (!loopStart) {
    loopStart = bar + 1
    return
  }
  const [a, b] = [loopStart, bar + 1].sort((x, y) => x - y) as [number, number]
  loopStart = 0
  arrange.loop(a, b)
}
</script>

<template>
  <section ref="pane" class="pane arranger" aria-label="Arranger">
    <div class="pane-head">
      <span>Arranger · {{ song.scenes.length }} scenes · {{ bars }} bars<template v-if="song.loop[0]"> · loop {{ song.loop[0] }}–{{ song.loop[1] }}</template></span>
      <span class="tools">
        <select v-if="song.scenes.length" :disabled="!status.running" aria-label="Play a scene now" @change="playScene">
          <option value="">▶ Play scene…</option>
          <option v-for="(s, i) in song.scenes" :key="i" :value="i">{{ s.name }} ({{ s.bars }})</option>
        </select>
        <button :disabled="head < 0" title="Scroll to the bar playing now" @click="goToHead">◎ Now</button>
      </span>
    </div>
    <!-- Launch scenes and snapshots live (#489): pads and keys. -->
    <LaunchBar />
    <!-- What opening files reported: a MIDI file imported as the song, a setup's skipped entries. -->
    <p v-if="files.notice" class="notice">{{ files.notice }}</p>
    <p v-if="!song.arrange.length" class="empty">
      No arrangement yet: every clip loops. Write an <code>arrange</code> line in the song, or record one with <b>Rec</b> in Launch; clips, automation lanes and snapshots are switched on per scene below.
    </p>
    <div v-else class="grid" :style="{ '--bar': `${BAR}px` }">
      <!-- The ruler: click a bar to go there, shift-click two to loop them. -->
      <div class="label ruler-label">bar</div>
      <div class="track ruler" :style="{ width: `${bars * BAR}px` }">
        <button
          v-for="b in bars" :key="b" class="bar" :class="{ loop: looped(b - 1) }" :style="{ left: `${(b - 1) * BAR}px` }"
          :title="`Bar ${b}: click to go there, shift-click to loop`" @click="onBar(b - 1, $event)"
        >{{ b }}</button>
      </div>
      <!-- One column head per entry: its scene, bars, and moves. -->
      <div class="label">scene</div>
      <div class="track" :style="{ width: `${bars * BAR}px` }">
        <div
          v-for="e in entries" :key="e.place" class="head" :class="{ now: song.playing && song.entry === e.place }"
          :style="{ left: `${e.start * BAR}px`, width: `${e.bars * BAR}px` }"
        >
          <b :title="e.sec?.name">{{ e.sec?.name }}</b>
          <input type="number" min="1" max="256" :value="e.bars" :aria-label="`${e.sec?.name} bars`" @change="setBars(e.s, $event)" />
          <span class="moves">
            <button :disabled="e.place === 0" title="Earlier" @click="arrange.move(e.place, e.place - 1)">◂</button>
            <button :disabled="e.place === entries.length - 1" title="Later" @click="arrange.move(e.place, e.place + 1)">▸</button>
            <button title="Take this entry out of the arrangement" @click="arrange.remove(e.place)">×</button>
          </span>
        </div>
      </div>
      <template v-for="r in rows" :key="`${r.kind}:${r.item}`">
        <div class="label" :class="{ silenced: silenced(r) }" :title="r.label">
          <span v-if="r.track != null" class="ms">
            <button
              class="m" :aria-pressed="muted(r.track)" :disabled="!status.running"
              :aria-label="`Mute ${song.tracks[r.track]?.name}`" title="Mute the track: its clips stop, its synth plays on"
              @click="toggleMute(r.track)"
            >M</button>
            <button
              class="s" :aria-pressed="soloed(r.track)" :disabled="!status.running"
              :aria-label="`Solo ${song.tracks[r.track]?.name}`" title="Solo the track: only soloed tracks play"
              @click="toggleSolo(r.track)"
            >S</button>
          </span>
          <i :style="{ background: r.colour }" /><span class="name">{{ r.label }}</span>
        </div>
        <div class="track" :class="{ silenced: silenced(r) }" :style="{ width: `${bars * BAR}px` }">
          <button
            v-for="e in entries" :key="e.place" class="cell" :class="{ on: has(e.sec, r) }"
            :style="{ left: `${e.start * BAR}px`, width: `${e.bars * BAR}px`, '--c': r.colour }"
            :aria-pressed="has(e.sec, r)" :title="`${r.label} in ${e.sec?.name} (every ${e.sec?.name}): click to switch`"
            @click="arrange.toggle(e.s, r.kind, r.item)"
          />
        </div>
      </template>
      <i v-if="head >= 0" class="playhead" :style="{ transform: `translateX(${head * BAR}px)` }" aria-hidden="true" />
    </div>
  </section>
</template>

<style scoped>
.tools { display: flex; gap: 6px; align-items: center; text-transform: none; letter-spacing: 0; }
.tools button.on { border-color: var(--accent); color: var(--accent); }
.tools select { font: inherit; font-size: 11px; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; }
/* Scrolling sideways keeps the head (and ◎ Now) and the row labels in place;
   the bars slide under the labels. */
.arranger > .pane-head { left: 0; }
.empty { color: var(--muted); padding: 12px; margin: 0; }
.notice { margin: 4px 12px; color: var(--accent); }
.grid { position: relative; display: grid; grid-template-columns: 170px max-content; row-gap: 2px; padding: 6px 10px 10px; }
.label { position: sticky; left: 0; z-index: 1; margin-left: -10px; padding-left: 10px; background: var(--panel); font-size: 11px; color: var(--muted); padding-top: 3px; padding-right: 8px; padding-bottom: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: flex; align-items: center; gap: 6px; }
.label i { width: 8px; height: 8px; border-radius: 2px; flex: none; }
.ms { display: flex; gap: 2px; flex: none; }
.ms button {
  width: 16px; height: 16px; padding: 0; border-radius: 3px; font: 700 9px/1 var(--font-mono);
  color: var(--muted); background: var(--panel-2); border: 1px solid var(--line);
}
.ms .m[aria-pressed='true'] { background: #7a2a22; color: #fff; border-color: #e0654f; box-shadow: 0 0 6px #e0654f55; }
.ms .s[aria-pressed='true'] { background: #8a6a10; color: #fff; border-color: #f0c23a; box-shadow: 0 0 6px #f0c23a55; }
.label .name { overflow: hidden; text-overflow: ellipsis; }
.label.silenced > i, .label.silenced > .name, .track.silenced { opacity: 0.4; }
.track { position: relative; height: 22px; }
.ruler { height: 18px; }
.bar { position: absolute; top: 0; width: var(--bar); height: 18px; padding: 0; border: 0; border-left: 1px solid var(--line); border-radius: 0; background: none; font-size: 9px; color: var(--muted); text-align: left; padding-left: 3px; }
.bar.loop { background: color-mix(in srgb, var(--accent) 25%, transparent); color: var(--text); }
.head { position: absolute; top: 0; height: 22px; display: flex; align-items: center; gap: 3px; padding: 0 3px; background: var(--panel-2); border: 1px solid var(--line); border-radius: 3px; overflow: hidden; }
.head.now { border-color: var(--accent); }
.head b { font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 2.5em; flex: 1; }
.head input { width: 34px; flex: none; font: inherit; font-size: 10px; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 2px; }
.moves { display: none; position: absolute; right: 1px; top: 1px; bottom: 1px; background: var(--panel-2); border-radius: 2px; }
.head:hover .moves, .moves:focus-within { display: flex; }
.moves button { padding: 0 3px; font-size: 10px; border: 0; background: none; }
.cell { position: absolute; top: 1px; height: 20px; padding: 0; border: 1px dashed var(--line); border-radius: 3px; background: transparent; }
.cell.on { border: 1px solid var(--c); background: color-mix(in srgb, var(--c) 45%, transparent); }
.playhead { position: absolute; top: 6px; bottom: 10px; left: calc(10px + 170px); width: 0; border-left: 2px solid var(--accent); pointer-events: none; will-change: transform; }
</style>
