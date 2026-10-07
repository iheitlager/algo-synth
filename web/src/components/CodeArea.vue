<script setup lang="ts">
// A text editor with line numbers and highlighting, shared by the song text
// (#203) and the Modular code (#329). Typing goes into a plain textarea;
// under it, the same text in colours from the engine's lexer (audio/lex.ts),
// which the caller picks. The parse error, if any, marks its line and column.
import { computed, ref } from 'vue'
import { paint, type Lexer } from '../audio/lex'

const text = defineModel<string>({ required: true })
const props = withDefaults(
  defineProps<{
    lexer: Lexer | null
    label: string
    disabled?: boolean
    error?: { line: number; col: number; msg: string } | null
    /** What Tab inserts. */
    indent?: string
  }>(),
  { disabled: false, error: null, indent: '  ' },
)

const lines = computed(() => paint(text.value, props.lexer?.(text.value) ?? []))

const area = ref<HTMLTextAreaElement>()
const code = ref<HTMLElement>()
const gutter = ref<HTMLElement>()
function onScroll() {
  const a = area.value
  if (!a) return
  if (code.value) {
    code.value.scrollTop = a.scrollTop
    code.value.scrollLeft = a.scrollLeft
  }
  if (gutter.value) gutter.value.scrollTop = a.scrollTop
}

// Tab indents instead of leaving the text.
function onKey(e: KeyboardEvent) {
  const a = area.value
  if (e.key !== 'Tab' || e.shiftKey || !a) return
  e.preventDefault()
  a.setRangeText(props.indent, a.selectionStart, a.selectionEnd, 'end')
  text.value = a.value
}
</script>

<template>
  <div class="editor" :class="{ disabled }">
    <div ref="gutter" class="gutter" aria-hidden="true">
      <div v-for="(_, i) in lines" :key="i" class="lno" :class="{ err: error?.line === i + 1 }">{{ i + 1 }}</div>
    </div>
    <div class="code-wrap">
      <pre ref="code" class="code" aria-hidden="true"><div
        v-for="(line, i) in lines" :key="i" class="ln" :class="{ err: error?.line === i + 1 }"
      ><span v-for="(p, k) in line" :key="k" :class="p.cls">{{ p.text }}</span><span
        v-if="error?.line === i + 1" class="mark" :style="{ left: `${error.col - 1}ch` }" :title="error.msg"
      /></div></pre>
      <textarea
        ref="area" v-model="text" wrap="off" spellcheck="false" autocapitalize="off" autocomplete="off"
        :disabled="disabled" :aria-label="label" @scroll="onScroll" @keydown="onKey"
      />
    </div>
  </div>
</template>

<style scoped>
.editor {
  --lh: 19px;
  flex: 1; min-height: 160px; display: flex; overflow: hidden;
  font-family: var(--font-mono); font-size: 13px; line-height: var(--lh);
  background: var(--bg); border: 1px solid var(--line); border-radius: 4px;
}
.editor:focus-within { border-color: color-mix(in srgb, var(--accent) 60%, var(--line)); }
.editor.disabled { opacity: 0.6; }
.gutter {
  flex: none; overflow: hidden; padding: 8px 8px 8px 10px; text-align: right; user-select: none;
  color: color-mix(in srgb, var(--muted) 60%, transparent); border-right: 1px solid var(--line); min-width: 3.2em;
}
.lno { height: var(--lh); }
.lno.err { color: var(--accent); }
.code-wrap { position: relative; flex: 1; min-width: 0; }
.code, textarea {
  position: absolute; inset: 0; margin: 0; padding: 8px; border: 0;
  font: inherit; line-height: inherit; white-space: pre; tab-size: 2; letter-spacing: normal;
}
.code { overflow: hidden; color: var(--text); pointer-events: none; }
.ln { position: relative; min-height: var(--lh); }
.ln.err { background: color-mix(in srgb, var(--accent) 10%, transparent); }
.mark {
  position: absolute; bottom: 1px; width: 1ch; height: 2px; background: var(--accent);
}
textarea {
  width: 100%; height: 100%; resize: none; overflow: auto; outline: none;
  background: transparent; color: transparent; caret-color: var(--text);
}
textarea::selection { background: color-mix(in srgb, var(--accent) 30%, transparent); color: transparent; }

/* The engine's classes (song::lex::Class). */
.kw { color: var(--accent); }
.name { color: #9cc4ff; }
.num { color: var(--score); }
.note { color: #8fd694; }
.pad { color: var(--text); font-weight: 500; }
.step { color: var(--mono); }
.rest { color: color-mix(in srgb, var(--muted) 70%, transparent); }
.call { color: #cf9cf2; }
.param { color: #6fd3d3; }
.punct { color: var(--muted); }
.comment { color: #6b7280; font-style: italic; }
</style>
