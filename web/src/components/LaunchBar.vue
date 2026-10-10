<script setup lang="ts">
// Launch (#489, epic #491): Ableton's Session view in the song's words. A pad
// per scene launches it in place of the arrangement, looping until the next;
// a pad per snapshot switches it on; both land at the moment chosen, Shift or
// the Now pad at once. The keys work from every view: 1–0 scenes, z–m
// snapshots, [ ] the moment, \ back to the arrangement, Esc cancel, Space
// play/stop. Rec writes the scenes launched as the `arrange` line.
import { computed, onBeforeUnmount, onMounted, reactive, watch } from 'vue'
import { arrange, launch, playSong, song, status, stopSong } from '../audio/engine'
import {
  QUANTIZES, QUANTIZE_NAMES, SCENE_KEYS, SNAPSHOT_KEYS, arrangeOps, keyAction, keyLabel, landsIn,
  launchState, newRec, recordStep, stepQuantize,
} from '../audio/launch'
import { Quantize } from '../audio/params'

const when = (now: boolean) => (now ? Quantize.Now : launchState.when)
const playScene = (i: number, now = false) => launch.scene(i, when(now))
const playSnapshot = (i: number, now = false) => launch.snapshot(i, when(now))
const resume = (now = false) => launch.resume(when(now))

/** What waits to land, in words. */
const waiting = computed(() => {
  if (song.queued === -1) return ''
  const what = song.queued === -2 ? 'the arrangement' : (song.scenes[song.queued]?.name ?? '')
  const at = song.queuedWhen === Quantize.End ? 'at the end of the scene' : `in ${landsIn(song.launchIn)}`
  return `→ ${what} ${at}`
})

// Rec: each scene each time it starts, written as `arrange` when Rec stops.
const rec = reactive(newRec())
watch(() => [song.launched, song.local] as const, ([l, local]) => recordStep(rec, l, local))
function toggleRec() {
  if (!rec.on) {
    Object.assign(rec, newRec(), { on: true })
    recordStep(rec, song.launched, song.local)
    return
  }
  rec.on = false
  if (!rec.entries.length) return
  for (const o of arrangeOps(song.arrange.length, rec.entries)) {
    if (o.op === 'remove') arrange.remove(o.at)
    else arrange.insert(o.at, o.scene)
  }
}

function onKey(e: KeyboardEvent) {
  const a = keyAction(e)
  if (!a || !status.running) return
  switch (a.kind) {
    case 'scene':
      if (a.index >= song.scenes.length) return
      playScene(a.index, a.now)
      break
    case 'snapshot':
      if (a.index >= song.snapshots.length) return
      playSnapshot(a.index, a.now)
      break
    case 'quantize': launchState.when = stepQuantize(launchState.when, a.by); break
    case 'resume':
      if (song.launched < 0) return
      resume()
      break
    case 'cancel':
      // Esc closes popups too: only take it when something waits.
      if (song.queued === -1) return
      launch.cancel()
      break
    case 'transport': if (song.playing) stopSong(); else playSong(); break
  }
  e.preventDefault()
}
onMounted(() => window.addEventListener('keydown', onKey))
onBeforeUnmount(() => window.removeEventListener('keydown', onKey))
</script>

<template>
  <div v-if="song.scenes.length" class="launch" role="group" aria-label="Launch">
    <div class="row">
      <span class="title">Launch</span>
      <button
        v-for="(s, i) in song.scenes" :key="`s${i}`" class="pad"
        :class="{ playing: song.launched === i, queued: song.queued === i }" :disabled="!status.running"
        :aria-pressed="song.launched === i" :aria-label="`Launch scene ${s.name}`"
        :title="`Launch ${s.name} (${s.bars} bars)${SCENE_KEYS[i] ? `, key ${keyLabel(SCENE_KEYS[i])}` : ''}; Shift: now`"
        @click="playScene(i, $event.shiftKey)"
      >
        <kbd v-if="SCENE_KEYS[i]">{{ keyLabel(SCENE_KEYS[i]) }}</kbd>{{ s.name }}
      </button>
      <button
        class="pad back" :class="{ queued: song.queued === -2 }" :disabled="!status.running || song.launched < 0"
        title="Back to the arrangement, where the clock is; key \" @click="resume($event.shiftKey)"
      >
        <kbd>\</kbd>↩ Arrangement
      </button>
    </div>
    <div class="row">
      <span class="title">When</span>
      <span class="seg" role="radiogroup" aria-label="When a launch lands">
        <button
          v-for="q in QUANTIZES" :key="q" role="radio" :aria-checked="launchState.when === q"
          :title="`Land on ${QUANTIZE_NAMES[q]}; keys [ and ]`" @click="launchState.when = q"
        >
          {{ QUANTIZE_NAMES[q] }}
        </button>
      </span>
      <template v-if="song.snapshots.length">
        <span class="title">Snapshot</span>
        <button
          v-for="(n, i) in song.snapshots" :key="`n${i}`" class="pad snap" :class="{ queued: song.snapshotQueued === i }"
          :disabled="!status.running" :aria-label="`Switch snapshot ${n}`"
          :title="`Switch ${n} on${SNAPSHOT_KEYS[i] ? `, key ${keyLabel(SNAPSHOT_KEYS[i])}` : ''}; Shift: now`"
          @click="playSnapshot(i, $event.shiftKey)"
        >
          <kbd v-if="SNAPSHOT_KEYS[i]">{{ keyLabel(SNAPSHOT_KEYS[i]) }}</kbd>[{{ n }}]
        </button>
      </template>
      <span class="waiting" aria-live="polite">{{ waiting }}</span>
      <button
        class="rec" :aria-pressed="rec.on" :disabled="!status.running"
        :title="rec.on ? 'Stop: write the scenes launched as the arrangement' : 'Record the scenes you launch into the arrangement'"
        @click="toggleRec"
      >
        ⏺ Rec<template v-if="rec.on"> · {{ rec.entries.length }}</template>
      </button>
    </div>
  </div>
</template>

<style scoped>
.launch { display: grid; gap: 4px; padding: 6px 10px 4px; border-bottom: 1px solid var(--line); }
.row { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
.title { width: 64px; font-size: 10px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--muted); }
.pad {
  display: inline-flex; align-items: center; gap: 5px; padding: 2px 8px 2px 4px; font-size: 11px;
  background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px;
}
.pad kbd {
  min-width: 14px; padding: 0 3px; font: 600 9px/14px var(--font-mono); text-align: center;
  color: var(--muted); border: 1px solid var(--line); border-radius: 3px;
}
.pad.playing { border-color: #4caf50; background: color-mix(in srgb, #4caf50 30%, var(--panel-2)); color: #fff; }
.pad.queued { border-color: var(--accent); animation: blink 0.5s steps(2, start) infinite; }
.pad.snap { color: var(--score); }
@keyframes blink { to { background: color-mix(in srgb, var(--accent) 35%, var(--panel-2)); } }
@media (prefers-reduced-motion: reduce) { .pad.queued { animation: none; background: color-mix(in srgb, var(--accent) 25%, var(--panel-2)); } }
.seg { display: inline-flex; border: 1px solid var(--line); border-radius: 4px; overflow: hidden; }
.seg button { border: 0; border-radius: 0; padding: 2px 8px; font-size: 11px; background: transparent; }
.seg button + button { border-left: 1px solid var(--line); }
.seg button[aria-checked='true'] { background: var(--accent); color: #000; }
.waiting { flex: 1; min-width: 8em; font-size: 11px; color: var(--accent); }
.rec[aria-pressed='true'] { border-color: #e0654f; color: #e0654f; }
</style>
