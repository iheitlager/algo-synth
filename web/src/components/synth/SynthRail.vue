<script setup lang="ts">
// The rail of synth tapes beside the faceplate (spec 003 Req 9): one per
// synth, with its model, its meter, mute and solo. A click selects it for the
// keyboard. Nothing here is decided: it shows what the engine reports.
import { getEngine, levels, params, status } from '../../audio/engine'
import { Param, type ParamId } from '../../audio/params'
import LedMeter from '../console/LedMeter.vue'

export interface Tape {
  s: number
  /** The synth's name (#127), its model's name and the model's accent. */
  name: string
  model: string
  accent: string
  /** The synth's own colour, as on the MIDI player's chips. */
  dot: string
  /** Where it is routed from, e.g. "Ch 1 · 3". */
  footer: string
  silenced: boolean
}

defineProps<{ tapes: Tape[]; selected: number; canAdd: boolean }>()
defineEmits<{ select: [s: number]; add: [] }>()

const val = (s: number, id: ParamId) => params.values[s]?.[id] ?? 0
const toggle = (s: number, id: ParamId) => getEngine()?.param(s, id, val(s, id) >= 0.5 ? 0 : 1)
const level = (s: number) => levels.values[s] ?? 0
</script>

<template>
  <nav class="rail" aria-label="Synths">
    <div
      v-for="t in tapes" :key="t.s" class="tape" :class="{ sel: selected === t.s, silenced: t.silenced }"
      :style="{ '--c': t.accent, '--dot': t.dot }"
    >
      <button class="pick" :aria-pressed="selected === t.s" :title="`Play ${t.name}`" @click="$emit('select', t.s)">
        <b><i class="dot" aria-hidden="true" />{{ t.name }}</b>
        <span>{{ t.model }} · {{ t.footer }}</span>
      </button>
      <div class="side">
        <LedMeter :level="level(t.s)" :height="46" :width="6" :segs="12" />
        <div class="ms">
          <button class="m" :aria-pressed="val(t.s, Param.Mute) >= 0.5" :disabled="!status.running" :aria-label="`Mute synth ${t.s + 1}`" title="Mute" @click="toggle(t.s, Param.Mute)">M</button>
          <button class="s" :aria-pressed="val(t.s, Param.Solo) >= 0.5" :disabled="!status.running" :aria-label="`Solo synth ${t.s + 1}`" title="Solo" @click="toggle(t.s, Param.Solo)">S</button>
        </div>
      </div>
    </div>
    <button class="add" :disabled="!status.running || !canAdd" @click="$emit('add')">+ Synth</button>
  </nav>
</template>

<style scoped>
.rail { display: flex; flex-direction: column; gap: 1px; width: 176px; flex: 0 0 176px; overflow-y: auto; background: var(--con-chassis); border-right: 2px solid #0b0d10; font-family: var(--con-font-silk); }
.tape { display: flex; align-items: stretch; gap: 4px; background: var(--con-panel); border-left: 5px solid var(--c); box-shadow: 0 1px 0 #0d0f13; }
.tape:nth-child(even) { background: #1b2028; }
.tape.sel { background: linear-gradient(#323843, #272c34); }
.tape.silenced .pick { opacity: 0.5; }
.pick { flex: 1; min-width: 0; text-align: left; padding: 7px 4px 7px 8px; background: none; border: 0; border-radius: 0; color: inherit; cursor: pointer; font: inherit; display: grid; gap: 1px; }
.pick:focus-visible { outline: 2px solid var(--con-paper); outline-offset: -2px; }
.pick b { color: var(--con-paper); font-size: 15px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pick span { color: var(--con-silk-dim); font-size: 11px; letter-spacing: 0.1em; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--dot); margin-right: 7px; vertical-align: 1px; }
.side { display: flex; gap: 5px; align-items: center; padding: 4px 6px 4px 0; }
.ms { display: flex; flex-direction: column; gap: 3px; }
.ms button {
  width: 20px; height: 20px; padding: 0; border-radius: 3px; border: 1px solid #39414d; cursor: pointer; background: linear-gradient(#2e343d, #232830);
  font: 700 11px var(--con-font-silk); color: var(--con-silk); box-shadow: 0 1px 0 #0a0c0f;
}
.ms button:disabled { opacity: 0.5; cursor: default; }
.ms .m[aria-pressed='true'] { background: #7a2a22; color: #fff; border-color: #e0654f; box-shadow: 0 0 8px #e0654f55; }
.ms .s[aria-pressed='true'] { background: #8a6a10; color: #fff; border-color: #f0c23a; box-shadow: 0 0 8px #f0c23a55; }
.add { margin: 6px; padding: 6px; font: 600 12px var(--con-font-silk); letter-spacing: 0.12em; text-transform: uppercase; background: var(--con-panel-2); color: var(--con-silk); border: 1px dashed #39414d; border-radius: 3px; cursor: pointer; }
.add:hover:not(:disabled) { color: var(--con-paper); border-color: var(--con-silk-dim); }
</style>
