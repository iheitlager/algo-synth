<script setup lang="ts">
// The Assistant's own window (#387): the pane, with the main window as its
// host. The song comes over the channel; Apply goes back over it, and the
// main window loads it into the engine.
import { onBeforeUnmount, reactive } from 'vue'
import { CHANNEL, connectMain, type AssistHost } from '../audio/assistlink'
import AssistantPane from './AssistantPane.vue'

const link = reactive({ song: '', focus: null as string | null, tracks: [] as string[], running: false, connected: false })
const main = connectMain(new BroadcastChannel(CHANNEL), (s) => {
  if (s) Object.assign(link, s, { connected: true })
  else link.connected = false
})
const host: AssistHost = { link, apply: (song) => main.apply(song) }
const goodbye = () => main.close()
window.addEventListener('pagehide', goodbye)
onBeforeUnmount(() => {
  window.removeEventListener('pagehide', goodbye)
  goodbye()
})
</script>

<template>
  <div class="window">
    <AssistantPane class="fill" :host="host" />
  </div>
</template>

<style scoped>
.window { height: 100%; padding: 8px; display: flex; }
.fill { flex: 1; min-width: 0; }
</style>
