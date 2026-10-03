<script setup lang="ts">
// A stepped selector (spec 003 Req 9): a row of buttons, one lit, for waveforms
// (drawn as icons), modes and discrete values. It holds no value of its own:
// it shows the option nearest `modelValue` and emits the value of the one chosen.
import { computed } from 'vue'
import { nearestStep, stepIndex, wavePath } from '../../audio/faceplate'

const props = defineProps<{
  modelValue: number
  label: string
  options: readonly (readonly [string, number])[]
  name?: string
  disabled?: boolean
}>()
const emit = defineEmits<{ 'update:modelValue': [v: number] }>()

const current = computed(() => nearestStep(props.options, props.modelValue))
const icon = (name: string) => wavePath(name, 26, 16)
const choose = (i: number) => {
  const o = props.options[i]
  if (o) emit('update:modelValue', o[1])
}
function key(e: KeyboardEvent) {
  const next = stepIndex(current.value, props.options.length, e.key)
  if (next === null) return
  e.preventDefault()
  choose(next)
  // Follow the focus to the newly chosen button.
  const group = (e.currentTarget as HTMLElement).parentElement
  requestAnimationFrame(() => group?.querySelectorAll<HTMLElement>('[role=radio]')[next]?.focus())
}
</script>

<template>
  <div class="sel">
    <div v-if="label" class="lab">{{ label }}</div>
    <div class="row" role="radiogroup" :aria-label="name ?? label">
      <button
        v-for="([n, v], i) in options" :key="v" type="button" role="radio" class="opt" :aria-checked="i === current"
        :tabindex="i === current ? 0 : -1" :title="n" :disabled="disabled" @click="choose(i)" @keydown="key"
      >
        <svg v-if="icon(n)" width="26" height="16" viewBox="0 0 26 16" aria-hidden="true">
          <path :d="icon(n) ?? ''" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" stroke-linecap="round" />
        </svg>
        <span v-else>{{ n }}</span>
        <span v-if="icon(n)" class="sr">{{ n }}</span>
      </button>
    </div>
  </div>
</template>

<style scoped>
.sel { display: flex; flex-direction: column; align-items: center; gap: 4px; font-family: var(--con-font-silk); }
.lab { font-size: 11px; font-weight: 500; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); line-height: 1; white-space: nowrap; }
.row { display: flex; gap: 0; border: 1px solid #39414d; border-radius: 3px; overflow: hidden; box-shadow: 0 2px 0 #0a0c0f; }
.opt {
  min-width: 30px; height: 26px; padding: 0 6px; display: grid; place-items: center; cursor: pointer; border: 0; border-right: 1px solid #0a0c0f;
  background: linear-gradient(#2e343d, #232830); color: var(--con-silk-dim); font: 600 12px var(--con-font-silk); letter-spacing: 0.08em;
  position: relative;
}
.opt:last-child { border-right: 0; }
.opt:hover:not(:disabled) { color: var(--con-paper); }
.opt:disabled { opacity: 0.5; cursor: default; }
.opt:focus-visible { outline: 2px solid var(--con-paper); outline-offset: -2px; }
.opt[aria-checked='true'] { color: #fff; background: linear-gradient(#171a1f, #1d2126); box-shadow: 0 2px 5px #000a inset; }
.opt[aria-checked='true']::after {
  content: ''; position: absolute; left: 20%; right: 20%; bottom: 2px; height: 2px; border-radius: 1px; background: var(--c, #f0b03a);
  box-shadow: 0 0 6px var(--c, #f0b03a);
}
.opt svg { display: block; }
.sr { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
</style>
