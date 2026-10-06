<script setup lang="ts">
// Wide-screen layout (spec 003): transport on top, the synths or the mixer
// console with the MIDI player across the bottom, or the composer on a screen
// of its own.
import { status, synths, view } from './audio/engine'
import ArrangerPane from './components/ArrangerPane.vue'
import ComposerPane from './components/ComposerPane.vue'
import ConsolePane from './components/ConsolePane.vue'
import KnobPop from './components/console/KnobPop.vue'
import InstrumentsPane from './components/InstrumentsPane.vue'
import PlayerPane from './components/PlayerPane.vue'
import SoundPane from './components/SoundPane.vue'
import TransportBar from './components/TransportBar.vue'

// A strip's faceplate is one double-click away: select the synth and show the synths.
function openSynth(s: number) {
  synths.selected = s
  view.main = 'synths'
}
</script>

<template>
  <div class="layout" :class="{ mixer: view.main === 'mixer', composer: view.main === 'composer' || view.main === 'sound' }">
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
    <SoundPane v-if="view.main === 'sound'" class="main" />
    <!-- The bottom pane: the arranger (also under the composer) or the MIDI file player. -->
    <div class="player bottom">
      <nav v-if="view.main !== 'composer'" class="tabs" aria-label="Bottom pane">
        <button :aria-pressed="view.bottom === 'arranger'" @click="view.bottom = 'arranger'">Arranger</button>
        <button :aria-pressed="view.bottom === 'player'" @click="view.bottom = 'player'">MIDI player</button>
      </nav>
      <ArrangerPane v-if="view.main === 'composer' || view.bottom === 'arranger'" class="fill" />
      <PlayerPane v-else class="fill" />
    </div>
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
    'player';
}
.layout.mixer { grid-template-rows: auto minmax(0, 1fr) 200px; }
.layout.composer { grid-template-rows: auto minmax(0, 1fr) minmax(180px, 30vh); }
.transport { grid-area: transport; display: flex; flex-direction: column; gap: 6px; }
.banner {
  margin: 0; padding: 8px 14px; border-radius: 4px; border: 1px solid #e0654f; background: #4a1d17; color: #ffd9d0;
  font: 600 14px/1.4 var(--con-font-silk); letter-spacing: 0.03em;
}
.main { grid-area: main; }
.player { grid-area: player; }
.bottom { display: flex; flex-direction: column; gap: 4px; min-height: 0; }
.bottom .fill { flex: 1; }
.tabs { display: flex; gap: 2px; }
.tabs button { padding: 2px 10px; font-size: 11px; letter-spacing: 0.06em; text-transform: uppercase; }
.tabs button[aria-pressed='true'] { border-color: var(--accent); color: var(--accent); }
</style>
