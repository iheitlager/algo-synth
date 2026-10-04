<script setup lang="ts">
// A note fragment as a piano roll (#168, spec 003 Req 5): rows are pitches,
// columns are sixteenths. A click on empty grid adds a sixteenth, a click on a
// note removes it, and the handle at a note's end drags its length. Every edit
// is a message to the engine, which prints the song back and sends the notes
// (as `setStep` does); nothing here parses or plays. A generator call is
// shown with its notes and a Freeze button, since only written notes edit.
import { computed, ref } from 'vue'
import { addNote, freezeFrag, removeNote, setNoteLength, song, type SongFrag } from '../audio/engine'
import { TICKS_PER_BAR, TICKS_PER_STEP, cellAt, dragLength, isBlack, noteAt, noteBox, noteName, rollRange, stepIn } from '../audio/roll'

const props = defineProps<{ frag: SongFrag; index: number; colour: string }>()

const notes = computed(() => props.frag.notes)
const bars = computed(() => notes.value?.bars ?? 1)
const events = computed(() => notes.value?.events ?? [])
const editable = computed(() => !!notes.value && !notes.value.generated)
const range = computed(() => rollRange(events.value))
const rows = computed(() => {
  const out: number[] = []
  for (let n = range.value.hi; n >= range.value.lo; n--) out.push(n)
  return out
})
const steps = computed(() => bars.value * (TICKS_PER_BAR / TICKS_PER_STEP))
const now = computed(() => (song.playing ? stepIn(song.step, bars.value) : -1))

// A drag in progress: the note and the length it would have.
const drag = ref<{ start: number; note: number; len: number } | null>(null)
const grid = ref<HTMLElement | null>(null)

const boxes = computed(() =>
  events.value.map((e) => {
    const len = drag.value && drag.value.start === e.start && drag.value.note === e.note ? drag.value.len : e.len
    const box = noteBox({ ...e, len }, bars.value, range.value.lo, range.value.hi)
    return {
      e, key: `${e.start}:${e.note}`,
      style: { left: `${box.left}%`, width: `${box.width}%`, top: `${box.top}%`, height: `${box.height}%` },
    }
  }),
)

function point(ev: PointerEvent) {
  const r = grid.value?.getBoundingClientRect()
  if (!r || !r.width || !r.height) return null
  return { fx: (ev.clientX - r.left) / r.width, fy: (ev.clientY - r.top) / r.height }
}

// Empty grid adds a note; a note under the click is removed by its own handler.
function onGrid(ev: PointerEvent) {
  if (!editable.value) return
  const p = point(ev)
  if (!p) return
  const { tick, note } = cellAt(p.fx, p.fy, bars.value, range.value.lo, range.value.hi)
  const hit = noteAt(events.value, tick, note)
  if (hit) removeNote(props.index, hit.start, hit.note)
  else addNote(props.index, tick, note)
}

function startDrag(ev: PointerEvent, start: number, note: number, len: number) {
  if (!editable.value) return
  ;(ev.target as HTMLElement).setPointerCapture(ev.pointerId)
  drag.value = { start, note, len }
}
function moveDrag(ev: PointerEvent) {
  const d = drag.value
  const p = point(ev)
  if (d && p) drag.value = { ...d, len: dragLength(d.start, p.fx, bars.value) }
}
function endDrag() {
  const d = drag.value
  drag.value = null
  const was = events.value.find((e) => d && e.start === d.start && e.note === d.note)
  if (d && was && d.len !== was.len) setNoteLength(props.index, d.start, d.note, d.len)
}
</script>

<template>
  <div class="roll" :style="{ '--hit': colour }">
    <div class="roll-head">
      <code>{{ notes?.text }}</code>
      <span v-if="notes?.live" class="tag">live</span>
      <span v-if="bars > 1" class="muted">{{ bars }} bars</span>
      <button
        v-if="notes?.generated" title="Replace the call with the notes it plays now"
        @click="freezeFrag(index)"
      >Freeze</button>
      <span v-else-if="editable" class="muted">click to add, click a note to remove, drag its end to stretch</span>
    </div>
    <div class="body">
      <div class="keys" aria-hidden="true">
        <span v-for="n in rows" :key="n" :class="{ black: isBlack(n), c: n % 12 === 0 }">{{ n % 12 === 0 || n === range.hi ? noteName(n) : '' }}</span>
      </div>
      <div
        ref="grid" class="grid" :class="{ readonly: !editable }"
        :style="{ '--rows': rows.length, '--steps': steps, '--bars': bars }"
        @pointerdown="onGrid"
      >
        <div v-for="n in rows" :key="n" class="row" :class="{ black: isBlack(n) }" />
        <div v-if="now >= 0" class="now" :style="{ left: `${(100 * now) / steps}%`, width: `${100 / steps}%` }" />
        <div
          v-for="b in boxes" :key="b.key" class="note" :class="{ accent: b.e.accent }" :style="b.style"
          :title="`${noteName(b.e.note)}`" @pointerdown.stop="editable && removeNote(index, b.e.start, b.e.note)"
        >
          <span
            v-if="editable" class="handle" title="Drag to change the length"
            @pointerdown.stop="startDrag($event, b.e.start, b.e.note, b.e.len)"
            @pointermove="moveDrag" @pointerup="endDrag" @pointercancel="endDrag"
          />
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.roll { margin: 4px 0 8px; }
.roll-head { display: flex; align-items: baseline; gap: 10px; margin-bottom: 4px; }
.roll-head code { font-family: var(--font-mono); }
.tag { color: var(--accent); border: 1px solid var(--accent); border-radius: 3px; padding: 0 5px; font-size: 11px; }
.body { display: flex; gap: 6px; }
.keys { display: flex; flex-direction: column; width: 2.6em; font-family: var(--font-mono); font-size: 10px; color: var(--muted); }
.keys span { flex: 1; min-height: 0; line-height: 1; text-align: right; }
.grid {
  position: relative; flex: 1; min-width: calc(var(--steps) * 18px); height: calc(var(--rows) * 14px);
  border: 1px solid var(--line); border-radius: 3px; overflow: hidden; touch-action: none; cursor: crosshair;
  background-image:
    linear-gradient(to right, color-mix(in srgb, var(--line) 90%, transparent) 1px, transparent 1px),
    linear-gradient(to right, color-mix(in srgb, var(--line) 45%, transparent) 1px, transparent 1px);
  background-size: calc(100% / var(--bars)) 100%, calc(100% / (var(--steps) / 4)) 100%;
}
.grid.readonly { cursor: default; }
.row { height: calc(100% / var(--rows)); }
.row.black { background: color-mix(in srgb, var(--panel-2) 70%, transparent); }
.now { position: absolute; top: 0; bottom: 0; background: color-mix(in srgb, var(--accent) 22%, transparent); pointer-events: none; }
.note {
  position: absolute; box-sizing: border-box; border-radius: 2px; cursor: pointer;
  background: color-mix(in srgb, var(--hit) 70%, var(--panel-2)); border: 1px solid var(--hit);
}
.note.accent { background: var(--hit); }
.handle { position: absolute; right: 0; top: 0; bottom: 0; width: 6px; cursor: ew-resize; background: color-mix(in srgb, var(--hit) 100%, #000 25%); }
.muted { color: var(--muted); }
</style>
