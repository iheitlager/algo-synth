<script setup lang="ts">
// The arranger (spec 002 Req 4, ADR-0015, #171): the song's arrangement as a
// timeline. Columns are the entries of `arrange` in order, as wide as their
// bars; rows are the fragments, automation lanes and scenes; a lit cell means
// that section plays it. Every click is a message: the engine changes the song
// and sends it back as text, so the arranger, the composer and the text agree.
// A fragment's row mutes and solos its track's synth (#346): the strip's own
// Mute and Solo, as the synth rail and the mixer show them.
import { computed } from 'vue'
import { MUTE, arrange, files, getEngine, params, song, status, synthColour, synths, type SongSection } from '../audio/engine'
import { Param, type ParamId } from '../audio/params'

const STEPS_PER_BAR = 16
/** Pixels per bar: wide enough to read a name, narrow enough for a song. */
const BAR = 48

/** A row; a fragment's carries the synth its track plays on, or null when muted. */
interface Row { kind: 0 | 1 | 2; item: number; label: string; colour: string; synth?: number | null }
const rows = computed<Row[]>(() => [
  ...song.frags.map((f, i) => {
    const s = song.tracks[f.track]?.synth
    return {
      kind: 0 as const, item: i, label: `${f.name} · ${song.tracks[f.track]?.name ?? ''}`,
      colour: synthColour(s ?? 0), synth: s === undefined || s === MUTE ? null : s,
    }
  }),
  ...song.autos.map((a, i) => ({ kind: 1 as const, item: i, label: `~ ${a}`, colour: 'var(--accent)' })),
  ...song.scenes.map((c, i) => ({ kind: 2 as const, item: i, label: `[${c}]`, colour: 'var(--score)' })),
])

const val = (s: number, id: ParamId) => params.values[s]?.[id] ?? 0
const toggle = (s: number, id: ParamId) => getEngine()?.param(s, id, val(s, id) >= 0.5 ? 0 : 1)
const anySolo = computed(() => synths.list.some((s) => val(s, Param.Solo) >= 0.5))
/** Whether a row's synth is not heard: muted, or another one soloed. */
const silenced = (r: Row) =>
  r.synth != null && (val(r.synth, Param.Mute) >= 0.5 || (anySolo.value && val(r.synth, Param.Solo) < 0.5))

/** The entries with where each starts, in bars. */
const entries = computed(() => {
  let start = 0
  return song.arrange.map((s, place) => {
    const sec: SongSection | undefined = song.sections[s]
    const e = { place, s, sec, start, bars: sec?.bars ?? 0 }
    start += e.bars
    return e
  })
})
const bars = computed(() => entries.value.reduce((n, e) => n + e.bars, 0))
const has = (sec: SongSection | undefined, r: Row) => !!(r.kind === 0 ? sec?.frags : r.kind === 1 ? sec?.autos : sec?.scenes)?.[r.item]
/** Where the song is, in bars, while it plays in the arrangement. */
const head = computed(() => {
  const e = entries.value[song.entry]
  return song.playing && e && song.local >= 0 ? e.start + song.local / STEPS_PER_BAR : -1
})
const looped = (bar: number) => song.loop[0] > 0 && bar + 1 >= song.loop[0] && bar + 1 <= song.loop[1]

// A section that isn't in the arrangement yet can be added at its end.
function addEntry(e: Event) {
  const select = e.target as HTMLSelectElement
  if (select.value !== '') arrange.insert(song.arrange.length, Number(select.value))
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
  <section class="pane arranger" aria-label="Arranger">
    <div class="pane-head">
      <span>Arranger · {{ song.sections.length }} sections · {{ bars }} bars<template v-if="song.loop[0]"> · loop {{ song.loop[0] }}–{{ song.loop[1] }}</template></span>
      <span class="tools">
        <button :disabled="!status.running" title="A new empty section of four bars, at the end" @click="arrange.addSection(4)">+ Section</button>
        <select v-if="song.sections.length" :disabled="!status.running" aria-label="Add a section to the arrangement" @change="addEntry">
          <option value="">+ Play section…</option>
          <option v-for="(s, i) in song.sections" :key="i" :value="i">{{ s.name }} ({{ s.bars }})</option>
        </select>
      </span>
    </div>
    <!-- What opening files reported: a MIDI file imported as the song, a setup's skipped entries. -->
    <p v-if="files.notice" class="notice">{{ files.notice }}</p>
    <p v-if="!song.arrange.length" class="empty">
      No arrangement yet: every fragment loops. <b>+ Section</b> starts one; fragments, automation lanes and scenes are switched on per section below.
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
      <!-- One column head per entry: its section, bars, and moves. -->
      <div class="label">section</div>
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
          <span v-if="r.synth != null" class="ms">
            <button
              class="m" :aria-pressed="val(r.synth, Param.Mute) >= 0.5" :disabled="!status.running"
              :aria-label="`Mute ${song.tracks[song.frags[r.item]?.track ?? 0]?.name}`" title="Mute the track's synth"
              @click="toggle(r.synth, Param.Mute)"
            >M</button>
            <button
              class="s" :aria-pressed="val(r.synth, Param.Solo) >= 0.5" :disabled="!status.running"
              :aria-label="`Solo ${song.tracks[song.frags[r.item]?.track ?? 0]?.name}`" title="Solo the track's synth"
              @click="toggle(r.synth, Param.Solo)"
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
.empty { color: var(--muted); padding: 12px; margin: 0; }
.notice { margin: 4px 12px; color: var(--accent); }
.grid { position: relative; display: grid; grid-template-columns: 170px max-content; row-gap: 2px; padding: 6px 10px 10px; }
.label { font-size: 11px; color: var(--muted); padding: 3px 8px 3px 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; display: flex; align-items: center; gap: 6px; }
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
