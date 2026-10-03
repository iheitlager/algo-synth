<script setup lang="ts">
// The panel of one insert slot (#58): its type and the five knobs that type
// has, with their real units. Mount it once; a slot button opens it.
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { INSERT_KNOBS } from '../../audio/console'
import { getEngine, params } from '../../audio/engine'
import { InsertType, Param } from '../../audio/params'
import { closeInsertPanel, insertPanel as p } from './insertPanel'
import ParamKnob from './ParamKnob.vue'

const id = (field: string) => Param[`I${p.slot + 1}${field}` as keyof typeof Param]
const type = computed(() => Math.round(params.values[p.strip]?.[id('Type')] ?? 0))
const knobs = computed(() => (INSERT_KNOBS[type.value] ?? []).map((k, i) => ({ ...k, id: id('ABCDE'[i] ?? 'A') })))
const types = Object.entries(InsertType)
const pick = (t: number) => getEngine()?.param(p.strip, id('Type'), t)

function onDown(e: PointerEvent) {
  if (!p.open) return
  const t = e.target as HTMLElement
  // The knob popover and the slot buttons are part of the panel's own work.
  if (t.closest?.('.insert-panel, .pop, .slot-btn')) return
  closeInsertPanel()
}
function onKey(e: KeyboardEvent) {
  if (p.open && e.key === 'Escape') closeInsertPanel()
}
onMounted(() => { addEventListener('pointerdown', onDown, true); addEventListener('keydown', onKey) })
onBeforeUnmount(() => { removeEventListener('pointerdown', onDown, true); removeEventListener('keydown', onKey) })
</script>

<template>
  <div v-show="p.open" class="insert-panel" role="dialog" :aria-label="`${p.title} insert ${p.slot + 1}`" :style="{ left: `${p.x}px`, top: `${p.y}px` }">
    <header><b>{{ p.title }}</b><span>Insert {{ p.slot + 1 }}</span></header>
    <div class="seg" role="group" aria-label="Insert type">
      <button v-for="[name, t] in types" :key="t" :aria-pressed="type === t" @click="pick(t)">{{ name }}</button>
    </div>
    <div v-if="knobs.length" class="knobs">
      <ParamKnob
        v-for="k in knobs" :key="k.id" :synth="p.strip" :id="k.id" :label="k.label" :name="`${p.title} insert ${p.slot + 1} ${k.label}`"
        :size="40" color="var(--con-drive)" :text="(v) => k.text?.(v) ?? ''" :def="k.def"
      />
    </div>
    <div v-else class="empty">Empty slot · pick a type</div>
  </div>
</template>

<style scoped>
.insert-panel {
  position: fixed; z-index: 15; width: 322px; padding: 10px 12px 12px; background: #0f1217; border: 1px solid #3a424e;
  border-top: 3px solid var(--con-drive); border-radius: 6px; box-shadow: 0 14px 34px #000d; font-family: var(--con-font-silk);
}
header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
header b { font-size: 16px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--con-paper); }
header span { font-size: 12px; letter-spacing: 0.16em; text-transform: uppercase; color: var(--con-silk-dim); }
.seg { display: flex; flex-wrap: wrap; border: 1px solid #343b46; border-radius: 3px; overflow: hidden; background: var(--con-inset); }
.seg button { flex: 1; border: 0; border-radius: 0; background: transparent; padding: 3px 4px; font: 500 12px var(--con-font-silk); letter-spacing: 0.1em; text-transform: uppercase; color: var(--con-silk-dim); cursor: pointer; }
.seg button + button { border-left: 1px solid #343b46; }
.seg button[aria-pressed='true'] { background: var(--con-drive); color: #fff; font-weight: 700; }
.knobs { display: flex; gap: 12px; margin-top: 12px; justify-content: space-between; }
.empty { margin-top: 12px; font-size: 13px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--con-silk-dim); }
</style>
