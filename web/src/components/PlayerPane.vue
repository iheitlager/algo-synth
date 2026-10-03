<script setup lang="ts">
// The loaded MIDI file (spec 002 Req 9): one row per channel with notes, its
// source, and a piano roll. Drawing and messages only; the engine plays it.
import { MUTE, PLAY, player, route, seek, type Part, type Route } from '../audio/engine'

const choices: { label: string; value: Route }[] = [
  { label: 'Mono', value: PLAY },
  { label: 'Mute', value: MUTE },
]
const colour = (r: Route) => (r === PLAY ? 'var(--mono)' : 'var(--muted)')

// Pitch range of a part, padded so a single note still has height.
function range(p: Part): [number, number] {
  let lo = 127
  let hi = 0
  for (const [, , n] of p.roll) {
    lo = Math.min(lo, n)
    hi = Math.max(hi, n)
  }
  return lo > hi ? [60, 61] : [lo - 1, hi + 2]
}

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
      <span>{{ player.parts.length }} parts · {{ Math.ceil(player.length / player.bar) }} bars</span>
    </div>
    <div v-else class="pane-head"><span>MIDI player</span><span>Demo or Open MIDI… to load a file</span></div>
    <div v-for="p in player.parts" :key="p.channel" class="row">
      <div class="track">
        <b>{{ p.name || `Channel ${p.channel + 1}` }}</b>
        <select :value="p.source" @change="route(p, Number(($event.target as HTMLSelectElement).value) as Route)">
          <option v-for="c in choices" :key="c.value" :value="c.value">{{ c.label }}</option>
        </select>
        <span class="muted">ch {{ p.channel + 1 }} · {{ p.notes }} notes</span>
      </div>
      <div class="lane" @click="onSeek">
        <svg
          :viewBox="`0 ${-range(p)[1]} ${player.length || 1} ${range(p)[1] - range(p)[0]}`"
          preserveAspectRatio="none"
        >
          <rect
            v-for="(n, i) in p.roll" :key="i"
            :x="n[0]" :y="-n[2] - 1" :width="Math.max(n[1] - n[0], 0.02)" height="1"
            :fill="colour(p.source)"
          />
        </svg>
        <div class="head" :style="{ left: `${(player.position / (player.length || 1)) * 100}%` }" />
      </div>
    </div>
    <p v-if="player.error" class="error">{{ player.error }}</p>
  </section>
</template>

<style scoped>
.row { display: grid; grid-template-columns: 180px minmax(0, 1fr); border-top: 1px solid var(--line); }
.track { display: grid; grid-template-columns: 1fr auto; gap: 2px 6px; padding: 6px 10px; align-items: center; }
.track .muted { grid-column: 1 / -1; color: var(--muted); font-size: 10px; }
select { font: inherit; font-size: 11px; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; }
.lane { position: relative; min-height: 44px; cursor: pointer; }
.lane svg { position: absolute; inset: 4px 0; width: 100%; height: calc(100% - 8px); }
.head { position: absolute; top: 0; bottom: 0; width: 1px; background: var(--accent); pointer-events: none; }
.error { color: var(--mono); padding: 0 12px; }
</style>
