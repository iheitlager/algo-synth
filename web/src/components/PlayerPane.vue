<script setup lang="ts">
// The loaded MIDI file (spec 002 Req 9): one row per channel with notes, its
// source, and a piano roll. Drawing and messages only; the engine plays it.
import { computed } from 'vue'
import { MUTE, importMidi, params, partName, player, route, seek, stripName, synthColour, synths, type Route } from '../audio/engine'
import { modelDef } from '../audio/models'
import { renamePart } from '../audio/names'
import { Param } from '../audio/params'
import EditableName from './EditableName.vue'
import PianoRoll from './PianoRoll.vue'
import Playhead from './Playhead.vue'

// Each part plays on one of the shown synths, or is muted.
const choices = computed<{ label: string; value: Route }[]>(() => [
  ...synths.list.map((s) => ({ label: `${stripName(s)} (${modelDef(params.values[s]?.[Param.Model] ?? 0).name})`, value: s })),
  { label: 'Mute', value: MUTE },
])
const colour = (r: Route) => (r === MUTE ? 'var(--muted)' : synthColour(r))

function onSeek(e: MouseEvent) {
  const el = e.currentTarget as HTMLElement
  const r = el.getBoundingClientRect()
  seek(((e.clientX - r.left) / r.width) * player.length)
}
</script>

<template>
  <section class="pane">
    <div v-if="player.loaded" class="pane-head">
      <span>{{ player.fileName }}</span>
      <span class="head-right">
        {{ player.parts.length }} parts · {{ Math.ceil(player.length / player.bar) }} bars
        <button title="Convert this file into the song: tracks, timed-note fragments and sections" @click="importMidi">Import as song</button>
      </span>
    </div>
    <div v-else class="pane-head"><span>MIDI player</span><span>Demo or Open… to load a file and its setup</span></div>
    <p v-if="player.notice" class="notice">{{ player.notice }}</p>
    <div class="rows">
      <div v-for="p in player.parts" :key="p.channel" class="row">
        <div class="track">
          <b><EditableName :value="partName(p)" :label="partName(p)" @rename="renamePart(p.channel, $event)" /></b>
          <select :value="p.synth" @change="route(p, Number(($event.target as HTMLSelectElement).value) as Route)">
            <option v-for="c in choices" :key="c.value" :value="c.value">{{ c.label }}</option>
          </select>
          <span class="muted">ch {{ p.channel + 1 }} · {{ p.notes }} notes</span>
        </div>
        <div class="lane" @click="onSeek">
          <PianoRoll :roll="p.roll" :length="player.length" :colour="colour(p.synth)" />
        </div>
      </div>
      <Playhead v-if="player.loaded" />
    </div>
    <p v-if="player.error" class="error">{{ player.error }}</p>
  </section>
</template>

<style scoped>
.row { display: grid; grid-template-columns: 180px minmax(0, 1fr); border-top: 1px solid var(--line); }
.track { display: grid; grid-template-columns: 1fr auto; gap: 2px 6px; padding: 6px 10px; align-items: center; }
.track b { min-width: 0; display: block; }
.track .muted { grid-column: 1 / -1; color: var(--muted); font-size: 10px; }
select { font: inherit; font-size: 11px; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; }
.lane { position: relative; min-height: 44px; cursor: pointer; }
.lane svg { position: absolute; inset: 4px 0; width: 100%; height: calc(100% - 8px); }
.rows { position: relative; }
.head-right { display: flex; gap: 8px; align-items: center; }
.head-right button { font-size: 11px; padding: 2px 8px; text-transform: none; letter-spacing: 0; }
.error { color: var(--mono); padding: 0 12px; }
.notice { color: var(--muted); padding: 4px 12px; margin: 0; font-size: 11px; }
</style>
