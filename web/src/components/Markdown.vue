<script setup lang="ts">
// A model's markdown, drawn (#459): blocks and spans from audio/markdown.ts,
// made into elements here, never into HTML from the text. Code blocks that
// are a song (or say nothing) get the song editor's colours (#203).
import { computed, h, type VNode } from 'vue'
import { lexer, paint } from '../audio/lex'
import { parseMarkdown, type Block, type Inline } from '../audio/markdown'

const props = defineProps<{ text: string }>()
const blocks = computed(() => parseMarkdown(props.text))

function spans(kids: Inline[]): (VNode | string)[] {
  return kids.map((k) => {
    if (k.t === 'b' || k.t === 'i') return h(k.t === 'b' ? 'strong' : 'em', spans(k.kids))
    return k.t === 'code' ? h('code', k.text) : k.text
  })
}

function code(b: Extract<Block, { t: 'code' }>): VNode {
  const lex = b.lang === '' || b.lang === 'song' ? lexer.value : null
  const lines = lex ? paint(b.text, lex(b.text)) : b.text.split('\n').map((l) => [{ text: l, cls: '' }])
  return h('pre', { class: 'block' }, lines.map((line) => h('div', line.length ? line.map((p) => (p.cls ? h('span', { class: p.cls }, p.text) : p.text)) : ' ')))
}

function block(b: Block): VNode {
  switch (b.t) {
    case 'p': return h('p', spans(b.kids))
    case 'h': return h(`h${Math.min(b.level + 2, 6)}`, spans(b.kids))
    case 'ul': return h('ul', b.items.map((i) => h('li', spans(i))))
    case 'ol': return h('ol', { start: b.start }, b.items.map((i) => h('li', spans(i))))
    case 'code': return code(b)
  }
}

const Blocks = () => blocks.value.map(block)
</script>

<template>
  <div class="md"><Blocks /></div>
</template>

<style scoped>
.md { display: flex; flex-direction: column; gap: 6px; white-space: normal; line-height: 1.45; }
.md :deep(p), .md :deep(ul), .md :deep(ol), .md :deep(pre) { margin: 0; }
.md :deep(ul), .md :deep(ol) { padding-left: 18px; display: flex; flex-direction: column; gap: 2px; }
.md :deep(h3), .md :deep(h4), .md :deep(h5), .md :deep(h6) { margin: 2px 0 0; font-size: 12px; font-weight: 600; }
.md :deep(code) { font-family: var(--font-mono); font-size: 11px; color: var(--accent); background: var(--bg); border-radius: 3px; padding: 0 3px; }
.md :deep(strong) { color: var(--text); font-weight: 600; }
.md :deep(.block) { font: 11px/1.45 var(--font-mono); background: var(--bg); border: 1px solid var(--line); border-radius: 3px; padding: 6px 8px; overflow-x: auto; white-space: pre; color: var(--text); }
/* The song lexer's classes, as the editor colours them (song::lex::Class). */
.md :deep(.kw) { color: var(--accent); }
.md :deep(.name) { color: #9cc4ff; }
.md :deep(.num) { color: var(--score); }
.md :deep(.note) { color: #8fd694; }
.md :deep(.pad) { font-weight: 500; }
.md :deep(.step) { color: var(--mono); }
.md :deep(.rest) { color: color-mix(in srgb, var(--muted) 70%, transparent); }
.md :deep(.call) { color: #cf9cf2; }
.md :deep(.param) { color: #6fd3d3; }
.md :deep(.punct) { color: var(--muted); }
.md :deep(.comment) { color: #6b7280; font-style: italic; }
</style>
