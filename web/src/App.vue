<script setup lang="ts">
// Wide-screen layout (spec 003): transport on top, the algo pane left,
// instruments right, the arrangement across the bottom.
import TransportBar from './components/TransportBar.vue'
import AlgoPane from './components/AlgoPane.vue'
import InstrumentsPane from './components/InstrumentsPane.vue'
import ArrangePane from './components/ArrangePane.vue'
import { demoSong } from './model/song'
</script>

<template>
  <div class="layout">
    <TransportBar class="transport" :bpm="demoSong.bpm" />
    <AlgoPane class="algo" :loops="demoSong.loops" :tracks="demoSong.tracks" />
    <InstrumentsPane class="instruments" />
    <ArrangePane class="arrange" :song="demoSong" />
  </div>
</template>

<style scoped>
.layout {
  height: 100%;
  display: grid;
  gap: 8px;
  padding: 8px;
  grid-template-columns: minmax(340px, 1fr) minmax(0, 2fr);
  grid-template-rows: auto minmax(0, 1fr) minmax(220px, 36vh);
  grid-template-areas:
    'transport transport'
    'algo instruments'
    'arrange arrange';
}
.transport { grid-area: transport; }
.algo { grid-area: algo; }
.instruments { grid-area: instruments; }
.arrange { grid-area: arrange; }
</style>
