<script setup lang="ts">
// The synths (spec 004, spec 005): up to 16, each one of six instruments with
// a panel and palette of its model. The selected one gets the keys.
import { computed, onBeforeUnmount, onMounted } from 'vue'
import { MAX_SYNTHS, addSynth, getEngine, params, removeSynth, status, synthColour, synths } from '../audio/engine'
import { MODELS, modelDef, type ModelDef } from '../audio/models'
import { Param, Preset, type PresetId } from '../audio/params'
import SynthPanel from './SynthPanel.vue'

const keys = Array.from({ length: 25 }, (_, i) => 48 + i)
const black = (n: number) => [1, 3, 6, 8, 10].includes(n % 12)

// The view shows what the engine reports (`params`), so a preset or a model
// change moves the sliders.
const defOf = (s: number): ModelDef => modelDef(params.values[s]?.[Param.Model] ?? 0)
const cards = computed(() => synths.list.map((s) => ({ s, def: defOf(s) })))
const themeOf = (def: ModelDef, s: number) => ({
  '--c': def.theme.accent, '--card': def.theme.panel, '--ink': def.theme.ink,
  '--soft': def.theme.soft, '--trim': def.theme.trim, '--dot': synthColour(s),
})
function loadPreset(s: number, e: Event) {
  const select = e.target as HTMLSelectElement
  if (select.value !== '') getEngine()?.preset(s, Number(select.value) as PresetId)
  // Hand the keys back to the computer keyboard.
  select.blur()
}
// Choosing a model loads its first preset, so the synth is one consistent sound.
function loadModel(s: number, e: Event) {
  const select = e.target as HTMLSelectElement
  const first = MODELS.find((m) => m.id === Number(select.value))?.presets[0]
  if (first !== undefined) getEngine()?.preset(s, Preset[first])
  select.blur()
}

const down = (s: number, n: number) => { synths.selected = s; getEngine()?.noteOn(s, n) }
const up = (s: number, n: number) => getEngine()?.noteOff(s, n)

// Computer keyboard: the bottom two rows play C4..E5 on the selected synth.
// A held key remembers its synth, so selecting another can't strand a note.
const map = 'awsedftgyhujkolp;'
const held = new Map<string, number>()
function onKey(e: KeyboardEvent, isDown: boolean) {
  if (e.repeat || e.metaKey || e.ctrlKey || ['INPUT', 'SELECT'].includes((e.target as HTMLElement).tagName)) return
  const i = map.indexOf(e.key)
  if (i < 0) return
  const n = 60 + i
  if (isDown && !held.has(e.key)) { held.set(e.key, synths.selected); down(synths.selected, n) }
  const s = held.get(e.key)
  if (!isDown && s !== undefined) { held.delete(e.key); up(s, n) }
}
const kd = (e: KeyboardEvent) => onKey(e, true)
const ku = (e: KeyboardEvent) => onKey(e, false)
onMounted(() => { window.addEventListener('keydown', kd); window.addEventListener('keyup', ku) })
onBeforeUnmount(() => { window.removeEventListener('keydown', kd); window.removeEventListener('keyup', ku) })

// Removing a synth releases the keys still held on it.
function onRemove(s: number) {
  for (const [key, synth] of held) {
    if (synth !== s) continue
    held.delete(key)
    up(s, 60 + map.indexOf(key))
  }
  removeSynth(s)
}
</script>

<template>
  <section class="pane">
    <div class="pane-head">
      <span>Synths · {{ synths.list.length }}/{{ MAX_SYNTHS }}</span>
      <span class="head-right">
        <span class="soon">{{ status.running ? 'keys A–; play the selected synth' : 'power on to play' }}</span>
        <button :disabled="!status.running || synths.list.length >= MAX_SYNTHS" @click="addSynth">+ Synth</button>
      </span>
    </div>
    <div class="cards">
      <article
        v-for="{ s, def } in cards" :key="s" class="card"
        :class="{ sel: synths.selected === s }" :style="themeOf(def, s)"
        @click="synths.selected = s"
      >
        <header>
          <i class="dot" :title="`synth ${s + 1}`" />
          <b :title="def.tagline">{{ def.name }}</b><span class="style">{{ def.maker }}</span>
          <button
            class="remove" title="Remove this synth; parts on it are muted"
            :disabled="synths.list.length <= 1" @click.stop="onRemove(s)"
          >×</button>
        </header>
        <div class="pick" @click.stop>
          <label>Model
            <select :disabled="!status.running" :value="def.id" @change="loadModel(s, $event)">
              <option v-for="m in MODELS" :key="m.id" :value="m.id">{{ m.name }}</option>
            </select>
          </label>
          <label>Preset
            <select :disabled="!status.running" @change="loadPreset(s, $event)">
              <option value="">—</option>
              <option v-for="name in def.presets" :key="name" :value="Preset[name]">{{ name }}</option>
            </select>
          </label>
        </div>
        <SynthPanel :s="s" :def="def" />
        <div class="kbd">
          <span
            v-for="n in keys" :key="n" class="key" :class="{ black: black(n) }"
            @pointerdown="down(s, n)" @pointerup="up(s, n)" @pointerleave="up(s, n)"
          />
        </div>
      </article>
    </div>
  </section>
</template>

<style scoped>
.cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(380px, 1fr)); gap: 8px; padding: 8px; }
.card { background: var(--card); color: var(--soft); border: 1px solid var(--trim); border-top: 3px solid var(--c); border-radius: 4px; padding: 10px; display: grid; gap: 10px; align-content: start; cursor: pointer; }
.card.sel { box-shadow: 0 0 0 1px var(--c); }
header { display: flex; align-items: baseline; gap: 8px; }
header b { color: var(--ink); font-size: 15px; }
.dot { width: 8px; height: 8px; border-radius: 50%; background: var(--dot); align-self: center; }
.remove { margin-left: auto; padding: 0 8px; line-height: 1.4; }
.head-right { display: flex; gap: 10px; align-items: center; }
.style { color: var(--soft); font-size: 11px; }
.pick { display: flex; gap: 12px; font-size: 11px; }
.pick label { display: flex; gap: 6px; align-items: center; }
.kbd { display: flex; height: 64px; gap: 1px; }
.key { flex: 1; background: #d9dde3; border-radius: 0 0 3px 3px; }
.key.black { background: #2b2f36; height: 62%; }
.key:active { background: var(--c); }
</style>
