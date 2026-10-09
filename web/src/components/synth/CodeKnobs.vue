<script setup lang="ts">
// A knob for each number of a Modular synth's SynthDef (#329), grouped by the
// UGen it sets, as the engine lists them (`Engine::knob_list`). A knob drives
// its `Ctl` parameter; the engine prints the code with the knobs' values, so
// the text is asked for again once a knob rests. A `Select` index is a switch
// (#433): two positions an LED toggle, more a row of buttons.
import { computed, onBeforeUnmount, watch } from 'vue'
import { exp, lin } from '../../audio/console'
import { codeKnobs, getEngine, params, requestCode, type CodeKnob } from '../../audio/engine'
import { Param, type ParamId } from '../../audio/params'
import ParamKnob from '../console/ParamKnob.vue'
import Selector from './Selector.vue'
import Switch from './Switch.vue'

const props = defineProps<{ s: number; color: string }>()

/** The knobs by UGen, in the order the code has them. */
const groups = computed(() => {
  const out: { key: number; ugen: string; knobs: CodeKnob[] }[] = []
  for (const k of codeKnobs[props.s] ?? []) {
    const last = out[out.length - 1]
    if (last?.key === k.module) last.knobs.push(k)
    else out.push({ key: k.module, ugen: k.ugen, knobs: [k] })
  }
  return out
})

const id = (k: CodeKnob) => (Param.Ctl1 + k.ctl) as ParamId
const scale = (k: CodeKnob) => (k.exp && k.lo > 0 ? exp(k.lo, k.hi) : lin(k.lo, k.hi))
const value = (k: CodeKnob) => params.values[props.s]?.[id(k)] ?? k.def
const set = (k: CodeKnob, v: number) => getEngine()?.param(props.s, id(k), v)
const positions = (k: CodeKnob) => Array.from({ length: Math.round(k.hi - k.lo) + 1 }, (_, i) => [String(k.lo + i), k.lo + i] as const)
const text = (v: number) => (Math.abs(v) >= 100 ? v.toFixed(0) : Math.abs(v) >= 1 ? v.toFixed(2) : v.toFixed(3))

// The text follows the knobs: once they rest, the engine prints it again.
let timer: ReturnType<typeof setTimeout> | undefined
watch(
  () => (codeKnobs[props.s] ?? []).map((k) => params.values[props.s]?.[id(k)]),
  (now, before) => {
    if (!before || now.length !== before.length || now.every((v, i) => v === before[i])) return
    clearTimeout(timer)
    timer = setTimeout(() => requestCode(props.s), 200)
  },
)
onBeforeUnmount(() => clearTimeout(timer))
</script>

<template>
  <div class="knobs">
    <p v-if="!groups.length" class="empty">Every number in the SynthDef becomes a knob here once it is applied.</p>
    <div v-for="g in groups" :key="g.key" class="ugen">
      <h4>{{ g.ugen }}</h4>
      <div class="row">
        <template v-for="k in g.knobs" :key="k.ctl">
          <Switch
            v-if="k.step > 0 && k.hi - k.lo === 1" :model-value="value(k) >= k.hi" :label="k.name"
            :name="`${g.ugen} ${k.name}`" :color="color" @update:model-value="set(k, $event ? k.hi : k.lo)"
          />
          <Selector
            v-else-if="k.step > 0" :model-value="value(k)" :label="k.name" :name="`${g.ugen} ${k.name}`"
            :options="positions(k)" @update:model-value="set(k, $event)"
          />
          <ParamKnob
            v-else :synth="s" :id="id(k)" :label="k.name" :name="`${g.ugen} ${k.name}`"
            :scale="scale(k)" :def="k.def" :size="32" :color="color" :text="text"
          />
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.knobs { display: flex; flex-wrap: wrap; gap: 10px 18px; align-items: flex-start; }
.ugen { display: grid; gap: 6px; padding: 6px 10px 8px; border: 1px solid var(--trim); border-radius: 4px; }
h4 { margin: 0; font-size: 11px; font-weight: 600; letter-spacing: 0.14em; text-transform: uppercase; color: var(--c); }
.row { display: flex; gap: 10px; }
.empty { margin: 0; font-size: 11px; color: var(--con-silk-dim); }
</style>
