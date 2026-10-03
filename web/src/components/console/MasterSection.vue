<script setup lang="ts">
// The master section (#54): equalizer, compressor with its gain-reduction
// meter, and the master fader with stereo meters and the limiter light.
import { computed } from 'vue'
import { dbText, exp, hzText, levelToPos, lin, posToDb, posToLevel, type EqBand } from '../../audio/console'
import { getEngine, levels, meter, METER_SYNTHS, params } from '../../audio/engine'
import { Param, type ParamId } from '../../audio/params'
import Fader from './Fader.vue'
import LedMeter from './LedMeter.vue'
import CompPlot from './CompPlot.vue'
import EqCurve from './EqCurve.vue'
import ParamKnob from './ParamKnob.vue'

const val = (id: ParamId) => params.values[0]?.[id] ?? 0
const gain = lin(-15, 15)
const gainText = (v: number) => `${v > 0 ? '+' : ''}${v.toFixed(1)} dB`
const bandsDef = [
  { name: 'Low', type: 'low' as const, freq: Param.EqLowFreq, gain: Param.EqLowGain, range: exp(20, 500), q: undefined, fq: 0.7 },
  { name: 'Mid 1', type: 'peak' as const, freq: Param.EqMid1Freq, gain: Param.EqMid1Gain, range: exp(100, 8000), q: Param.EqMid1Q, fq: 1 },
  { name: 'Mid 2', type: 'peak' as const, freq: Param.EqMid2Freq, gain: Param.EqMid2Gain, range: exp(500, 12_000), q: Param.EqMid2Q, fq: 1 },
  { name: 'High', type: 'high' as const, freq: Param.EqHighFreq, gain: Param.EqHighGain, range: exp(2000, 18_000), q: undefined, fq: 0.7 },
]
const defaults: Record<string, number> = { Low: 100, 'Mid 1': 500, 'Mid 2': 3000, High: 8000 }
const qScale = exp(0.3, 8)
const bands = computed<EqBand[]>(() =>
  bandsDef.map((b) => ({ type: b.type, freq: val(b.freq), gainDb: val(b.gain), q: b.q ? val(b.q) : b.fq })),
)

const compKnobs = [
  { label: 'Thresh', id: Param.CompThreshold, scale: lin(-60, 0), def: -12, text: (v: number) => `${dbText(v, 0)} dB` },
  { label: 'Ratio', id: Param.CompRatio, scale: exp(1, 20), def: 1, text: (v: number) => `${v.toFixed(1)}:1` },
  { label: 'Attack', id: Param.CompAttack, scale: exp(0.1, 100), def: 10, text: (v: number) => `${v.toFixed(1)} ms` },
  { label: 'Release', id: Param.CompRelease, scale: exp(10, 1000), def: 120, text: (v: number) => `${Math.round(v)} ms` },
  { label: 'Make-up', id: Param.CompMakeup, scale: lin(0, 24), def: 0, text: (v: number) => `+${v.toFixed(1)} dB` },
]

const pos = computed(() => levelToPos(val(Param.MasterGain)))
const setPos = (p: number) => getEngine()?.param(0, Param.MasterGain, posToLevel(p))
const readout = computed(() => `${dbText(posToDb(pos.value))} dB`)
const left = computed(() => levels.values[METER_SYNTHS] ?? 0)
const right = computed(() => levels.values[METER_SYNTHS + 1] ?? 0)
const limiting = computed(() => Math.max(left.value, right.value) > 0.98)
const grFill = computed(() => `${Math.min(1, Math.max(0, meter.reduction / 20)) * 100}%`)
</script>

<template>
  <div class="master-grid">
    <div class="left">
      <section class="msec" style="--c: var(--con-eq)">
        <h3>Equalizer</h3>
        <EqCurve :bands="bands" />
        <div class="bands">
          <div v-for="b in bandsDef" :key="b.name" class="band">
            <div class="bn">{{ b.name }}</div>
            <ParamKnob :synth="0" :id="b.gain" label="Gain" :name="`${b.name} gain`" :scale="gain" :text="gainText" :size="40" color="var(--con-eq)" bipolar :def="0" />
            <ParamKnob :synth="0" :id="b.freq" label="Freq" :name="`${b.name} frequency`" :scale="b.range" :text="hzText" :size="30" color="var(--con-eq)" :def="defaults[b.name]" />
            <ParamKnob v-if="b.q" :synth="0" :id="b.q" label="Q" :name="`${b.name} Q`" :scale="qScale" :text="(v) => v.toFixed(1)" :size="30" color="var(--con-eq)" :def="1" />
          </div>
        </div>
      </section>
      <section class="msec" style="--c: var(--con-comp)">
        <h3>Compressor</h3>
        <div class="top">
          <CompPlot :threshold="val(Param.CompThreshold)" :ratio="val(Param.CompRatio)" :makeup="val(Param.CompMakeup)" />
          <div class="gr"><div class="grm"><div :style="{ height: grFill }" /></div><span>GR dB</span></div>
        </div>
        <div class="comp">
          <ParamKnob v-for="k in compKnobs" :key="k.id" :synth="0" :id="k.id" :label="k.label" :name="`Compressor ${k.label}`" :scale="k.scale" :text="k.text" :def="k.def" :size="40" color="var(--con-comp)" />
        </div>
      </section>
    </div>
    <div class="out">
      <div class="ch">Master</div>
      <div class="fader-wrap">
        <Fader :model-value="pos" label="Master level" :def="0.8" @update:model-value="setPos" />
        <div class="meters"><LedMeter :level="left" :height="300" :width="11" :segs="36" /><LedMeter :level="right" :height="300" :width="11" :segs="36" /></div>
      </div>
      <div class="readout">{{ readout }}</div>
      <div class="lim" :class="{ hit: limiting }">Limit</div>
    </div>
  </div>
</template>

<style scoped>
.master-grid { display: grid; grid-template-columns: 1fr auto; gap: 12px; font-family: var(--con-font-silk); }
.msec { background: linear-gradient(#20252d, #1b1f26); border: 1px solid #2b323c; border-radius: 4px; padding: 8px 10px 10px; margin-bottom: 10px; }
h3 { margin: 0 0 6px; font-size: 13px; letter-spacing: 0.2em; text-transform: uppercase; font-weight: 700; color: var(--c); }
.bands { display: grid; grid-template-columns: repeat(4, 1fr); gap: 4px; justify-items: center; align-items: start; margin-top: 6px; }
.band { display: flex; flex-direction: column; align-items: center; gap: 5px; }
.bn { font-size: 11px; letter-spacing: 0.16em; color: var(--con-silk-dim); text-transform: uppercase; }
.top { display: grid; grid-template-columns: 1fr auto; gap: 10px; align-items: start; }
.gr { display: flex; flex-direction: column; align-items: center; gap: 3px; height: 150px; }
.grm { flex: 1; width: 12px; background: var(--con-inset); border-radius: 2px; position: relative; overflow: hidden; }
.grm div { position: absolute; left: 0; right: 0; top: 0; background: var(--con-comp); }
.gr span { font-size: 10px; letter-spacing: 0.14em; color: var(--con-silk-dim); text-transform: uppercase; }
.comp { display: grid; grid-template-columns: repeat(3, 1fr); gap: 6px 4px; justify-items: center; margin-top: 8px; }
.out { display: flex; flex-direction: column; align-items: center; gap: 8px; }
.out .fader-wrap { display: flex; gap: 5px; align-items: stretch; height: 300px; }
.meters { display: flex; gap: 3px; }
.ch { font-size: 11px; letter-spacing: 0.16em; color: var(--con-silk-dim); text-transform: uppercase; }
.readout { font: 500 11px var(--con-font-mono); color: var(--con-silk); background: var(--con-inset); border-radius: 3px; padding: 2px 6px; min-width: 52px; text-align: center; }
.lim { font-size: 11px; letter-spacing: 0.16em; color: var(--con-silk-dim); padding: 2px 8px; border-radius: 3px; background: var(--con-inset); border: 1px solid #343b46; text-transform: uppercase; }
.lim.hit { color: #fff; background: #6a1d14; border-color: var(--con-led-r); box-shadow: 0 0 10px #f0503a88; }
</style>
