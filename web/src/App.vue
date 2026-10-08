<script setup lang="ts">
// Wide-screen layout (spec 003): transport on top, the synths, the mixer
// console or the composer, with the arranger across the bottom, and the
// Assistant beside them when shown (#387).
import { onBeforeUnmount, onMounted, watch } from 'vue'
import { loadSong, song, status, synths, view } from './audio/engine'
import { CHANNEL, assistant, serveWindow, type AssistHost, type LinkState } from './audio/assistlink'
import { LIMITS, setSplit, splits } from './audio/split'
import ArrangerPane from './components/ArrangerPane.vue'
import AssistantPane from './components/AssistantPane.vue'
import ComposerPane from './components/ComposerPane.vue'
import ConsolePane from './components/ConsolePane.vue'
import KnobPop from './components/console/KnobPop.vue'
import InstrumentsPane from './components/InstrumentsPane.vue'
import Splitter from './components/Splitter.vue'
import TransportBar from './components/TransportBar.vue'

// A strip's faceplate is one double-click away: select the synth and show the synths.
function openSynth(s: number) {
  synths.selected = s
  view.main = 'synths'
}

// The Assistant (#387) reads the song as the engine prints it, and Apply loads
// a song through the engine like any text (ADR-0012). Its own window gets the
// same over a BroadcastChannel; only this window touches the engine.
const linkState = (): LinkState => ({
  song: song.text,
  focus: song.frags[song.cued]?.name ?? null,
  frags: song.frags.map((f) => f.name),
  running: status.running,
})
function applySong(text: string): boolean {
  if (!status.running) return false
  loadSong(text)
  return true
}
const appHost: AssistHost = {
  link: {
    get song() { return song.text },
    get focus() { return song.frags[song.cued]?.name ?? null },
    get frags() { return song.frags.map((f) => f.name) },
    get running() { return status.running },
    connected: true,
  },
  apply: async (text) => applySong(text),
}
let server: ReturnType<typeof serveWindow> | null = null
const goodbye = () => {
  server?.close()
  server = null
}
onMounted(() => {
  if (typeof BroadcastChannel === 'undefined') return
  server = serveWindow(new BroadcastChannel(CHANNEL), { state: linkState, apply: applySong, popped: (open) => (assistant.popped = open) })
  window.addEventListener('pagehide', goodbye)
})
watch(() => [song.text, song.cued, status.running], () => server?.push())
onBeforeUnmount(() => {
  window.removeEventListener('pagehide', goodbye)
  goodbye()
})
function popOut() {
  window.open(`${import.meta.env.BASE_URL}assistant.html`, CHANNEL, 'popup,width=560,height=860')
}
</script>

<template>
  <div
    class="layout" :class="{ mixer: view.main === 'mixer', composer: view.main === 'composer', assist: assistant.shown }"
    :style="{
      '--arranger': view.main === 'composer' && splits.arranger != null ? `${splits.arranger}px` : undefined,
      '--assistant': splits.assistant != null ? `${splits.assistant}px` : undefined,
    }"
  >
    <div class="transport">
      <TransportBar />
      <!-- Whatever stops the engine working (a dsp.wasm older than the page, say) gets a row of its own:
           the transport bar clips, and a silent fallback looks like a bug in the instrument. -->
      <p v-if="status.error" class="banner" role="alert">{{ status.error }}</p>
    </div>
    <!-- The synths stay mounted so the computer keyboard plays them from the mixer too. -->
    <InstrumentsPane v-show="view.main === 'synths'" class="main" />
    <ConsolePane v-if="view.main === 'mixer'" class="main" @open-synth="openSynth" />
    <ComposerPane v-if="view.main === 'composer'" class="main" />
    <!-- In the composer the arranger's height is dragged (#373). -->
    <Splitter
      v-if="view.main === 'composer'" class="split" between="rows" :size="splits.arranger" :min="LIMITS.arranger"
      label="Height of the arranger" @resize="(v) => setSplit('arranger', v)"
    />
    <!-- The bottom pane: the arranger, under every view (ADR-0015, ADR-0022). -->
    <div class="foot bottom">
      <ArrangerPane class="fill" />
    </div>
    <!-- The Assistant beside every view (#387); its width is dragged. -->
    <template v-if="assistant.shown">
      <Splitter
        class="assist-split" between="columns" :size="splits.assistant" :min="LIMITS.assistant"
        label="Width of the Assistant" @resize="(v) => setSplit('assistant', v)"
      />
      <section v-if="assistant.popped" class="pane assist-pane popped">
        <div class="pane-head"><span>Assistant</span></div>
        <p class="note">The Assistant is open in its own window.</p>
      </section>
      <AssistantPane v-else class="assist-pane" :host="appHost" popout @popout="popOut" />
    </template>
    <!-- One popover for every knob, in the synths and in the mixer. -->
    <KnobPop />
  </div>
</template>

<style scoped>
.layout {
  height: 100%;
  display: grid;
  gap: 8px;
  padding: 8px;
  grid-template-columns: minmax(0, 1fr);
  grid-template-rows: auto minmax(0, 1fr) minmax(220px, 36vh);
  grid-template-areas:
    'transport'
    'main'
    'arranger';
}
.layout.mixer { grid-template-rows: auto minmax(0, 1fr) 200px; }
.layout.composer {
  grid-template-rows: auto minmax(0, 1fr) 6px var(--arranger, minmax(180px, 30vh));
  grid-template-areas: 'transport' 'main' 'split' 'arranger';
  row-gap: 4px;
}
/* The Assistant (#387): a third column beside every view, under the transport. */
.layout.assist { grid-template-columns: minmax(0, 1fr) 6px var(--assistant, minmax(300px, 26%)); column-gap: 4px; }
.layout.assist .transport { grid-column: 1 / -1; }
.assist-split { grid-column: 2; grid-row: 2 / -1; }
.assist-pane { grid-column: 3; grid-row: 2 / -1; }
.popped .note { margin: 0; padding: 10px 12px; color: var(--muted); }
.split { grid-area: split; }
.transport { grid-area: transport; display: flex; flex-direction: column; gap: 6px; }
.banner {
  margin: 0; padding: 8px 14px; border-radius: 4px; border: 1px solid #e0654f; background: #4a1d17; color: #ffd9d0;
  font: 600 14px/1.4 var(--con-font-silk); letter-spacing: 0.03em;
}
.main { grid-area: main; }
.foot { grid-area: arranger; }
.bottom { display: flex; flex-direction: column; gap: 4px; min-height: 0; }
.bottom .fill { flex: 1; }
</style>
