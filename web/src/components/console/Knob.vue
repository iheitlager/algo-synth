<script setup lang="ts">
// A rotary knob (#52). Drag up or down to turn it, click for a slider, double-
// click to reset, wheel or arrow keys to nudge; shift makes it finer. The knob
// holds no value of its own: it shows `modelValue` (0..1) and emits changes.
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { arcPath, clamp01, dragValue, knobAngle, knobArc, SWEEP } from '../../audio/console'
import { closePop, pop, showPop, updatePop } from './pop'

const props = withDefaults(
  defineProps<{
    modelValue: number
    label: string
    /** The accessible name, when `label` is short or empty. */
    name?: string
    /** Where double-click puts it. */
    def?: number
    size?: number
    color?: string
    bipolar?: boolean
    /** Hide the readout under the label. */
    noVal?: boolean
    fmt?: (v: number) => string
    /** A modulation of the song drives it (ADR-0019): drawn with a ring. */
    modulated?: boolean
  }>(),
  { size: 38, color: 'var(--con-paper)', def: 0, fmt: (v: number) => `${Math.round(v * 100)}%` },
)
const emit = defineEmits<{ 'update:modelValue': [v: number] }>()

const id = Symbol('knob')
const root = ref<HTMLElement | null>(null)
const live = ref(false)
const c = computed(() => props.size / 2)
const r = computed(() => c.value - 3)
const track = computed(() => arcPath(c.value, c.value, r.value, -SWEEP, SWEEP))
const value = computed(() => knobArc(c.value, c.value, r.value, props.modelValue, props.bipolar))
const angle = computed(() => knobAngle(props.modelValue))
const text = computed(() => props.fmt(props.modelValue))
const set = (v: number) => emit('update:modelValue', clamp01(v))

watch(() => props.modelValue, (v) => updatePop(id, props.fmt(v), v))
const popOptions = (interactive: boolean) => ({
  label: props.name ?? props.label, color: props.color, text: text.value, value: props.modelValue, interactive, apply: set,
})

let drag: { y: number; v: number; moved: boolean } | null = null
function down(e: PointerEvent) {
  if (e.button !== 0) return
  root.value?.setPointerCapture(e.pointerId)
  drag = { y: e.clientY, v: props.modelValue, moved: false }
}
function move(e: PointerEvent) {
  if (!drag) return
  const dy = drag.y - e.clientY
  if (!drag.moved && Math.abs(dy) < 3) return
  if (!drag.moved && root.value) {
    drag.moved = true
    live.value = true
    showPop(id, root.value, popOptions(false))
  }
  set(dragValue(drag.v, dy, e.shiftKey))
}
function up() {
  if (!drag) return
  const moved = drag.moved
  drag = null
  live.value = false
  if (moved) closePop()
  else if (root.value) showPop(id, root.value, popOptions(true))
}
function cancel() {
  drag = null
  live.value = false
  if (pop.owner === id) closePop()
}
function reset() {
  closePop()
  set(props.def)
}
function wheel(e: WheelEvent) {
  set(props.modelValue - Math.sign(e.deltaY) * (e.shiftKey ? 0.004 : 0.02))
}
function key(e: KeyboardEvent) {
  const step = e.shiftKey ? 0.1 : 0.02
  if (e.key === 'ArrowUp' || e.key === 'ArrowRight') set(props.modelValue + step)
  else if (e.key === 'ArrowDown' || e.key === 'ArrowLeft') set(props.modelValue - step)
  else if (e.key === 'Home') set(0)
  else if (e.key === 'End') set(1)
  else return
  e.preventDefault()
}
onBeforeUnmount(() => { if (pop.owner === id) closePop() })
</script>

<template>
  <div
    ref="root" class="knob" :class="{ live, modulated }" data-knob tabindex="0" role="slider" :aria-label="name ?? label"
    :title="modulated ? 'Modulated by the song' : undefined"
    aria-valuemin="0" aria-valuemax="100" :aria-valuenow="Math.round(modelValue * 100)" :aria-valuetext="text"
    @pointerdown="down" @pointermove="move" @pointerup="up" @pointercancel="cancel" @dblclick="reset"
    @wheel.prevent="wheel" @keydown="key"
  >
    <svg :width="size" :height="size" :viewBox="`0 0 ${size} ${size}`" aria-hidden="true">
      <circle
        v-if="modulated" class="mod-ring" :cx="c" :cy="c" :r="r + 2.5" fill="none" stroke="var(--con-led-a)"
        stroke-width="1" stroke-dasharray="2 2"
      />
      <path :d="track" fill="none" stroke="#0b0d10" stroke-width="3.5" stroke-linecap="round" />
      <path :d="value" fill="none" :stroke="color" stroke-width="3.5" stroke-linecap="round" />
      <circle :cx="c" :cy="c" :r="r - 6" fill="url(#con-cap)" stroke="#07080a" stroke-width="1" />
      <circle :cx="c" :cy="c" :r="r - 8" fill="none" stroke="#4a5361" stroke-width="0.6" opacity="0.7" />
      <line
        :x1="c" :y1="c - (r - 9)" :x2="c" :y2="c - 3" stroke="#e9e4d6" stroke-width="2" stroke-linecap="round"
        :transform="`rotate(${angle} ${c} ${c})`"
      />
    </svg>
    <div v-if="label" class="lab">{{ label }}</div>
    <div v-if="!noVal" class="val">{{ text }}</div>
  </div>
</template>

<style scoped>
.knob { display: flex; flex-direction: column; align-items: center; gap: 1px; cursor: ns-resize; touch-action: none; user-select: none; -webkit-user-select: none; }
.knob svg { display: block; overflow: visible; }
.lab { font: 500 11px var(--con-font-silk); letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); line-height: 1; white-space: nowrap; }
.knob:hover .lab, .knob.live .lab { color: var(--con-paper); }
.val { font: 400 10px var(--con-font-mono); color: var(--con-silk); line-height: 1; height: 11px; white-space: nowrap; }
.knob:focus-visible { outline: 2px solid var(--con-paper); outline-offset: 2px; border-radius: 4px; }
</style>
