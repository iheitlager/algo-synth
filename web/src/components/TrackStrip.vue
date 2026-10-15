<script setup lang="ts">
// The song's tracks in the composer (#213): each track's synth, model and
// preset, and Save as setting. A choice is a message (`trackEdit`, `routeTrack`);
// the engine rewrites the track line, sets the synth and prints the song back.
import { computed, onMounted } from 'vue'
import { MUTE, fetchPacks, kitOn, packs, params, pickKit, routeTrack, song, synthColour, trackEdit, type Route, type SongTrack } from '../audio/engine'
import { modelDef } from '../audio/models'
import { Model, Param } from '../audio/params'
import { choiceOf, modelOfPreset, modelsFor, parseChoice, presetChoices } from '../audio/trackpick'

defineProps<{ synths: { label: string; value: Route }[] }>()

// The kits are a Pad Sampler track's presets (#596).
onMounted(() => { if (!packs.loaded) void fetchPacks() })

// What a track plays: its preset's model, else (a sampler keeping its samples) its synth's.
const modelOf = (t: SongTrack) =>
  modelOfPreset(t.preset) ?? modelDef(t.synth === MUTE ? 0 : (params.values[t.synth]?.[Param.Model] ?? 0))

const rows = computed(() =>
  song.tracks.map((t, i) => {
    const model = modelOf(t)
    const presets = presetChoices(model, song.settings, packs.kits)
    // A Pad Sampler track shows its kit (#596); a choice not in the list shows none.
    const choice = choiceOf(t, model.id === Model.PadSampler ? kitOn(t.synth) : '')
    return { t, i, model, models: modelsFor(t.kind, song.fits), presets, choice: presets.some((p) => p.value === choice) ? choice : '' }
  }),
)

// A new model starts on its first factory preset.
function pickModel(i: number, id: number) {
  const first = presetChoices(modelDef(id), [])[0]
  const c = first ? parseChoice(first.value) : null
  if (c?.kind === 'preset') trackEdit.preset(i, c.id)
}
function pickPreset(i: number, value: string) {
  const c = parseChoice(value)
  if (c?.kind === 'preset') trackEdit.preset(i, c.id)
  else if (c?.kind === 'setting') trackEdit.setting(i, c.id)
  else if (c?.kind === 'kit') pickKit(song.tracks[i].synth, c.id)
}
</script>

<template>
  <div class="tracks" aria-label="Tracks">
    <div v-for="r in rows" :key="r.i" class="track" :style="{ '--c': synthColour(r.t.synth === MUTE ? 0 : r.t.synth) }">
      <b class="name">{{ r.t.name }}</b>
      <span class="muted kind">{{ r.t.kind }}</span>
      <label title="The synth the track plays on">
        <select class="picker" :value="r.t.synth" @change="routeTrack(r.i, Number(($event.target as HTMLSelectElement).value) as Route)">
          <option v-for="c in synths" :key="c.value" :value="c.value">{{ c.label }}</option>
        </select>
      </label>
      <label title="The track's instrument: the models that play this kind of track">
        <select class="picker" :value="r.model.id" :disabled="r.t.synth === MUTE" @change="pickModel(r.i, Number(($event.target as HTMLSelectElement).value))">
          <option v-for="m in r.models" :key="m.id" :value="m.id">{{ m.name }}</option>
        </select>
      </label>
      <label title="A factory preset of the model, or one of the song's settings">
        <select class="picker" :value="r.choice" :disabled="r.t.synth === MUTE" @change="pickPreset(r.i, ($event.target as HTMLSelectElement).value)">
          <option v-if="!r.choice" value="" disabled>samples as loaded</option>
          <option v-for="p in r.presets" :key="p.value" :value="p.value">{{ p.label }}</option>
        </select>
      </label>
    </div>
  </div>
</template>

<style scoped>
.tracks { display: flex; flex-direction: column; gap: 4px; padding: 6px 12px 0; }
/* One grid for every row, so each column lines up whatever its text:
   name, kind, synth, model, preset. */
.track {
  display: grid; grid-template-columns: 9em 4em 17em 10em 14em; align-items: center; gap: 8px;
  padding: 2px 0 2px 8px; border-left: 3px solid var(--c);
}
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.kind { font-size: 11px; }
.muted { color: var(--muted); }
.track label { display: block; min-width: 0; }
.track label .picker { width: 100%; min-width: 0; max-width: none; }
</style>
