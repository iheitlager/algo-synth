<script setup lang="ts">
// Wide-screen layout (spec 003): transport on top, the synths or the mixer
// console, the MIDI player across the bottom.
import { nextTick } from 'vue'
import { synths, view } from './audio/engine'
import ConsolePane from './components/ConsolePane.vue'
import KnobPop from './components/console/KnobPop.vue'
import InstrumentsPane from './components/InstrumentsPane.vue'
import PlayerPane from './components/PlayerPane.vue'
import TransportBar from './components/TransportBar.vue'

// A strip's panel is one double-click away: show the synths and bring its card in view.
async function openSynth(s: number) {
  synths.selected = s
  view.main = 'synths'
  await nextTick()
  document.getElementById(`synth-${s}`)?.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
}
</script>

<template>
  <div class="layout" :class="{ mixer: view.main === 'mixer' }">
    <TransportBar class="transport" />
    <!-- The synths stay mounted so the computer keyboard plays them from the mixer too. -->
    <InstrumentsPane v-show="view.main === 'synths'" class="main" />
    <ConsolePane v-if="view.main === 'mixer'" class="main" @open-synth="openSynth" />
    <PlayerPane class="player" />
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
.transport { grid-area: transport; }
.main { grid-area: main; }
.player { grid-area: player; }
</style>
