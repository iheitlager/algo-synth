<script setup lang="ts">
// A deck's lane (#449), as DJ apps scroll a track: time runs from the bottom
// to the top, one pixel a step. Above the playhead is the level heard on each
// step, rising as the song plays; below it the bars to come arrive as bands
// of the arrangement's sections. Bar and 8-bar phrase lines run across. It
// only draws what the engine reported (decktrail.ts).
import { onBeforeUnmount, onMounted, ref } from 'vue'
import { decks, trails } from '../audio/decks'
import { PHRASE, STEPS_PER_BAR, aheadEntry, at, sectionColour, width } from '../audio/decktrail'

const props = defineProps<{ deck: number; sections: readonly { name: string }[]; arrange: readonly number[] }>()

// Bars of history above the playhead and bars ahead below it: 16 bars, 256 px.
const HISTORY = 10 * STEPS_PER_BAR
const AHEAD = 6 * STEPS_PER_BAR
const HEIGHT = HISTORY + AHEAD

const canvas = ref<HTMLCanvasElement | null>(null)
let frame = 0
let drawn = ''

function draw() {
  frame = requestAnimationFrame(draw)
  const c = canvas.value
  const d = decks.list[props.deck]
  const t = trails[props.deck]
  if (!c || !d || !t) return
  const dpr = window.devicePixelRatio || 1
  const w = c.clientWidth
  const key = [w, dpr, d.step, t.last, t.peaks[t.last % t.peaks.length], d.ahead?.from, d.ahead?.entries.join(), props.arrange.join(), props.sections.length].join()
  if (key === drawn) return
  drawn = key
  if (c.width !== Math.round(w * dpr)) c.width = Math.round(w * dpr)
  if (c.height !== HEIGHT * dpr) c.height = HEIGHT * dpr
  const g = c.getContext('2d')
  if (!g) return
  const css = getComputedStyle(c)
  const colour = (name: string) => css.getPropertyValue(name).trim()
  g.setTransform(dpr, 0, 0, dpr, 0, 0)
  g.fillStyle = colour('--panel')
  g.fillRect(0, 0, w, HEIGHT)

  // Row y shows step `cur + y - HISTORY`: the past above, the bars ahead below.
  const cur = d.step
  const entryAt = (k: number) => (k <= cur ? (at(t, k)?.entry ?? -1) : aheadEntry(d.ahead, Math.floor(k / STEPS_PER_BAR)))
  const labels: { y: number; name: string }[] = []
  let prev = -2
  for (let y = 0; y < HEIGHT; y++) {
    const k = cur + y - HISTORY
    if (k < 0) continue
    const e = entryAt(k)
    const s = e >= 0 ? props.arrange[e] : undefined
    if (s !== undefined) {
      g.globalAlpha = k > cur ? 0.32 : 0.16
      g.fillStyle = sectionColour(s)
      g.fillRect(0, y, w, 1)
      if (e !== prev) labels.push({ y, name: props.sections[s]?.name ?? '' })
    }
    prev = e
    g.globalAlpha = 1
    const level = k <= cur ? at(t, k) : null
    if (level && level.peak > 0) {
      const half = (width(level.peak) * (w - 8)) / 2
      g.fillStyle = colour('--accent')
      g.fillRect(w / 2 - half, y, 2 * half, 1)
    }
    if (k % STEPS_PER_BAR === 0) {
      g.fillStyle = colour(k % PHRASE === 0 ? '--muted' : '--line')
      g.fillRect(0, y, w, 1)
    }
  }
  g.font = '10px ui-monospace, Menlo, monospace'
  g.textBaseline = 'top'
  g.fillStyle = colour('--text')
  for (const l of labels) g.fillText(l.name, 4, l.y + 2)
  // The playhead, in the text's colour so it stands out from the levels.
  g.fillStyle = colour('--text')
  g.fillRect(0, HISTORY, w, 1)
}

onMounted(() => (frame = requestAnimationFrame(draw)))
onBeforeUnmount(() => cancelAnimationFrame(frame))
</script>

<template>
  <canvas ref="canvas" class="lane" :height="HEIGHT" aria-label="Level and arrangement, past above the playhead, to come below" />
</template>

<style scoped>
.lane { display: block; width: 100%; height: 256px; border: 1px solid var(--line); border-radius: 4px; background: var(--panel); }
</style>
