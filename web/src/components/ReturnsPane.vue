<script setup lang="ts">
// The send effects (spec 002 Req 2): global echo and reverb returns. Each
// synth's sends are on its card. The view only sends values and shows what
// the engine reports back.
import { getEngine, params, status } from '../audio/engine'
import { Param, type ParamId } from '../audio/params'

const val = (id: ParamId) => params.values[0]?.[id] ?? 0
function send(id: ParamId, e: Event) {
  const t = e.target as HTMLInputElement
  getEngine()?.param(0, id, t.type === 'checkbox' ? Number(t.checked) : Number(t.value))
}
// Delay times and the reverb's size are exponential sliders.
const echoMs = (t: number) => 1 * 2000 ** t
const echoPos = (ms: number) => (ms > 1 ? Math.log(ms) / Math.log(2000) : 0)
const sizeS = (t: number) => 0.1 * 100 ** t
const sizePos = (s: number) => (s > 0.1 ? Math.log(s / 0.1) / Math.log(100) : 0)
const slide = (id: ParamId, f: (t: number) => number, e: Event) =>
  getEngine()?.param(0, id, f(Number((e.target as HTMLInputElement).value)))
</script>

<template>
  <section class="pane returns">
    <fieldset :disabled="!status.running">
      <b>Echo</b>
      <label>Time <input type="range" min="0" max="1" step="0.001" :value="echoPos(val(Param.EchoTime))" @input="slide(Param.EchoTime, echoMs, $event)" /> {{ Math.round(val(Param.EchoTime)) }} ms</label>
      <label>Feedback <input type="range" min="0" max="0.95" step="0.01" :value="val(Param.EchoFeedback)" @input="send(Param.EchoFeedback, $event)" /></label>
      <label>Tone <input type="range" min="0" max="1" step="0.01" :value="val(Param.EchoTone)" @input="send(Param.EchoTone, $event)" /></label>
      <label class="sync"><input type="checkbox" :checked="val(Param.EchoPingPong) >= 0.5" @change="send(Param.EchoPingPong, $event)" /> Ping-pong</label>
      <label>Return <input type="range" min="0" max="1" step="0.01" :value="val(Param.EchoReturn)" @input="send(Param.EchoReturn, $event)" /></label>
    </fieldset>
    <fieldset :disabled="!status.running">
      <b>Reverb</b>
      <label>Size <input type="range" min="0" max="1" step="0.001" :value="sizePos(val(Param.ReverbSize))" @input="slide(Param.ReverbSize, sizeS, $event)" /> {{ val(Param.ReverbSize).toFixed(1) }} s</label>
      <label>Damping <input type="range" min="0" max="1" step="0.01" :value="val(Param.ReverbDamping)" @input="send(Param.ReverbDamping, $event)" /></label>
      <label>Pre-delay <input type="range" min="0" max="100" step="1" :value="val(Param.ReverbPreDelay)" @input="send(Param.ReverbPreDelay, $event)" /></label>
      <label>Return <input type="range" min="0" max="1" step="0.01" :value="val(Param.ReverbReturn)" @input="send(Param.ReverbReturn, $event)" /></label>
    </fieldset>
  </section>
</template>

<style scoped>
.returns { display: flex; flex-wrap: wrap; gap: 6px 24px; padding: 8px 12px; font-size: 11px; color: var(--muted); }
fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
b { color: var(--accent); font-size: 13px; }
label { display: flex; gap: 6px; align-items: center; white-space: nowrap; }
</style>
