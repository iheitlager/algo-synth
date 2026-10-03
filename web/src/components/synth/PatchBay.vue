<script setup lang="ts">
// The patch bay (spec 003 Req 9, spec 004 Req 7): sources down, destinations
// across, a lit point where a slot joins the two, and a knob for each slot's
// amount. Pressing a point joins or parts them in the first free slot. The
// view only sends the slot's three parameters; the eight slots are the engine's.
import { computed } from 'vue'
import { lin } from '../../audio/console'
import { findSlot, jackName, pressCell, type PatchSlot } from '../../audio/faceplate'
import { getEngine, params, status } from '../../audio/engine'
import { MOD_DESTS, MOD_SOURCES } from '../../audio/models'
import { Param, type ParamId } from '../../audio/params'
import ParamKnob from '../console/ParamKnob.vue'

const props = withDefaults(defineProps<{ s: number; slots?: number }>(), { slots: 8 })

const val = (id: ParamId) => params.values[props.s]?.[id] ?? 0
const ids = Array.from({ length: props.slots }, (_, i) => {
  const key = (field: string) => Param[`Patch${i + 1}${field}` as keyof typeof Param]
  return { source: key('Source'), dest: key('Dest'), amount: key('Amount') }
})
const patch = computed<PatchSlot[]>(() => ids.map((i) => ({ source: val(i.source), dest: val(i.dest), amount: val(i.amount) })))
// The id 0 of each list is the empty slot: it is not a jack.
const sources = MOD_SOURCES.filter(([, id]) => id !== 0)
const dests = MOD_DESTS.filter(([, id]) => id !== 0)
const used = computed(() => patch.value.filter((s) => s.source !== 0).length)
// One colour per source, so a joined point and its slot read together.
const colour = (source: number) => `hsl(${(source * 47 + 20) % 360} 70% 62%)`

const joined = (source: number, dest: number) => findSlot(patch.value, source, dest) >= 0
function press(source: number, dest: number) {
  const edit = pressCell(patch.value, source, dest)
  const slot = edit && ids[edit.slot]
  if (!edit || !slot) return
  getEngine()?.param(props.s, slot.source, edit.value.source)
  getEngine()?.param(props.s, slot.dest, edit.value.dest)
  getEngine()?.param(props.s, slot.amount, edit.value.amount)
}
const nameOf = (list: typeof sources, id: number) => jackName(list.find(([, i]) => i === id)?.[0] ?? '')
</script>

<template>
  <div class="bay">
    <table class="matrix">
      <caption class="sr">Patch bay: press a point to join a source to a destination</caption>
      <thead>
        <tr>
          <td />
          <th v-for="[name, id] in dests" :key="id" scope="col" class="dest"><span>{{ jackName(name) }}</span></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="[sname, sid] in sources" :key="sid">
          <th scope="row" class="src" :style="{ '--k': colour(sid) }">{{ jackName(sname) }}</th>
          <td v-for="[dname, did] in dests" :key="did">
            <button
              type="button" class="cell" :class="{ on: joined(sid, did) }" :style="{ '--k': colour(sid) }"
              :aria-pressed="joined(sid, did)" :aria-label="`${jackName(sname)} to ${jackName(dname)}`"
              :disabled="!status.running || (!joined(sid, did) && used >= props.slots)" @click="press(sid, did)"
            />
          </td>
        </tr>
      </tbody>
    </table>
    <div class="slots">
      <div class="count">Slots {{ used }}/{{ slots }}</div>
      <div v-for="(slot, i) in patch" :key="i" class="slot" :class="{ free: slot.source === 0 }">
        <template v-if="slot.source !== 0">
          <i class="swatch" :style="{ background: colour(slot.source) }" aria-hidden="true" />
          <span class="route">{{ nameOf(sources, slot.source) }} → {{ nameOf(dests, slot.dest) }}</span>
          <ParamKnob
            :synth="s" :id="ids[i]!.amount" label="" :name="`Patch ${i + 1} amount`" :scale="lin(-1, 1)" :size="28" bipolar no-val
            :color="colour(slot.source)" :text="(v: number) => `${v > 0 ? '+' : ''}${Math.round(v * 100)}%`"
          />
          <span class="amt">{{ slot.amount > 0 ? '+' : '' }}{{ Math.round(slot.amount * 100) }}%</span>
        </template>
        <span v-else class="empty">{{ i + 1 }} · free</span>
      </div>
    </div>
  </div>
</template>

<style scoped>
.bay { display: flex; gap: 22px; align-items: flex-start; flex-wrap: wrap; font-family: var(--con-font-silk); }
.matrix { border-collapse: separate; border-spacing: 2px; }
.dest { height: 84px; vertical-align: bottom; padding: 0 0 4px; }
.dest span { display: block; writing-mode: vertical-rl; transform: rotate(180deg); font: 500 11px var(--con-font-silk); letter-spacing: 0.1em; text-transform: uppercase; color: var(--con-silk-dim); white-space: nowrap; margin: 0 auto; }
.src { text-align: right; padding: 0 8px 0 0; font: 500 11px var(--con-font-silk); letter-spacing: 0.1em; text-transform: uppercase; color: var(--con-silk); white-space: nowrap; border-right: 3px solid var(--k); }
.cell { width: 22px; height: 18px; padding: 0; border-radius: 3px; border: 1px solid #343b46; background: var(--con-inset); cursor: pointer; display: block; }
.cell:hover:not(:disabled) { border-color: var(--k); }
.cell:disabled { opacity: 0.35; cursor: default; }
.cell:focus-visible { outline: 2px solid var(--con-paper); outline-offset: 1px; }
.cell.on { background: var(--k); border-color: var(--k); box-shadow: 0 0 9px var(--k), 0 0 2px #fff6 inset; }
.slots { flex: 1 1 520px; display: grid; gap: 4px; min-width: 240px; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); }
.count { grid-column: 1 / -1; }
.count { font: 600 11px var(--con-font-silk); letter-spacing: 0.18em; text-transform: uppercase; color: var(--con-silk-dim); margin-bottom: 2px; }
.slot { display: flex; align-items: center; gap: 8px; min-height: 30px; padding: 0 8px; background: var(--con-inset); border-radius: 4px; border: 1px solid #272d36; }
.slot.free { opacity: 0.45; }
.swatch { width: 8px; height: 8px; border-radius: 50%; flex: 0 0 auto; }
.route { flex: 1; font: 500 12px var(--con-font-silk); letter-spacing: 0.08em; text-transform: uppercase; color: var(--con-paper); white-space: nowrap; }
.amt { width: 38px; text-align: right; font: 400 10px var(--con-font-mono); color: var(--con-silk); }
.empty { font: 500 11px var(--con-font-silk); letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); }
.sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
</style>
