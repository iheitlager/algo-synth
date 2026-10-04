<script setup lang="ts">
// The playhead over all lanes (#137): the only part of the player that follows
// the position, moved with a transform so the page is not laid out again.
import { computed } from 'vue'
import { player } from '../audio/engine'

const x = computed(() => `translateX(${(player.position / (player.length || 1)) * 100}%)`)
</script>

<template>
  <div class="playhead" aria-hidden="true"><i :style="{ transform: x }" /></div>
</template>

<style scoped>
.playhead { position: absolute; top: 0; bottom: 0; left: 180px; right: 0; overflow: hidden; pointer-events: none; }
i { position: absolute; inset: 0; border-left: 1px solid var(--accent); will-change: transform; }
</style>
