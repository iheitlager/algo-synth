<script setup lang="ts">
// One effect processor P1–P4 as a rack module (#54): the type, the knobs that
// type has, a return level and a return meter. P2–P4 can hear the processor
// before them (#94). All parameters are global.
import { computed } from 'vue'
import { PROC_KNOBS, clamp01, levelDb } from '../../audio/console'
import { getEngine, levels, METER_STRIPS, params } from '../../audio/engine'
import { Param, ProcType, type ParamId } from '../../audio/params'
import ParamKnob from './ParamKnob.vue'

const props = defineProps<{ n: number }>()

const id = (field: string) => Param[`P${props.n + 1}${field}` as keyof typeof Param]
const type = computed(() => params.values[0]?.[id('Type')] ?? 0)
const knobs = computed(() => (PROC_KNOBS[Math.round(type.value)] ?? []).map((k, i) => ({ ...k, id: id('ABCDE'[i] ?? 'A') })))
const types = Object.entries(ProcType)
const colour = computed(() => `var(--con-p${props.n + 1})`)
// P2–P4 can take the previous processor's output; P1 has no such setting.
const chainId = computed(() => (props.n > 0 ? id('In') : undefined))
const chained = computed(() => chainId.value !== undefined && (params.values[0]?.[chainId.value] ?? 0) >= 0.5)
const pick = (t: number) => getEngine()?.param(0, id('Type'), t)
const on = (pid: ParamId) => (params.values[0]?.[pid] ?? 0) >= 0.5
const flip = (pid: ParamId) => getEngine()?.param(0, pid, on(pid) ? 0 : 1)
// The return meter: −54 dB is empty, 0 dB full.
const level = computed(() => clamp01((levelDb(levels.values[METER_STRIPS + 2 + props.n] ?? 0) + 54) / 54))
</script>

<template>
  <div class="proc" :class="{ chained }" :style="{ '--c': colour, '--prev': `var(--con-p${n})` }">
    <header>
      <span class="id">P{{ n + 1 }}</span>
      <button
        v-if="chainId !== undefined" class="chain" :aria-pressed="chained" :title="`Hear P${n}'s output as well as the sends`"
        @click="flip(chainId)"
      >← P{{ n }}</button>
      <span class="seg" role="group" :aria-label="`P${n + 1} type`">
        <button v-for="[name, t] in types" :key="t" :aria-pressed="Math.round(type) === t" @click="pick(t)">{{ name }}</button>
      </span>
      <span class="rm" title="Return level"><span class="bar"><i :style="{ width: `${level * 100}%` }" /></span></span>
    </header>
    <div v-if="knobs.length" class="knobs">
      <template v-for="k in knobs" :key="k.id">
        <button v-if="k.toggle" class="pp" :aria-pressed="on(k.id)" @click="flip(k.id)">{{ k.label }}</button>
        <ParamKnob
          v-else :synth="0" :id="k.id" :label="k.label" :name="`P${n + 1} ${k.label}`" :size="40" :color="colour"
          :text="(v) => k.text?.(v) ?? ''" :def="k.def"
        />
      </template>
      <span class="gap" />
      <ParamKnob :synth="0" :id="id('Return')" label="Return" :name="`P${n + 1} return`" :size="48" :color="colour" :def="0.4" />
    </div>
    <div v-else class="empty">Empty slot · pick a type</div>
  </div>
</template>

<style scoped>
.proc { background: linear-gradient(#20252d, #1b1f26); border: 1px solid #2b323c; border-left: 5px solid var(--c); border-radius: 4px; padding: 8px 10px 9px; font-family: var(--con-font-silk); }
header { display: flex; flex-wrap: wrap; align-items: center; gap: 6px 8px; margin-bottom: 7px; }
.proc { position: relative; }
/* A chained processor hangs from the one before it. */
.proc.chained::before { content: ''; position: absolute; left: 14px; top: -11px; width: 4px; height: 11px; background: var(--prev); }
.chain { border: 1px solid #343b46; background: var(--con-inset); border-radius: 3px; padding: 1px 6px; font: 500 12px var(--con-font-silk); letter-spacing: 0.08em; color: var(--con-silk-dim); cursor: pointer; white-space: nowrap; }
.chain[aria-pressed='true'] { color: #101215; background: var(--prev); border-color: var(--prev); font-weight: 700; }
.id { font-size: 20px; font-weight: 700; letter-spacing: 0.08em; color: var(--c); width: 26px; }
.seg { display: inline-flex; border: 1px solid #343b46; border-radius: 3px; overflow: hidden; background: var(--con-inset); }
.seg button { border: 0; border-radius: 0; background: transparent; padding: 2px 5px; font: 500 12px var(--con-font-silk); letter-spacing: 0.06em; text-transform: uppercase; color: var(--con-silk-dim); cursor: pointer; }
.seg button + button { border-left: 1px solid #343b46; }
.seg button[aria-pressed='true'] { background: var(--c); color: #101215; font-weight: 700; }
.rm { margin-left: auto; display: flex; align-items: center; gap: 6px; font-size: 11px; letter-spacing: 0.14em; color: var(--con-silk-dim); text-transform: uppercase; }
.bar { width: 56px; height: 8px; border-radius: 2px; background: var(--con-inset); overflow: hidden; display: block; }
.bar i { display: block; height: 100%; background: var(--c); }
.knobs { display: flex; gap: 12px; align-items: flex-start; min-height: 66px; }
.knobs .gap { flex: 1; }
.pp { align-self: center; border: 1px solid #343b46; background: var(--con-inset); border-radius: 3px; padding: 2px 8px; font: 500 12px var(--con-font-silk); letter-spacing: 0.12em; color: var(--con-silk-dim); text-transform: uppercase; cursor: pointer; }
.pp[aria-pressed='true'] { color: #101215; background: var(--c); border-color: var(--c); font-weight: 700; }
.empty { font-size: 13px; letter-spacing: 0.14em; text-transform: uppercase; color: var(--con-silk-dim); padding: 14px 0 0; }
</style>
