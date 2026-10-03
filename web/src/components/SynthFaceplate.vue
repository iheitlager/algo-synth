<script setup lang="ts">
// One synth's faceplate (spec 003 Req 9): its model's description drawn with
// the console's knobs, LED switches, stepped selectors and envelope curves, in
// the model's palette. The values shown are the engine's (`params`); a change
// goes out as a message and nothing is decided here.
import { computed } from 'vue'
import { exp, lin } from '../audio/console'
import { fmtUnit } from '../audio/faceplate'
import { getEngine, params, status } from '../audio/engine'
import { scaleOf, type Control, type ModelDef } from '../audio/models'
import type { ParamId } from '../audio/params'
import ParamKnob from './console/ParamKnob.vue'
import EnvGraph from './synth/EnvGraph.vue'
import PatchBay from './synth/PatchBay.vue'
import Selector from './synth/Selector.vue'
import Switch from './synth/Switch.vue'

const props = defineProps<{ s: number; def: ModelDef }>()

const val = (id: ParamId) => params.values[props.s]?.[id] ?? 0
const send = (id: ParamId, v: number) => getEngine()?.param(props.s, id, v)

/** The look of this faceplate: the model's colours, and the console's tokens remapped onto them. */
const vars = computed(() => {
  const t = props.def.theme
  return {
    '--c': t.accent, '--plate': t.panel, '--trim': t.trim, '--wood': t.wood ?? 'transparent',
    '--con-paper': t.ink, '--con-silk': t.ink, '--con-silk-dim': t.soft,
  }
})

// An envelope's times: exponential knobs, so 5 ms and 4 s are both easy to set.
const ENV_KNOBS = {
  a: { label: 'A', scale: exp(0.001, 2), unit: 'sec' as const, def: 0.005 },
  d: { label: 'D', scale: exp(0.001, 4), unit: 'sec' as const, def: 0.3 },
  s: { label: 'S', scale: lin(0, 1), unit: 'pct' as const, def: 0.7 },
  r: { label: 'R', scale: exp(0.001, 4), unit: 'sec' as const, def: 0.3 },
}
type Env = Extract<Control, { kind: 'env' }>
const envKnobs = (c: Env) =>
  (['a', 'd', 's', 'r'] as const).flatMap((k) => {
    const id = c[k]
    return id === undefined ? [] : [{ k, id, ...ENV_KNOBS[k] }]
  })
/** What the curve is drawn from: a missing decay or sustain is a plain AR, and a contour's release is its decay. */
const envTimes = (c: Env) => ({
  a: val(c.a),
  d: c.d !== undefined ? val(c.d) : 0.001,
  s: c.s !== undefined ? val(c.s) : 1,
  r: c.r !== undefined ? val(c.r) : c.decayIsRelease && c.d !== undefined ? val(c.d) : 0.05,
})
const key = (c: Control, i: number) => (c.kind === 'note' ? c.text : `${c.kind}${i}`)
</script>

<template>
  <div class="plate" :class="{ wood: !!def.theme.wood }" :style="vars">
    <i v-if="def.theme.wood" class="cheek l" aria-hidden="true" /><i v-if="def.theme.wood" class="cheek r" aria-hidden="true" />
    <div class="mods">
      <section v-for="sec in def.sections" :key="sec.title" class="mod" :class="{ wide: sec.patch }" :aria-label="sec.title">
        <h3>{{ sec.title }}</h3>
        <PatchBay v-if="sec.patch" :s="s" />
        <div v-else class="ctls">
          <template v-for="(c, i) in sec.controls" :key="key(c, i)">
            <ParamKnob
              v-if="c.kind === 'knob'" :synth="s" :id="c.param" :label="c.label" :name="`${def.name} ${sec.title} ${c.label}`"
              :scale="scaleOf(c)" :def="c.def" :size="c.size ?? 38" :bipolar="c.bipolar" :color="def.theme.accent"
              :text="(v: number) => fmtUnit(c.unit, v)"
            />
            <Selector
              v-else-if="c.kind === 'select'" :model-value="val(c.param)" :label="c.label" :options="c.options"
              :name="`${def.name} ${sec.title} ${c.label}`" :disabled="!status.running" @update:model-value="send(c.param, $event)"
            />
            <Switch
              v-else-if="c.kind === 'switch'" :model-value="val(c.param) >= 0.5" :label="c.label"
              :name="`${def.name} ${sec.title} ${c.label}`" :disabled="!status.running" @update:model-value="send(c.param, Number($event))"
            />
            <div v-else-if="c.kind === 'env'" class="env">
              <EnvGraph v-bind="envTimes(c)" :label="`${def.name} ${sec.title}`" />
              <div class="eknobs">
                <ParamKnob
                  v-for="k in envKnobs(c)" :key="k.k" :synth="s" :id="k.id" :label="k.label"
                  :name="`${def.name} ${sec.title} ${k.label}`" :scale="k.scale" :def="k.def" :size="30" :color="def.theme.accent"
                  :text="(v: number) => fmtUnit(k.unit, v)"
                />
              </div>
            </div>
            <p v-else class="note">{{ c.text }}</p>
          </template>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.plate {
  position: relative; padding: 14px 22px 16px; border-radius: 6px; border: 1px solid var(--trim); color: var(--con-silk-dim);
  background: linear-gradient(180deg, color-mix(in srgb, var(--plate) 90%, white 10%), var(--plate) 18%, color-mix(in srgb, var(--plate) 88%, black 12%));
  box-shadow: 0 1px 0 #ffffff12 inset, 0 8px 22px #0008; font-family: var(--con-font-silk);
}
.cheek { position: absolute; top: 0; bottom: 0; width: 14px; background: repeating-linear-gradient(90deg, var(--wood), color-mix(in srgb, var(--wood) 70%, black) 3px, var(--wood) 6px); }
.cheek.l { left: 0; border-radius: 6px 0 0 6px; box-shadow: -1px 0 0 #0006 inset; }
.cheek.r { right: 0; border-radius: 0 6px 6px 0; box-shadow: 1px 0 0 #0006 inset; }
.plate.wood { padding-inline: 30px; }
.mods { display: flex; flex-wrap: wrap; gap: 12px 10px; align-items: flex-start; }
.mod {
  flex: 0 1 auto; padding: 8px 12px 10px; min-width: 0; border-radius: 4px; border: 1px solid var(--trim);
  background: color-mix(in srgb, var(--plate) 82%, black 18%); box-shadow: 0 1px 0 #ffffff0d inset; border-top: 2px solid var(--c);
}
.mod.wide { flex: 1 1 100%; }
h3 { margin: 0 0 8px; font-size: 12px; font-weight: 600; letter-spacing: 0.2em; text-transform: uppercase; color: var(--c); line-height: 1; }
.ctls { display: flex; flex-wrap: wrap; gap: 8px 14px; align-items: flex-end; }
.env { display: flex; flex-direction: column; gap: 6px; }
.eknobs { display: flex; gap: 8px; justify-content: space-between; }
.note { margin: 0; max-width: 260px; font: 400 11px/1.4 var(--con-font-mono); }
</style>
