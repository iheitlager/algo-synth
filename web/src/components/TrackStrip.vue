<script setup lang="ts">
// The song's tracks in the composer (#213): each track's synth, model and
// preset, and Save as setting. A choice is a message (`trackEdit`, `routeTrack`);
// the engine rewrites the track line, sets the synth and prints the song back.
import { computed } from 'vue'
import { MUTE, params, routeTrack, song, synthColour, trackEdit, type Route, type SongTrack } from '../audio/engine'
import { modelDef } from '../audio/models'
import { Param } from '../audio/params'
import { choiceOf, modelOfPreset, modelsFor, parseChoice, presetChoices } from '../audio/trackpick'

defineProps<{ synths: { label: string; value: Route }[] }>()

// What a track plays: its preset's model, else (a sampler keeping its samples) its synth's.
const modelOf = (t: SongTrack) =>
  modelOfPreset(t.preset) ?? modelDef(t.synth === MUTE ? 0 : (params.values[t.synth]?.[Param.Model] ?? 0))

const rows = computed(() =>
  song.tracks.map((t, i) => {
    const model = modelOf(t)
    return { t, i, model, models: modelsFor(t.kind, song.fits), presets: presetChoices(model, song.settings) }
  }),
)

// A new model starts on its first factory preset.
function pickModel(i: number, id: number) {
  const first = presetChoices(modelDef(id), [])[0]
  const c = first ? parseChoice(first.value) : null
  if (c) trackEdit.preset(i, c.id)
}
function pickPreset(i: number, value: string) {
  const c = parseChoice(value)
  if (c?.kind === 'preset') trackEdit.preset(i, c.id)
  else if (c?.kind === 'setting') trackEdit.setting(i, c.id)
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
        <select class="picker" :value="choiceOf(r.t)" :disabled="r.t.synth === MUTE" @change="pickPreset(r.i, ($event.target as HTMLSelectElement).value)">
          <option v-if="!choiceOf(r.t)" value="" disabled>samples as loaded</option>
          <option v-for="p in r.presets" :key="p.value" :value="p.value">{{ p.label }}</option>
        </select>
      </label>
      <button
        :disabled="r.t.synth === MUTE || r.t.preset < 0"
        title="Write the synth's sound into the song as a setting the track plays"
        @click="trackEdit.save(r.i)"
      >Save as setting</button>
    </div>
  </div>
</template>

<style scoped>
.tracks { display: flex; flex-direction: column; gap: 4px; padding: 6px 12px 0; }
.track { display: flex; align-items: center; gap: 8px; padding: 2px 0 2px 8px; border-left: 3px solid var(--c); }
.name { width: 10em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.kind { width: 4.5em; font-size: 11px; }
.muted { color: var(--muted); }
</style>
