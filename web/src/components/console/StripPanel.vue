<script setup lang="ts">
// A strip's or group's presets (#152): pan, sends and insert slots, never its
// fader, mute, solo or routing (ADR-0014). Copy and paste are in the menu.
import { onBeforeUnmount, onMounted } from 'vue'
import PresetBar from '../PresetBar.vue'
import { closeStripPanel, stripPanel as p } from './stripPanel'

function onDown(e: PointerEvent) {
  if (!p.open) return
  if ((e.target as HTMLElement).closest?.('.strip-panel, .strip-tool')) return
  closeStripPanel()
}
function onKey(e: KeyboardEvent) {
  if (p.open && e.key === 'Escape') closeStripPanel()
}
onMounted(() => { addEventListener('pointerdown', onDown, true); addEventListener('keydown', onKey) })
onBeforeUnmount(() => { removeEventListener('pointerdown', onDown, true); removeEventListener('keydown', onKey) })
</script>

<template>
  <div v-if="p.open" class="strip-panel" role="dialog" :aria-label="`${p.title} presets`" :style="{ left: `${p.x}px`, top: `${p.y}px` }">
    <header><b>{{ p.title }}</b><span>Strip preset</span></header>
    <PresetBar :key="p.strip" compact label="Strip preset" :target="{ kind: 'strip', s: p.strip }" />
    <p>Pan, sends and inserts; the fader, mute, solo and routing stay.</p>
  </div>
</template>

<style scoped>
.strip-panel {
  position: fixed; z-index: 15; width: 300px; padding: 10px 12px 10px; background: #0f1217; border: 1px solid #3a424e;
  border-top: 3px solid var(--con-silk-dim); border-radius: 6px; box-shadow: 0 14px 34px #000d; font-family: var(--con-font-silk);
}
header { display: flex; justify-content: space-between; align-items: baseline; margin-bottom: 8px; }
header b { font-size: 16px; letter-spacing: 0.1em; text-transform: uppercase; color: var(--con-paper); }
header span { font-size: 12px; letter-spacing: 0.16em; text-transform: uppercase; color: var(--con-silk-dim); }
p { margin: 8px 0 0; font-size: 11px; color: var(--con-silk-dim); }
</style>
