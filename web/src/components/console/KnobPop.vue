<script setup lang="ts">
// The shared popover for the console's knobs (#52). Mount it once.
import { nextTick, onBeforeUnmount, onMounted, ref } from 'vue'
import { closePop, pop } from './pop'

const root = ref<HTMLElement | null>(null)
const input = ref<HTMLInputElement | null>(null)

function onDown(e: PointerEvent) {
  if (!pop.open || root.value?.contains(e.target as Node)) return
  // A press on the knob that owns it is the knob's to handle.
  if ((e.target as HTMLElement).closest?.('[data-knob]')) return
  closePop()
}
function onKey(e: KeyboardEvent) {
  if (pop.open && (e.key === 'Escape' || e.key === 'Enter')) closePop()
}
onMounted(() => {
  addEventListener('pointerdown', onDown, true)
  addEventListener('keydown', onKey)
})
onBeforeUnmount(() => {
  removeEventListener('pointerdown', onDown, true)
  removeEventListener('keydown', onKey)
})
// A click opens a slider: put the focus on it so the arrow keys work too.
const focus = () => nextTick(() => input.value?.focus({ preventScroll: true }))
defineExpose({ focus })
</script>

<template>
  <svg width="0" height="0" style="position: absolute" aria-hidden="true">
    <defs>
      <radialGradient id="con-cap" cx="35%" cy="28%" r="80%">
        <stop offset="0" stop-color="#4a5260" />
        <stop offset="0.6" stop-color="#2a2f38" />
        <stop offset="1" stop-color="#15181d" />
      </radialGradient>
    </defs>
  </svg>
  <div
    v-show="pop.open" ref="root" class="pop" :class="{ below: pop.below, inert: !pop.interactive }" role="dialog"
    :aria-label="pop.label" :style="{ left: `${pop.x}px`, top: `${pop.y}px`, '--c': pop.color, '--ax': `${pop.arrow}px` }"
  >
    <div class="row"><span class="pl">{{ pop.label }}</span><span class="pv">{{ pop.text }}</span></div>
    <input
      ref="input" type="range" min="0" max="1000" step="1" :value="Math.round(pop.value * 1000)" :disabled="!pop.interactive"
      :aria-label="pop.label" @input="pop.apply?.(Number(($event.target as HTMLInputElement).value) / 1000)"
    />
    <small>Esc or click away to close</small>
  </div>
</template>

<style scoped>
.pop {
  position: fixed; z-index: 20; width: 188px; padding: 9px 11px 10px;
  background: #0c0e12; border: 1px solid #3a424e; border-radius: 6px; box-shadow: 0 12px 30px #000c;
  font-family: var(--con-font-silk);
}
.pop.inert { pointer-events: none; }
.row { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 7px; }
.pl { font-size: 14px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-paper); font-weight: 600; }
.pv { font: 500 13px var(--con-font-mono); color: var(--c, var(--con-paper)); }
input { width: 100%; margin: 0; accent-color: var(--c, #e9e4d6); height: 22px; }
small { display: block; margin-top: 5px; font-size: 11px; letter-spacing: 0.08em; color: var(--con-silk-dim); text-transform: uppercase; }
.pop::after {
  content: ''; position: absolute; left: var(--ax, 50%); bottom: -6px; width: 10px; height: 10px; margin-left: -5px;
  background: #0c0e12; border-right: 1px solid #3a424e; border-bottom: 1px solid #3a424e; transform: rotate(45deg);
}
.pop.below::after { bottom: auto; top: -6px; transform: rotate(225deg); }
</style>
