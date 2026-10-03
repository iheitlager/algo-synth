<script setup lang="ts">
// One synth's panel, drawn from its model's description (spec 005 Req 8):
// the controls of that instrument, and nothing but messages going out.
import { getEngine, params, status } from '../audio/engine'
import { MOD_DESTS, MOD_SOURCES, type Control, type ModelDef } from '../audio/models'
import { Param, type ParamId } from '../audio/params'

const props = defineProps<{ s: number; def: ModelDef }>()

const val = (id: ParamId) => params.values[props.s]?.[id] ?? 0
// The cutoff slider is exponential: 0..1 is 20 Hz..20 kHz; the LFO rate's
// 0..1 is 0.01..50 Hz.
const toHz = { cutoff: (t: number) => 20 * 1000 ** t, lfo: (t: number) => 0.01 * 5000 ** t }
const toPos = {
  cutoff: (hz: number) => (hz > 0 ? Math.log(hz / 20) / Math.log(1000) : 0),
  lfo: (hz: number) => (hz > 0 ? Math.log(hz / 0.01) / Math.log(5000) : 0),
}
const position = (c: Extract<Control, { kind: 'range' }>) => (c.map ? toPos[c.map](val(c.param)) : val(c.param))
function send(id: ParamId, e: Event, map?: 'cutoff' | 'lfo') {
  const t = e.target as HTMLInputElement | HTMLSelectElement
  const raw = t instanceof HTMLInputElement && t.type === 'checkbox' ? Number(t.checked) : Number(t.value)
  getEngine()?.param(props.s, id, map ? toHz[map](raw) : raw)
}
// The patch: 8 overrides (source → destination, amount), spec 004 Req 7.
const patchSlots = Array.from({ length: 8 }, (_, i) => {
  const key = (field: string) => Param[`Patch${i + 1}${field}` as keyof typeof Param]
  return { n: i + 1, source: key('Source'), dest: key('Dest'), amount: key('Amount') }
})
</script>

<template>
  <fieldset class="panel" :disabled="!status.running">
    <div v-for="sec in def.sections" :key="sec.title" class="sec" :class="{ wide: sec.patch }">
      <b>{{ sec.title }}</b>
      <template v-if="sec.patch">
        <div v-for="slot in patchSlots" :key="slot.n" class="patch">
          <span class="soft">{{ slot.n }}</span>
          <select :value="val(slot.source)" @change="send(slot.source, $event)">
            <option v-for="[name, id] in MOD_SOURCES" :key="id" :value="id">{{ name }}</option>
          </select>
          <span class="soft">→</span>
          <select :value="val(slot.dest)" @change="send(slot.dest, $event)">
            <option v-for="[name, id] in MOD_DESTS" :key="id" :value="id">{{ name }}</option>
          </select>
          <input
            type="range" min="-1" max="1" step="0.01" :value="val(slot.amount)"
            :title="`amount ${val(slot.amount).toFixed(2)}`" @input="send(slot.amount, $event)"
          />
        </div>
      </template>
      <div v-else class="ctls">
        <template v-for="c in sec.controls" :key="c.kind === 'note' ? c.text : c.param">
          <label v-if="c.kind === 'range'">{{ c.label }}
            <input
              type="range" :min="c.min" :max="c.max" :step="c.step" :value="position(c)"
              @input="send(c.param, $event, c.map)"
            />
          </label>
          <label v-else-if="c.kind === 'select'">{{ c.label }}
            <select :value="val(c.param)" @change="send(c.param, $event)">
              <option v-for="[name, id] in c.options" :key="id" :value="id">{{ name }}</option>
            </select>
          </label>
          <label v-else-if="c.kind === 'switch'" class="sw">
            <input type="checkbox" :checked="val(c.param) >= 0.5" @change="send(c.param, $event)" /> {{ c.label }}
          </label>
          <p v-else class="note">{{ c.text }}</p>
        </template>
      </div>
    </div>
  </fieldset>
</template>

<style scoped>
.panel { border: 0; margin: 0; padding: 0; min-width: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 8px; font-size: 11px; color: var(--soft); }
.sec { display: grid; gap: 4px; align-content: start; border-top: 1px solid var(--trim); padding-top: 4px; min-width: 0; }
.sec.wide { grid-column: 1 / -1; }
.sec > b { color: var(--c); font-size: 10px; letter-spacing: 0.08em; text-transform: uppercase; }
.ctls { display: grid; grid-template-columns: 1fr 1fr; gap: 4px 8px; align-items: end; }
.ctls label { display: grid; gap: 2px; }
.ctls .sw { display: flex; gap: 4px; align-items: center; }
.note { grid-column: 1 / -1; margin: 0; line-height: 1.4; }
.patch { display: grid; grid-template-columns: 1.2em 1fr auto 1fr 1fr; gap: 4px 6px; align-items: center; }
input[type='range'] { accent-color: var(--c); }
</style>
