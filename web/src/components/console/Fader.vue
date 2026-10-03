<script setup lang="ts">
// A channel fader (#52): drag the cap, wheel, or use the arrow keys; double-
// click puts it back. It holds a position 0..1; `console.ts` turns that into
// dB and into the engine's level. Ticks are drawn at the given dB values.
import { computed, ref } from 'vue'
import { clamp01, dbText, dbToPos, posToDb } from '../../audio/console'

const props = withDefaults(
  defineProps<{ modelValue: number; label: string; def?: number; ticks?: number[]; side?: 'l' | 'r' }>(),
  { def: 1, ticks: () => [0, -6, -12, -24, -40], side: 'l' },
)
const emit = defineEmits<{ 'update:modelValue': [p: number] }>()

const root = ref<HTMLElement | null>(null)
const top = (p: number) => `calc(8px + (100% - 16px) * ${1 - p})`
const marks = computed(() => props.ticks.map((db) => ({ db, top: top(dbToPos(db)), text: db === 0 ? '0' : String(Math.abs(db)) })))
const text = computed(() => `${dbText(posToDb(props.modelValue))} dB`)
const set = (p: number) => emit('update:modelValue', clamp01(p))

let dragging = false
function fromPointer(e: PointerEvent) {
  const r = root.value?.getBoundingClientRect()
  return r ? 1 - (e.clientY - r.top - 8) / (r.height - 16) : props.modelValue
}
function down(e: PointerEvent) {
  root.value?.setPointerCapture(e.pointerId)
  dragging = true
  set(fromPointer(e))
}
function move(e: PointerEvent) {
  if (dragging) set(fromPointer(e))
}
function key(e: KeyboardEvent) {
  const step = e.shiftKey ? 0.1 : 0.02
  if (e.key === 'ArrowUp') set(props.modelValue + step)
  else if (e.key === 'ArrowDown') set(props.modelValue - step)
  else if (e.key === 'Home') set(1)
  else if (e.key === 'End') set(0)
  else return
  e.preventDefault()
}
</script>

<template>
  <div
    ref="root" class="fader" tabindex="0" role="slider" :aria-label="label" aria-valuemin="0" aria-valuemax="100"
    :aria-valuenow="Math.round(modelValue * 100)" :aria-valuetext="text"
    @pointerdown="down" @pointermove="move" @pointerup="dragging = false" @pointercancel="dragging = false"
    @dblclick="set(def)" @wheel.prevent="set(modelValue - Math.sign($event.deltaY) * 0.02)" @keydown="key"
  >
    <div class="slot" />
    <div v-for="m in marks" :key="m.db" class="tick" :class="side" :style="{ top: m.top }"><i /><em>{{ m.text }}</em></div>
    <div class="thumb" :style="{ top: top(modelValue) }" />
  </div>
</template>

<style scoped>
.fader { position: relative; width: 34px; height: 100%; cursor: ns-resize; touch-action: none; }
.slot { position: absolute; left: 50%; top: 8px; bottom: 8px; width: 5px; margin-left: -2.5px; background: var(--con-inset); border-radius: 3px; box-shadow: 0 0 0 1px #2d343e inset, 0 1px 0 #2a303a; }
.tick { position: absolute; left: 0; right: 0; height: 0; display: flex; justify-content: space-between; align-items: center; font: 400 9px var(--con-font-mono); color: var(--con-silk-dim); pointer-events: none; }
.tick i { width: 5px; height: 1px; background: #4a5361; }
.tick em { font-style: normal; }
.tick.l em { order: -1; }
.thumb {
  position: absolute; left: 50%; width: 30px; height: 40px; margin: -20px 0 0 -15px; border-radius: 3px;
  background: linear-gradient(90deg, #14171c 0 8%, #3b424d 8% 46%, #e9e4d6 46% 54%, #3b424d 54% 92%, #14171c 92%);
  box-shadow: 0 5px 8px #000a, 0 1px 0 #566070 inset;
}
.fader:focus-visible { outline: none; }
.fader:focus-visible .thumb { outline: 2px solid var(--con-paper); }
</style>
