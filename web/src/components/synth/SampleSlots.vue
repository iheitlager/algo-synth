<script setup lang="ts">
// The sample store as the view shows it (#125): the slots with their lengths, a button per
// slot to put the sample to use, adding WAV files by choosing or dropping them, and how full
// the store is. The engine parses the bytes; this forwards them (ADR-0001). Shared by the
// multisampler and the pad sampler, which differ in what "use" means.
import { computed, ref } from 'vue'
import { clearSample, loadSample, sampleStore, status } from '../../audio/engine'
import { SAMPLE_SLOTS, freeSlot } from '../../audio/sampler'

defineProps<{
  /** The label and tooltip of the per-slot button that puts a sample to use. */
  useLabel: string
  useTitle: string
  /** Whether a slot is played by a zone or pad, so it cannot be freed. */
  inUse: (slot: number) => boolean
}>()
const emit = defineEmits<{ use: [slot: number]; added: [slot: number] }>()

const loaded = computed(() => sampleStore.slots.flatMap((info, slot) => (info ? [{ slot, info }] : [])))
const memory = computed(() => Math.min(1, sampleStore.used / sampleStore.cap))
const over = ref(false)
const seconds = (frames: number) => (frames / (status.sampleRate || 48_000)).toFixed(2)

async function addFiles(files: File[]) {
  for (const f of files) {
    const slot = freeSlot(sampleStore.slots)
    if (slot < 0) { sampleStore.error = 'all sample slots are used; free some'; return }
    sampleStore.error = ''
    const code = await loadSample(slot, await f.arrayBuffer(), f.name)
    if (code >= 0) emit('added', slot)
  }
}
function onDrop(e: DragEvent) {
  over.value = false
  if (e.dataTransfer?.files.length) void addFiles(Array.from(e.dataTransfer.files))
}
async function pick(e: Event) {
  const input = e.target as HTMLInputElement
  const files = input.files ? Array.from(input.files) : []
  input.value = ''
  await addFiles(files)
}
</script>

<template>
  <div class="box store" :class="{ over }" @dragover.prevent="over = true" @dragleave="over = false" @drop.prevent="onDrop">
    <h4>Samples</h4>
    <label class="file">
      Add WAV, or drop it here
      <input type="file" accept=".wav,audio/wav,audio/x-wav" multiple aria-label="Add WAV samples" @change="pick" />
    </label>
    <ul class="slots">
      <li v-for="{ slot, info } in loaded" :key="slot">
        <span class="name" :title="info.name">{{ info.name }}</span>
        <small>{{ seconds(info.frames) }} s</small>
        <button :title="useTitle" :disabled="!status.running" @click="emit('use', slot)">{{ useLabel }}</button>
        <button :title="inUse(slot) ? 'Used: free it by replacing it first' : 'Free this sample'" :disabled="inUse(slot)" @click="clearSample(slot)">×</button>
      </li>
    </ul>
    <div class="meter" role="meter" aria-label="Sample store memory" :aria-valuenow="Math.round(memory * 100)" aria-valuemin="0" aria-valuemax="100">
      <i :style="{ width: `${memory * 100}%` }" />
    </div>
    <small>{{ loaded.length }} of {{ SAMPLE_SLOTS }} slots · {{ Math.round(memory * 100) }}% of the store</small>
    <p v-if="sampleStore.error" class="err" role="alert">{{ sampleStore.error }}</p>
  </div>
</template>

<style scoped>
.box { flex: 1 1 260px; padding: 8px 10px; border: 1px solid var(--trim); border-radius: 4px; background: color-mix(in srgb, var(--plate) 88%, black 12%); }
h4 { margin: 0 0 6px; font-size: 11px; letter-spacing: 0.16em; text-transform: uppercase; color: var(--c); }
.slots { list-style: none; margin: 6px 0; padding: 0; display: flex; flex-direction: column; gap: 3px; max-height: 150px; overflow: auto; }
.slots li { display: flex; align-items: center; gap: 6px; }
.name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--con-silk); }
small { color: var(--con-silk-dim); }
button { font: inherit; color: var(--con-silk); background: var(--plate); border: 1px solid var(--trim); border-radius: 3px; padding: 2px 8px; cursor: pointer; }
button:disabled { opacity: 0.45; cursor: default; }
.file { display: block; cursor: pointer; padding: 6px 8px; text-align: center; border: 1px dashed var(--trim); border-radius: 3px; color: var(--c); }
.file input { display: none; }
.store.over { outline: 2px dashed var(--c); }
.meter { height: 6px; margin: 4px 0; background: var(--plate); border: 1px solid var(--trim); border-radius: 3px; overflow: hidden; }
.meter i { display: block; height: 100%; background: var(--c); }
.err { margin: 4px 0 0; color: #e66; }
</style>
