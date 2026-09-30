<script setup lang="ts">
// Classic bars view: one row per track, clips placed by bar (ADR-0005).
import type { Song } from '../model/song'

const props = defineProps<{ song: Song }>()
const bars = Array.from({ length: props.song.bars }, (_, i) => i + 1)
const clipsOf = (id: string) => props.song.clips.filter((c) => c.track === id)
</script>

<template>
  <section class="pane">
    <div class="pane-head">
      <span>Arrangement</span>
      <span class="legend">
        <span class="dot hand" />hand <span class="dot algo" />algo <span class="dot score" />score
      </span>
    </div>
    <div class="grid" :style="{ '--bars': song.bars }">
      <div class="corner" />
      <div class="ruler">
        <span v-for="b in bars" :key="b" :class="{ four: b % 4 === 1 }">{{ b % 4 === 1 ? b : '' }}</span>
      </div>
      <template v-for="t in song.tracks" :key="t.id">
        <div class="track">
          <b>{{ t.name }}</b>
          <span class="src">{{ t.source }}</span>
          <span class="fx">{{ t.inserts.join(' · ') || '—' }}</span>
        </div>
        <div class="lane">
          <div
            v-for="c in clipsOf(t.id)" :key="c.bar" class="clip" :class="c.origin"
            :style="{ gridColumn: `${c.bar} / span ${c.bars}` }"
          >{{ c.pattern }}</div>
        </div>
      </template>
    </div>
  </section>
</template>

<style scoped>
.grid { display: grid; grid-template-columns: 180px minmax(0, 1fr); }
.ruler, .lane { display: grid; grid-template-columns: repeat(var(--bars), minmax(28px, 1fr)); }
.ruler { position: sticky; top: 33px; background: var(--panel); z-index: 1; }
.ruler span { font-size: 10px; color: var(--muted); padding: 2px 4px; border-left: 1px solid var(--line); }
.ruler .four { border-left-color: var(--muted); }
.corner { position: sticky; top: 33px; background: var(--panel); z-index: 1; }
.track { display: grid; grid-template-columns: 1fr auto; padding: 6px 10px; border-top: 1px solid var(--line); gap: 2px 6px; }
.src { color: var(--muted); font-size: 11px; }
.fx { grid-column: 1 / -1; color: var(--muted); font-size: 10px; }
.lane {
  border-top: 1px solid var(--line); padding: 4px 0; min-height: 44px;
  background-image: linear-gradient(to right, var(--line) 1px, transparent 1px);
  background-size: calc(100% / var(--bars)) 100%;
}
.clip { grid-row: 1; margin: 0 1px; border-radius: 3px; padding: 4px 6px; font-size: 11px; color: #111; overflow: hidden; white-space: nowrap; }
.clip.hand { background: var(--hand); }
.clip.algo { background: var(--algo); }
.clip.score { background: var(--score); }
.legend { display: flex; gap: 6px; align-items: center; text-transform: none; letter-spacing: 0; }
.dot { width: 8px; height: 8px; border-radius: 2px; display: inline-block; }
.dot.hand { background: var(--hand); } .dot.algo { background: var(--algo); } .dot.score { background: var(--score); }
</style>
