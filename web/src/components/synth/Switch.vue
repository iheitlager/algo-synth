<script setup lang="ts">
// An LED toggle button (spec 003 Req 9): a lit dot and a silkscreen label. It
// holds no value of its own: it shows `modelValue` and emits the change.
defineProps<{ modelValue: boolean; label: string; name?: string; color?: string; disabled?: boolean }>()
defineEmits<{ 'update:modelValue': [v: boolean] }>()
</script>

<template>
  <button
    type="button" class="sw" role="switch" :aria-checked="modelValue" :aria-label="name ?? label" :disabled="disabled"
    :style="color ? { '--led': color } : undefined" @click="$emit('update:modelValue', !modelValue)"
  >
    <i class="led" aria-hidden="true" />
    <span class="lab">{{ label }}</span>
  </button>
</template>

<style scoped>
.sw {
  display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 40px; padding: 5px 7px 4px; cursor: pointer;
  background: linear-gradient(#2e343d, #232830); border: 1px solid #39414d; border-radius: 3px; box-shadow: 0 2px 0 #0a0c0f;
  font-family: var(--con-font-silk);
}
.sw:disabled { opacity: 0.5; cursor: default; }
.sw:active:not(:disabled) { transform: translateY(1px); box-shadow: 0 1px 0 #0a0c0f; }
.sw:focus-visible { outline: 2px solid var(--con-paper); outline-offset: 2px; }
.led { width: 9px; height: 9px; border-radius: 50%; background: #1a1d22; border: 1px solid #07080a; box-shadow: 0 1px 0 #ffffff14; }
.sw[aria-checked='true'] .led { background: var(--led, var(--c, #f0b03a)); box-shadow: 0 0 9px var(--led, var(--c, #f0b03a)), 0 0 2px #fff8 inset; }
.lab { font-size: 11px; font-weight: 500; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); line-height: 1; white-space: nowrap; }
.sw[aria-checked='true'] .lab { color: var(--con-paper); }
</style>
