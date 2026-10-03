<script setup lang="ts">
// The eight patch slots of a synth (spec 004 Req 7): each joins a source to a
// destination with an amount. The view only sends the slot's three parameters.
import { getEngine, params, status } from '../../audio/engine'
import { MOD_DESTS, MOD_SOURCES } from '../../audio/models'
import { Param, type ParamId } from '../../audio/params'

const props = defineProps<{ s: number }>()
const val = (id: ParamId) => params.values[props.s]?.[id] ?? 0
const slots = Array.from({ length: 8 }, (_, i) => {
  const key = (field: string) => Param[`Patch${i + 1}${field}` as keyof typeof Param]
  return { n: i + 1, source: key('Source'), dest: key('Dest'), amount: key('Amount') }
})
const send = (id: ParamId, e: Event) => getEngine()?.param(props.s, id, Number((e.target as HTMLInputElement | HTMLSelectElement).value))
</script>

<template>
  <div class="bay">
    <div v-for="slot in slots" :key="slot.n" class="slot">
      <span class="n">{{ slot.n }}</span>
      <select :value="val(slot.source)" :disabled="!status.running" :aria-label="`Patch ${slot.n} source`" @change="send(slot.source, $event)">
        <option v-for="[name, id] in MOD_SOURCES" :key="id" :value="id">{{ name }}</option>
      </select>
      <span class="n">→</span>
      <select :value="val(slot.dest)" :disabled="!status.running" :aria-label="`Patch ${slot.n} destination`" @change="send(slot.dest, $event)">
        <option v-for="[name, id] in MOD_DESTS" :key="id" :value="id">{{ name }}</option>
      </select>
      <input
        type="range" min="-1" max="1" step="0.01" :value="val(slot.amount)" :disabled="!status.running"
        :aria-label="`Patch ${slot.n} amount`" @input="send(slot.amount, $event)"
      />
    </div>
  </div>
</template>

<style scoped>
.bay { display: grid; gap: 4px; }
.slot { display: grid; grid-template-columns: 1.2em minmax(70px, 1fr) auto minmax(70px, 1fr) minmax(70px, 1fr); gap: 4px 6px; align-items: center; }
.n { color: var(--con-silk-dim); font: 500 11px var(--con-font-silk); text-align: center; }
select { font: 500 12px var(--con-font-silk); background: var(--con-inset); color: var(--con-paper); border: 1px solid #343b46; border-radius: 3px; padding: 2px 4px; }
input[type='range'] { accent-color: var(--c); }
</style>
