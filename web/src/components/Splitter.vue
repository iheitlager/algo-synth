<script setup lang="ts">
// A divider between two panes (#373): dragged with a mouse, pen or finger, or
// moved with the arrow keys when focused; a double-click puts it back. It sizes
// the pane right after it in its container (the song text, the arranger),
// which stays at least `min` and at most `LIMITS.share` of the container.
import { ref } from 'vue'
import { clampSize, dragged, stepped } from '../audio/split'

const props = defineProps<{
  /** `columns` for a divider between two columns, `rows` between two rows. */
  between: 'columns' | 'rows'
  /** The pane's size in pixels; null while it is at the layout's default. */
  size: number | null
  min: number
  label: string
}>()
const emit = defineEmits<{ resize: [size: number | null] }>()

const handle = ref<HTMLElement | null>(null)
const across = () => props.between === 'columns'
/** The container's width or height, and the pane's size as laid out now. */
const room = () => {
  const box = handle.value?.parentElement
  return (across() ? box?.clientWidth : box?.clientHeight) ?? 0
}
const current = () => {
  if (props.size != null) return props.size
  const pane = handle.value?.nextElementSibling as HTMLElement | null
  return (across() ? pane?.offsetWidth : pane?.offsetHeight) ?? props.min
}
const set = (to: number) => emit('resize', clampSize(to, props.min, room()))

let start = 0
let from = 0
function down(e: PointerEvent) {
  start = current()
  from = across() ? e.clientX : e.clientY
  handle.value?.setPointerCapture(e.pointerId)
  e.preventDefault()
}
function move(e: PointerEvent) {
  if (!handle.value?.hasPointerCapture(e.pointerId)) return
  set(dragged(start, from, across() ? e.clientX : e.clientY))
}
function key(e: KeyboardEvent) {
  const to = stepped(current(), e.key)
  if (to == null) return
  set(to)
  e.preventDefault()
}
</script>

<template>
  <div
    ref="handle" class="splitter" :class="between" role="separator" tabindex="0"
    :aria-orientation="between === 'columns' ? 'vertical' : 'horizontal'" :aria-label="label"
    :aria-valuenow="size ?? undefined" :aria-valuemin="min" :title="`${label}: drag, or double-click to reset`"
    @pointerdown="down" @pointermove="move" @keydown="key" @dblclick="emit('resize', null)"
  />
</template>

<style scoped>
.splitter { position: relative; touch-action: none; border-radius: 3px; }
.splitter.columns { cursor: col-resize; width: 6px; }
.splitter.rows { cursor: row-resize; height: 6px; }
.splitter::after { content: ''; position: absolute; border-radius: 2px; background: var(--line); }
.splitter.columns::after { inset: 25% 2px; }
.splitter.rows::after { inset: 2px 35%; }
.splitter:hover::after, .splitter:focus-visible::after { background: var(--accent); }
.splitter:focus-visible { outline: none; }
</style>

