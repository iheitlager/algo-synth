<script setup lang="ts">
// The synths (spec 003 Req 3 and 9, spec 005): a rail of tapes, one per synth
// (up to 16, each one of seven instruments), and the selected one as a faceplate
// in its model's palette with a keyboard under it. The selected synth gets the
// keys, the computer keyboard's too.
import { computed, onBeforeUnmount, onMounted, reactive } from 'vue'
import { MAX_SYNTHS, addSynth, getEngine, params, player, removeSynth, renameSynth, status, stripName, synthColour, synths } from '../audio/engine'
import { MODELS, familyModels, modelDef, type ModelDef } from '../audio/models'
import { Model, Param, Preset, type PresetId } from '../audio/params'
import EditableName from './EditableName.vue'
import PresetBar from './PresetBar.vue'
import type { UserPreset } from '../audio/presets'
import SynthFaceplate from './SynthFaceplate.vue'
import Keyboard from './synth/Keyboard.vue'
import SynthRail, { type Tape } from './synth/SynthRail.vue'

// The view shows what the engine reports (`params`), so a preset or a model
// change moves the knobs.
const val = (s: number, id: number) => params.values[s]?.[id] ?? 0
const defOf = (s: number): ModelDef => modelDef(val(s, Param.Model))
const anySolo = computed(() => synths.list.some((s) => val(s, Param.Solo) >= 0.5))
const tapes = computed<Tape[]>(() =>
  synths.list.map((s) => {
    const channels = player.parts.filter((p) => p.synth === s).map((p) => p.channel + 1)
    return {
      s,
      name: stripName(s),
      model: defOf(s).name,
      accent: defOf(s).theme.accent,
      dot: synthColour(s),
      footer: channels.length ? `Ch ${channels.join('·')}` : '—',
      silenced: val(s, Param.Mute) >= 0.5 || (anySolo.value && val(s, Param.Solo) < 0.5),
    }
  }),
)
const sel = computed(() => (synths.list.includes(synths.selected) ? synths.selected : (synths.list[0] ?? 0)))
const def = computed(() => defOf(sel.value))
const accent = computed(() => ({ '--c': def.value.theme.accent }))
/** The model's name as presets store it (`Model`'s key). */
const modelName = computed(() => Object.entries(Model).find(([, id]) => id === def.value.id)?.[0])
/** A synth keeps its family, as the model picker does: only presets of models in it. */
const sameFamily = (p: UserPreset) => p.model !== undefined && modelDef(Model[p.model as keyof typeof Model]).family === def.value.family
// Choosing a model loads its first preset, so the synth is one consistent sound.
function loadModel(s: number, e: Event) {
  const select = e.target as HTMLSelectElement
  const first = MODELS.find((m) => m.id === Number(select.value))?.presets[0]
  if (first !== undefined) getEngine()?.preset(s, Preset[first])
  select.blur()
}

// Keys sounding now, to light on the keyboard: from the mouse or the computer keyboard.
const lit = reactive(new Set<number>())
const down = (s: number, n: number) => { synths.selected = s; getEngine()?.noteOn(s, n); lit.add(n) }
const up = (s: number, n: number) => { getEngine()?.noteOff(s, n); lit.delete(n) }

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
  <section class="pane stage" aria-label="Synths">
    <SynthRail :tapes="tapes" :selected="sel" :can-add="synths.list.length < MAX_SYNTHS" @select="synths.selected = $event" @add="(m) => addSynth(m)" @rename="renameSynth" />
    <div class="face" :style="accent">
      <header>
        <div class="title">
          <h2><EditableName :value="stripName(sel)" :label="stripName(sel)" @rename="renameSynth(sel, $event)" /></h2>
          <span :title="def.tagline">{{ def.name }} · {{ def.maker }}</span>
        </div>
        <div class="pick">
          <label>Model
            <select :disabled="!status.running" :value="def.id" @change="loadModel(sel, $event)">
              <option v-for="m in familyModels(def.family)" :key="m.id" :value="m.id">{{ m.name }}</option>
            </select>
          </label>
          <PresetBar
            :target="{ kind: 'synth', s: sel }" :of="{ model: modelName }" :allow="sameFamily" :factory="def.presets.map((n) => [n, Preset[n]])"
            @factory="(v) => getEngine()?.preset(sel, v as PresetId)"
          />
          <span class="hint">{{ status.running ? 'keys A–; play it' : 'power on to play' }}</span>
          <button class="remove" title="Remove this synth; parts on it are muted" :disabled="synths.list.length <= 1" @click="onRemove(sel)">× Remove</button>
        </div>
      </header>
      <div class="scroll">
        <SynthFaceplate :key="sel" :s="sel" :def="def" />
      </div>
      <Keyboard :lit="lit" @down="down(sel, $event)" @up="up(sel, $event)" />
    </div>
  </section>
</template>

<style scoped>
.stage { display: flex; align-items: stretch; overflow: hidden; background: var(--con-chassis); border-color: var(--con-line); }
.face { flex: 1; min-width: 0; display: flex; flex-direction: column; }
header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 8px 14px; border-bottom: 1px solid #0b0d10; background: var(--con-panel); box-shadow: 0 1px 0 #2a303a inset; font-family: var(--con-font-silk); }
.title { display: flex; align-items: baseline; gap: 12px; min-width: 0; }
h2 { margin: 0; font-size: 22px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; color: var(--con-paper); white-space: nowrap; }
.title span { color: var(--con-silk-dim); font-size: 12px; letter-spacing: 0.12em; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pick { display: flex; gap: 14px; align-items: center; font-size: 11px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); }
.pick label { display: flex; gap: 6px; align-items: center; }
.pick select { font: 500 13px var(--con-font-silk); letter-spacing: 0.06em; background: var(--con-inset); color: var(--con-paper); border: 1px solid #343b46; border-radius: 3px; padding: 3px 6px; }
.hint { font-style: italic; }
.remove { font: 600 12px var(--con-font-silk); letter-spacing: 0.1em; text-transform: uppercase; }
.scroll { flex: 1; min-height: 0; overflow: auto; padding: 10px 12px; }
</style>
