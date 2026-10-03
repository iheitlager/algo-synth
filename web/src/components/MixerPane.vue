<script setup lang="ts">
// The mixer (spec 002 Req 2): one strip per synth (fader, pan, four sends,
// mute, solo) and the returns of the send effects. The view only sends values
// and shows what the engine reports back.
import { getEngine, meter, params, status, synthColour, synths } from '../audio/engine'
import { Param, type ParamId, ProcType } from '../audio/params'

const val = (id: ParamId) => params.values[0]?.[id] ?? 0
const sval = (s: number, id: ParamId) => params.values[s]?.[id] ?? 0
const sends = [
  { name: 'P1', id: Param.Send1 }, { name: 'P2', id: Param.Send2 },
  { name: 'P3', id: Param.Send3 }, { name: 'P4', id: Param.Send4 },
]
function strip(s: number, id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement
  getEngine()?.param(s, id, t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
}
function send(id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement
  getEngine()?.param(0, id, t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
}
// The processors P1–P4 (spec 002 Req 2). A knob is 0..1; what it means
// depends on the type, so the labels and readouts come from this table.
const procTypes = Object.entries(ProcType)
interface Knob { label: string; show?: (v: number) => string; toggle?: boolean }
const knobsOf: Record<number, Knob[]> = {
  [ProcType.Off]: [],
  [ProcType.Echo]: [
    { label: 'Time', show: (v) => `${Math.round(2000 ** v)} ms` },
    { label: 'Feedback' }, { label: 'Tone' }, { label: 'Ping-pong', toggle: true },
  ],
  [ProcType.Reverb]: [
    { label: 'Size', show: (v) => `${(0.1 * 100 ** v).toFixed(1)} s` },
    { label: 'Damping' }, { label: 'Pre-delay', show: (v) => `${Math.round(v * 100)} ms` },
  ],
}
const procs = [1, 2, 3, 4].map((n) => {
  const id = (f: string) => Param[`P${n}${f}` as keyof typeof Param]
  return { n, type: id('Type'), ret: id('Return'), knobs: ['A', 'B', 'C', 'D', 'E'].map(id) }
})
const kindOf = (type: ParamId) => val(type)
// The master compressor (spec 002 Req 2): ratio 1 is off.
const comp = [
  { label: 'Threshold', id: Param.CompThreshold, min: -60, max: 0, step: 0.5, unit: 'dB' },
  { label: 'Ratio', id: Param.CompRatio, min: 1, max: 20, step: 0.1, unit: ':1' },
  { label: 'Attack', id: Param.CompAttack, min: 0.1, max: 100, step: 0.1, unit: 'ms' },
  { label: 'Release', id: Param.CompRelease, min: 10, max: 1000, step: 1, unit: 'ms' },
  { label: 'Make-up', id: Param.CompMakeup, min: 0, max: 24, step: 0.5, unit: 'dB' },
]
// The master equalizer (spec 002 Req 2): shelves and two parametric bands.
const eq = [
  { name: 'Low', freq: Param.EqLowFreq, gain: Param.EqLowGain, fmin: 20, fmax: 500 },
  { name: 'Mid 1', freq: Param.EqMid1Freq, gain: Param.EqMid1Gain, q: Param.EqMid1Q, fmin: 100, fmax: 8000 },
  { name: 'Mid 2', freq: Param.EqMid2Freq, gain: Param.EqMid2Gain, q: Param.EqMid2Q, fmin: 500, fmax: 12000 },
  { name: 'High', freq: Param.EqHighFreq, gain: Param.EqHighGain, fmin: 2000, fmax: 18000 },
]
// A frequency slider is exponential: 0..1 spans the band's range.
const hzOf = (t: number, lo: number, hi: number) => lo * (hi / lo) ** t
const posOf = (hz: number, lo: number, hi: number) => Math.log(hz / lo) / Math.log(hi / lo)
function sendFreq(id: ParamId, lo: number, hi: number, e: Event) {
  getEngine()?.param(0, id, hzOf(Number((e.target as HTMLInputElement).value), lo, hi))
}
</script>

<template>
  <section class="pane mixer">
    <fieldset class="strips" :disabled="!status.running">
      <div v-for="s in synths.list" :key="s" class="strip">
        <b :style="{ color: synthColour(s) }">Synth {{ s + 1 }}</b>
        <label>Level <input type="range" min="0" max="1" step="0.01" :value="sval(s, Param.Level)" @input="strip(s, Param.Level, $event)" /></label>
        <label>Pan <input type="range" min="-1" max="1" step="0.01" :value="sval(s, Param.Pan)" @input="strip(s, Param.Pan, $event)" /></label>
        <label v-for="send in sends" :key="send.name">{{ send.name }} <input type="range" min="0" max="1" step="0.01" :value="sval(s, send.id)" @input="strip(s, send.id, $event)" /></label>
        <label class="sync"><input type="checkbox" :checked="sval(s, Param.Mute) >= 0.5" @change="strip(s, Param.Mute, $event)" /> Mute</label>
        <label class="sync"><input type="checkbox" :checked="sval(s, Param.Solo) >= 0.5" @change="strip(s, Param.Solo, $event)" /> Solo</label>
      </div>
    </fieldset>
    <fieldset class="returns" :disabled="!status.running">
      <div v-for="p in procs" :key="p.n" class="proc">
        <b>P{{ p.n }}</b>
        <select :value="val(p.type)" @change="send(p.type, $event)">
          <option v-for="[name, id] in procTypes" :key="id" :value="id">{{ name }}</option>
        </select>
        <template v-if="kindOf(p.type) > 0">
          <template v-for="(k, i) in knobsOf[kindOf(p.type)]" :key="i">
            <label v-if="k.toggle" class="sync"><input type="checkbox" :checked="val(p.knobs[i] as ParamId) >= 0.5" @change="send(p.knobs[i] as ParamId, $event)" /> {{ k.label }}</label>
            <label v-else>{{ k.label }} <input type="range" min="0" max="1" step="0.001" :value="val(p.knobs[i] as ParamId)" @input="send(p.knobs[i] as ParamId, $event)" /> <span v-if="k.show">{{ k.show(val(p.knobs[i] as ParamId)) }}</span></label>
          </template>
          <label>Return <input type="range" min="0" max="1" step="0.01" :value="val(p.ret)" @input="send(p.ret, $event)" /></label>
        </template>
      </div>
    </fieldset>
    <fieldset class="master" :disabled="!status.running">
      <b>EQ</b>
      <span v-for="b in eq" :key="b.name" class="band">
        {{ b.name }}
        <label>Freq <input type="range" min="0" max="1" step="0.001" :value="posOf(val(b.freq), b.fmin, b.fmax)" @input="sendFreq(b.freq, b.fmin, b.fmax, $event)" /> {{ Math.round(val(b.freq)) }} Hz</label>
        <label>Gain <input type="range" min="-15" max="15" step="0.5" :value="val(b.gain)" @input="send(b.gain, $event)" /> {{ Number(val(b.gain).toFixed(1)) }} dB</label>
        <label v-if="b.q">Q <input type="range" min="0.3" max="8" step="0.1" :value="val(b.q)" @input="send(b.q, $event)" /></label>
      </span>
    </fieldset>
    <fieldset class="master" :disabled="!status.running">
      <b>Compressor</b>
      <label v-for="c in comp" :key="c.label">{{ c.label }}
        <input type="range" :min="c.min" :max="c.max" :step="c.step" :value="val(c.id)" @input="send(c.id, $event)" />
        {{ Number(val(c.id).toFixed(1)) }} {{ c.unit }}
      </label>
      <span title="How far the compressor is turning the master down">GR <b>{{ meter.reduction.toFixed(1) }}</b> dB</span>
    </fieldset>
  </section>
</template>

<style scoped>
.mixer { display: grid; gap: 8px; padding: 8px 12px; font-size: 11px; color: var(--muted); }
.strips { border: 0; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 8px; }
.strip { display: grid; grid-template-columns: repeat(2, minmax(70px, 1fr)); gap: 4px 8px; padding: 6px 8px; background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; }
.strip b { grid-column: 1 / -1; }
.strip label { display: grid; gap: 2px; }
.strip .sync { display: flex; align-items: center; gap: 4px; }
.returns { border: 0; margin: 0; padding: 0; display: grid; gap: 4px; }
.master { border: 0; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
.master b:first-child { color: var(--accent); font-size: 13px; }
.band { display: flex; flex-wrap: wrap; gap: 4px 8px; align-items: center; padding-right: 10px; border-right: 1px solid var(--line); }
.proc { display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
.proc b { color: var(--accent); font-size: 13px; width: 2em; }
label { display: flex; gap: 6px; align-items: center; white-space: nowrap; }
</style>
