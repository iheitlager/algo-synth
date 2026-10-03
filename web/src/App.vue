<script setup lang="ts">
// Wide-screen layout (spec 003): transport on top, the returns, the synths, the MIDI
// player across the bottom.
import TransportBar from './components/TransportBar.vue'
import InstrumentsPane from './components/InstrumentsPane.vue'
import ReturnsPane from './components/ReturnsPane.vue'
import PlayerPane from './components/PlayerPane.vue'
</script>

<template>
  <div class="layout">
    <TransportBar class="transport" />
    <ReturnsPane class="returns" />
    <InstrumentsPane class="instruments" />
    <PlayerPane class="player" />
  </div>
</template>

<style scoped>
.layout {
  height: 100%;
  display: grid;
  gap: 8px;
  padding: 8px;
  grid-template-columns: minmax(0, 1fr);
  grid-template-rows: auto auto minmax(0, 1fr) minmax(220px, 36vh);
  grid-template-areas:
    'transport'
    'returns'
    'instruments'
    'player';
}
.transport { grid-area: transport; }
.returns { grid-area: returns; }
.instruments { grid-area: instruments; }
.player { grid-area: player; }
</style>
