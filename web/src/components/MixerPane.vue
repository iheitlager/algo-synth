<script setup lang="ts">
// The mixer (spec 002 Req 2): one strip per synth (fader, pan, four sends,
// mute, solo) and the returns of the send effects. The view only sends values
// and shows what the engine reports back.
import { getEngine, params, status, synthColour, synths } from '../audio/engine'
import { Param, type ParamId } from '../audio/params'

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
// Delay times and the reverb's size are exponential sliders.
const echoMs = (t: number) => 1 * 2000 ** t
const echoPos = (ms: number) => (ms > 1 ? Math.log(ms) / Math.log(2000) : 0)
const sizeS = (t: number) => 0.1 * 100 ** t
const sizePos = (s: number) => (s > 0.1 ? Math.log(s / 0.1) / Math.log(100) : 0)
const slide = (id: ParamId, f: (t: number) => number, e: Event) =>
  getEngine()?.param(0, id, f(Number((e.target as HTMLInputElement).value)))
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
    <div class="returns">
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
    </div>
  </section>
</template>

<style scoped>
.mixer { display: grid; gap: 8px; padding: 8px 12px; font-size: 11px; color: var(--muted); }
.strips { border: 0; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 8px; }
.strip { display: grid; grid-template-columns: repeat(2, minmax(70px, 1fr)); gap: 4px 8px; padding: 6px 8px; background: var(--panel-2); border: 1px solid var(--line); border-radius: 4px; }
.strip b { grid-column: 1 / -1; }
.strip label { display: grid; gap: 2px; }
.strip .sync { display: flex; align-items: center; gap: 4px; }
.returns { display: flex; flex-wrap: wrap; gap: 6px 24px; }
fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: 4px 14px; align-items: center; }
b { color: var(--accent); font-size: 13px; }
label { display: flex; gap: 6px; align-items: center; white-space: nowrap; }
</style>
