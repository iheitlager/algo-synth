<script setup lang="ts">
// One thin channel strip (#54): its tape, drive, four sends, pan, mute and
// solo, fader and meter. It is described by props, not by "synth", so a group
// bus can use it later (epic #62).
import { computed, ref } from 'vue'
import { INSERT_SHORT, levelToPos, posToDb, posToLevel, dbText, lin } from '../../audio/console'
import { getEngine, levels, params, status } from '../../audio/engine'
import { Param, type ParamId } from '../../audio/params'
import EditableName from '../EditableName.vue'
import Fader from './Fader.vue'
import LedMeter from './LedMeter.vue'
import { showInsertPanel } from './insertPanel'
import { showStripPanel } from './stripPanel'
import ParamKnob from './ParamKnob.vue'

const props = defineProps<{
  /** The strip's index in the engine: a synth 0–15, a group 16–23. */
  s: number
  kind: 'synth' | 'group'
  collapsed?: boolean
  /** Where it can be sent, master first. */
  outs: { out: number; label: string }[]
  /** The group it feeds, for the coloured tag under the tape. */
  feeds?: { label: string; color: string }
  title: string
  subtitle: string
  color: string
  /** Where it is routed from, e.g. "Ch 1 · 3". */
  footer: string
  selected?: boolean
  /** Dimmed because another strip is soloed or this one is muted. */
  silenced?: boolean
  /** Which processors are Off, so their sends can be drawn dimmed. */
  procOff: boolean[]
}>()
const emit = defineEmits<{
  select: []
  open: []
  collapse: []
  hide: []
  remove: []
  /** Another strip was dropped on this one: its index. */
  move: [from: number]
  setOut: [out: number]
  /** The name typed for this strip; '' puts the default back (#127). */
  rename: [name: string]
}>()
const renaming = ref(false)

function onDrop(e: DragEvent) {
  const from = Number(e.dataTransfer?.getData('text/plain'))
  if (Number.isInteger(from) && from !== props.s) emit('move', from)
}
function onDrag(e: DragEvent) {
  e.dataTransfer?.setData('text/plain', String(props.s))
  if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move'
}

const val = (id: ParamId) => params.values[props.s]?.[id] ?? 0
const send = (id: ParamId, v: number) => getEngine()?.param(props.s, id, v)
// Three insert slots in series; a button opens the slot's panel.
const slotTypes = [Param.I1Type, Param.I2Type, Param.I3Type]
const slots = computed(() => slotTypes.map((id, n) => ({ n, type: Math.round(val(id)), text: INSERT_SHORT[Math.round(val(id))] ?? '—' })))
const openSlot = (n: number, e: MouseEvent) => showInsertPanel(props.s, n, props.title, e.currentTarget as HTMLElement)
const sends = [Param.Send1, Param.Send2, Param.Send3, Param.Send4]
const pos = computed(() => levelToPos(val(Param.Level)))
const setPos = (p: number) => send(Param.Level, posToLevel(p))
const readout = computed(() => `${dbText(posToDb(pos.value))} dB`)
const level = computed(() => levels.values[props.s] ?? 0)
const out = computed(() => Math.round(val(Param.Out)))
const panText = (p: number) => (Math.abs(p) < 0.02 ? 'C' : `${p < 0 ? 'L' : 'R'}${Math.round(Math.abs(p) * 100)}`)
const toggle = (id: ParamId) => send(id, val(id) >= 0.5 ? 0 : 1)
</script>

<template>
  <div
    v-if="collapsed" class="strip collapsed" :class="{ silenced, selected }" :style="{ '--c': color }"
    @dragover.prevent @drop.prevent="onDrop"
  >
    <button class="tape" :title="`${title}: click to open, drag to move`" draggable="true" @dragstart="onDrag" @click="$emit('collapse')">
      <span class="vert">{{ title }}</span>
    </button>
    <div class="ms">
      <button class="btn mute" :aria-pressed="val(Param.Mute) >= 0.5" title="Mute" :disabled="!status.running" @click="toggle(Param.Mute)">M</button>
      <button class="btn solo" :aria-pressed="val(Param.Solo) >= 0.5" title="Solo" :disabled="!status.running" @click="toggle(Param.Solo)">S</button>
    </div>
    <LedMeter :level="level" :height="214" />
  </div>

  <div
    v-else class="strip" :class="{ silenced, selected }" :style="{ '--c': color }"
    @dragover.prevent @drop.prevent="onDrop"
  >
    <div class="tape-wrap">
      <div v-if="renaming" class="tape">
        <b><EditableName v-model:editing="renaming" :value="title" :label="title" @rename="$emit('rename', $event)" /></b><span>{{ subtitle }}</span>
      </div>
      <button v-else class="tape" :title="`${title}: click to select, double-click for its panel, drag to move`" draggable="true" @dragstart="onDrag" @click="$emit('select')" @dblclick="$emit('open')">
        <b>{{ title }}</b><span>{{ subtitle }}</span>
      </button>
      <span v-if="!renaming" class="tools">
        <button title="Rename this strip" :aria-label="`Rename ${title}`" @click="renaming = true">✎</button>
        <button class="strip-tool" title="Strip presets, copy and paste" :aria-label="`${title} presets`" @click="showStripPanel(s, title, $event.currentTarget as HTMLElement)">⋯</button>
        <button title="Collapse this strip" @click="$emit('collapse')">◂</button>
        <button v-if="kind === 'group'" title="Remove this group; what feeds it goes to the master" @click="$emit('remove')">×</button>
        <button v-else title="Hide this strip from the console" @click="$emit('hide')">×</button>
      </span>
    </div>
    <div class="feeds" :style="feeds ? { background: feeds.color } : undefined" :class="{ none: !feeds }">{{ feeds ? `→ ${feeds.label}` : 'Master' }}</div>

    <div class="sec">
      <div class="cap">Inserts</div>
      <button
        v-for="slot in slots" :key="slot.n" class="slot-btn" :data-on="slot.type > 0 ? 1 : 0" :title="`Insert ${slot.n + 1}: click to edit`"
        :disabled="!status.running" @click="openSlot(slot.n, $event)"
      >{{ slot.text }}</button>
    </div>

    <div class="sec">
      <div class="cap">Sends</div>
      <div class="sends">
        <ParamKnob
          v-for="(id, n) in sends" :key="id" :synth="s" :id="id" :label="`P${n + 1}`" :name="`${title} send to P${n + 1}`"
          :size="28" :color="`var(--con-p${n + 1})`" no-val :class="{ off: procOff[n] }"
        />
      </div>
    </div>

    <div class="sec">
      <div class="cap">{{ kind === 'group' ? 'Bal' : 'Pan' }}</div>
      <ParamKnob :synth="s" :id="Param.Pan" label="" :name="`${title} ${kind === 'group' ? 'balance' : 'pan'}`" :scale="lin(-1, 1)" :text="panText" :size="36" color="var(--con-pan)" bipolar no-val />
      <div class="pan">{{ panText(val(Param.Pan)) }}</div>
    </div>

    <div class="ms">
      <button class="btn mute" :aria-pressed="val(Param.Mute) >= 0.5" title="Mute" :disabled="!status.running" @click="toggle(Param.Mute)">M</button>
      <button class="btn solo" :aria-pressed="val(Param.Solo) >= 0.5" title="Solo" :disabled="!status.running" @click="toggle(Param.Solo)">S</button>
    </div>

    <div class="fader-wrap">
      <Fader :model-value="pos" :label="`${title} level`" :def="0.8" @update:model-value="setPos" />
      <LedMeter :level="level" :height="214" />
    </div>
    <div class="readout">{{ readout }}</div>
    <select class="out" :value="out" :aria-label="`${title} output`" :disabled="!status.running" @change="$emit('setOut', Number(($event.target as HTMLSelectElement).value))">
      <option v-for="o in outs" :key="o.out" :value="o.out">{{ o.label }}</option>
    </select>
    <div class="ch">{{ footer }}</div>
  </div>
</template>

<style scoped>
.strip {
  width: 78px; flex: 0 0 78px; display: flex; flex-direction: column; align-items: center; gap: 9px; padding: 0 0 10px;
  background: var(--con-panel); border-right: 1px solid #0d0f13; box-shadow: 1px 0 0 #262c35 inset; font-family: var(--con-font-silk);
}
.strip:nth-child(even) { background: #1b2028; }
.strip.silenced { filter: saturate(0.35) brightness(0.7); }
.strip.selected .tape { background: linear-gradient(#323843, #272c34); }
.tape-wrap { position: relative; align-self: stretch; display: flex; }
.tape-wrap .tape { flex: 1; }
.tools { position: absolute; top: 6px; right: 2px; display: none; flex-direction: column; gap: 2px; }
.tape-wrap:hover .tools, .tools:focus-within { display: flex; }
.tools button { width: 16px; height: 16px; padding: 0; border: 0; background: #0009; color: var(--con-silk); border-radius: 3px; font: 12px/1 var(--con-font-silk); cursor: pointer; }
.tools button:hover { color: #fff; background: #000c; }
.feeds { align-self: stretch; height: 14px; margin-top: -9px; font: 600 10px/14px var(--con-font-silk); letter-spacing: 0.14em; text-transform: uppercase; text-align: center; color: #101215; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; padding: 0 3px; }
.feeds.none { background: transparent; color: var(--con-silk-dim); }
.out { width: 66px; background: var(--con-inset); color: var(--con-silk); border: 1px solid #343b46; border-radius: 3px; font: 500 11px var(--con-font-silk); letter-spacing: 0.06em; padding: 1px 2px; text-transform: uppercase; }
.strip.collapsed { width: 34px; flex: 0 0 34px; gap: 8px; }
.strip.collapsed .tape { padding: 8px 0; min-height: 96px; border-top-width: 5px; }
.vert { writing-mode: vertical-rl; transform: rotate(180deg); font: 700 15px var(--con-font-silk); letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-paper); white-space: nowrap; }
.strip.collapsed .ms { flex-direction: column; }
.strip.collapsed .btn { width: 24px; }
.tape {
  align-self: stretch; text-align: center; padding: 7px 2px 6px; border: 0; border-top: 5px solid var(--c); border-radius: 0;
  background: linear-gradient(#262b33, #1f242b); color: inherit; cursor: pointer; font: inherit;
}
.tape:hover:not(:disabled) { border-color: var(--c); background: linear-gradient(#2c323b, #232830); }
.tape b { display: block; font-size: 16px; font-weight: 700; letter-spacing: 0.1em; color: var(--con-paper); text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.tape span { display: block; font-size: 11px; letter-spacing: 0.12em; color: var(--con-silk-dim); text-transform: uppercase; margin-top: 1px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.sec { display: flex; flex-direction: column; align-items: center; gap: 4px; width: 100%; }
.cap { font-size: 11px; letter-spacing: 0.18em; text-transform: uppercase; color: var(--con-silk-dim); }
.sends { display: grid; grid-template-columns: 1fr 1fr; gap: 3px 0; width: 100%; padding-inline: 5px; justify-items: center; }
.sends .off { opacity: 0.28; }
.slot-btn {
  border: 1px solid #343b46; background: var(--con-inset); border-radius: 3px; font: 500 12px var(--con-font-silk);
  letter-spacing: 0.14em; padding: 1px 7px; cursor: pointer; color: var(--con-silk-dim); width: 54px; line-height: 1.2;
}
.slot-btn[data-on='1'] { color: #fff; background: #3a1714; border-color: var(--con-drive); box-shadow: 0 0 8px #d6483f55; }
.pan { font: 400 10px var(--con-font-mono); color: var(--con-silk-dim); }
.ms { display: flex; gap: 5px; }
.btn {
  width: 30px; height: 24px; border-radius: 3px; border: 1px solid #39414d; cursor: pointer; padding: 0;
  background: linear-gradient(#2e343d, #232830); font: 700 13px var(--con-font-silk); letter-spacing: 0.06em; color: var(--con-silk);
  box-shadow: 0 2px 0 #0a0c0f;
}
.btn:active:not(:disabled) { transform: translateY(1px); box-shadow: 0 1px 0 #0a0c0f; }
.btn.mute[aria-pressed='true'] { background: #7a2a22; color: #fff; border-color: #e0654f; box-shadow: 0 0 10px #e0654f55; }
.btn.solo[aria-pressed='true'] { background: #8a6a10; color: #fff; border-color: #f0c23a; box-shadow: 0 0 10px #f0c23a55; }
.fader-wrap { display: flex; gap: 5px; align-items: stretch; height: 214px; }
.readout { margin-top: 10px; font: 500 11px var(--con-font-mono); color: var(--con-silk); background: var(--con-inset); border-radius: 3px; padding: 2px 6px; min-width: 52px; text-align: center; }
.ch { font-size: 11px; letter-spacing: 0.16em; color: var(--con-silk-dim); text-transform: uppercase; white-space: nowrap; }
</style>
