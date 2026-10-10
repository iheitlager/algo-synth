<script setup lang="ts">
// Decks (ADR-0029, epic #391): deck A is the song the app edits, decks B–D
// each play a loaded song from an engine in a Web Worker. Every control is a
// message; the deck mixer is Rust.
import { song, status } from '../audio/engine'
import { decks, loadDeck, playDeck, setCrossfade, setDeck, setSync, stopDeck, type Start } from '../audio/decks'
import { DeckSide } from '../audio/params'
import DeckLane from './DeckLane.vue'

const LETTERS = ['A', 'B', 'C', 'D']
// Where Play starts a deck on the master's clock (ADR-0029).
const STARTS: { label: string; value: Start }[] = [
  { label: 'Next bar', value: 'bar' },
  { label: 'Next phrase (8 bars)', value: 'phrase' },
  { label: 'Now', value: 'now' },
]
const SIDES = [
  { label: 'Thru', value: DeckSide.Thru },
  { label: 'Left', value: DeckSide.Left },
  { label: 'Right', value: DeckSide.Right },
]

async function onFile(deck: number, e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  if (file) await loadDeck(deck, file)
}
// Bar and step of a worker deck's clock (16 steps a bar, as the transport shows).
const where = (step: number) => `bar ${Math.floor(step / 16) + 1} · step ${(step % 16) + 1}`
// How far the last sync found the deck off deck A's bar: within a sample or two it is locked.
const syncText = (ms: number) => (Math.abs(ms) < 0.05 ? 'locked' : `pulled ${Math.abs(ms).toFixed(1)} ms ${ms > 0 ? 'back' : 'forward'}`)
const db = (peak: number) => (peak > 1e-5 ? `${(20 * Math.log10(peak)).toFixed(0)} dB` : '−∞')
</script>

<template>
  <section class="pane decks" aria-label="Decks">
    <div class="pane-head">
      <span>Decks</span>
      <span>{{ decks.bpm ? `${decks.bpm.toFixed(1)} BPM, deck A leads` : '' }}</span>
    </div>
    <p v-if="!decks.isolated" class="notice" role="alert">
      This page is not cross-origin isolated, so decks B–D can't run. Serve it with the
      Cross-Origin-Opener-Policy and Cross-Origin-Embedder-Policy headers (<code>make serve</code>, <code>make dev</code>).
    </p>
    <p v-else-if="!status.running" class="notice">Power on to load songs into decks B–D.</p>
    <div class="row">
      <div v-for="(d, i) in decks.list" :key="i" class="deck" :class="{ on: i === 0 || d.playing }">
        <div class="head">
          <b class="letter">{{ LETTERS[i] }}</b>
          <span class="name" :title="d.name">{{ d.name || 'empty' }}</span>
        </div>
        <DeckLane :deck="i" :scenes="i === 0 ? song.scenes : d.scenes" :arrange="i === 0 ? song.arrange : d.arrange" />
        <template v-if="i > 0">
          <label class="file" :class="{ off: !decks.isolated || !status.running }">
            <input type="file" accept=".song" :disabled="!decks.isolated || !status.running" @change="onFile(i, $event)" />Load song…
          </label>
          <label>
            Start
            <select v-model="d.start" :disabled="!d.loaded">
              <option v-for="s in STARTS" :key="s.value" :value="s.value">{{ s.label }}</option>
            </select>
          </label>
          <label class="check" title="Pull this deck's bar lines onto deck A's at every bar">
            <input type="checkbox" :checked="d.sync" @change="setSync(i, ($event.target as HTMLInputElement).checked)" />
            Sync to deck A<template v-if="d.sync && d.syncMs !== null && d.playing"> · {{ syncText(d.syncMs) }}</template>
          </label>
          <div class="buttons">
            <button :disabled="!d.loaded || d.cued" :class="{ on: d.playing || d.cued }" @click="playDeck(i)">{{ d.cued ? 'Cued…' : '▶ Play' }}</button>
            <button :disabled="!d.loaded" @click="stopDeck(i)">■ Stop</button>
          </div>
          <span class="muted">{{ d.loaded ? where(d.step) : '' }}</span>
          <span v-if="d.error" class="error">{{ d.error }}</span>
        </template>
        <span v-else class="muted">the song in the composer</span>
        <label class="level">
          Level
          <input type="range" min="0" max="1" step="0.01" :value="d.level" @input="setDeck(i, 'level', +($event.target as HTMLInputElement).value)" />
        </label>
        <label>
          Crossfader
          <select :value="d.side" @change="setDeck(i, 'side', +($event.target as HTMLSelectElement).value)">
            <option v-for="s in SIDES" :key="s.value" :value="s.value">{{ s.label }}</option>
          </select>
        </label>
        <span class="muted">peak {{ db(d.peak) }}<template v-if="i > 0"> · dropped {{ d.dropped }}</template></span>
      </div>
    </div>
    <label class="xfade">
      <span>Left</span>
      <input type="range" min="0" max="1" step="0.01" :value="decks.crossfade" aria-label="Crossfader" @input="setCrossfade(+($event.target as HTMLInputElement).value)" />
      <span>Right</span>
    </label>
  </section>
</template>

<style scoped>
.notice { margin: 12px; color: var(--muted); }
.row { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; padding: 12px; }
.deck { display: flex; flex-direction: column; gap: 8px; padding: 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); min-width: 0; }
.deck.on { border-color: var(--accent); }
.head { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
.letter { color: var(--accent); font-size: 20px; }
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.buttons { display: flex; gap: 6px; }
.muted { color: var(--muted); font-size: 12px; }
.error { color: var(--mono); font-size: 12px; }
.level, .deck label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
.file { border: 1px solid var(--line); border-radius: 4px; padding: 4px 10px; background: var(--panel); cursor: pointer; text-align: center; }
.file:hover { border-color: var(--accent); }
.file.off { opacity: 0.5; cursor: default; }
.file input { display: none; }
.deck label.check { flex-direction: row; align-items: center; gap: 6px; }
.xfade { display: flex; align-items: center; gap: 12px; padding: 0 12px 12px; color: var(--muted); font-size: 12px; }
.xfade input { flex: 1; }
.on { border-color: var(--accent); color: var(--accent); }
@media (max-width: 800px) { .row { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
