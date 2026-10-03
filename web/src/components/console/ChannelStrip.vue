<script setup lang="ts">
// One thin channel strip (#54): its tape, drive, four sends, pan, mute and
// solo, fader and meter. It is described by props, not by "synth", so a group
// bus can use it later (epic #62).
import { computed } from 'vue'
import { INSERT_SHORT, levelToPos, posToDb, posToLevel, dbText, lin } from '../../audio/console'
import { getEngine, levels, params, status } from '../../audio/engine'
import { Param, type ParamId } from '../../audio/params'
import Fader from './Fader.vue'
import LedMeter from './LedMeter.vue'
import { showInsertPanel } from './insertPanel'
import ParamKnob from './ParamKnob.vue'

const props = defineProps<{
  /** The strip's index in the engine (the synth it carries). */
  s: number
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
defineEmits<{ select: []; open: [] }>()

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
const panText = (p: number) => (Math.abs(p) < 0.02 ? 'C' : `${p < 0 ? 'L' : 'R'}${Math.round(Math.abs(p) * 100)}`)
const toggle = (id: ParamId) => send(id, val(id) >= 0.5 ? 0 : 1)
</script>

<template>
  <div class="strip" :class="{ silenced, selected }" :style="{ '--c': color }">
    <button class="tape" :title="`Select ${title}; double-click for its panel`" @click="$emit('select')" @dblclick="$emit('open')">
      <b>{{ title }}</b><span>{{ subtitle }}</span>
    </button>

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
      <div class="cap">Pan</div>
      <ParamKnob :synth="s" :id="Param.Pan" label="" :name="`${title} pan`" :scale="lin(-1, 1)" :text="panText" :size="36" color="var(--con-pan)" bipolar no-val />
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
.tape {
  align-self: stretch; text-align: center; padding: 7px 2px 6px; border: 0; border-top: 5px solid var(--c); border-radius: 0;
  background: linear-gradient(#262b33, #1f242b); color: inherit; cursor: pointer; font: inherit;
}
.tape:hover:not(:disabled) { border-color: var(--c); background: linear-gradient(#2c323b, #232830); }
.tape b { display: block; font-size: 16px; font-weight: 700; letter-spacing: 0.1em; color: var(--con-paper); text-transform: uppercase; }
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
