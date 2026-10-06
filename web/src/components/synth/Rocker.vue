<script setup lang="ts">
// A rocker switch as the Minimoog has them (#308): a coloured paddle whose
// top half stands out when on and the bottom half when off. Like `Switch.vue`
// it holds no value of its own: it shows `modelValue` and emits the change.
import type { RockerColour } from '../../audio/models'

defineProps<{ modelValue: boolean; label: string; colour: RockerColour; name?: string; disabled?: boolean }>()
defineEmits<{ 'update:modelValue': [v: boolean] }>()
</script>

<template>
  <button
    type="button" class="rk" :class="colour" role="switch" :aria-checked="modelValue" :aria-label="name ?? label"
    :disabled="disabled" @click="$emit('update:modelValue', !modelValue)"
  >
    <span class="lab">{{ label }}</span>
    <i class="paddle" aria-hidden="true" />
  </button>
</template>

<style scoped>
.rk {
  display: flex; flex-direction: column; align-items: center; gap: 5px; min-width: 34px; padding: 0; cursor: pointer;
  background: none; border: 0; font-family: var(--con-font-silk);
}
.rk:disabled { opacity: 0.5; cursor: default; }
.rk:focus-visible { outline: 2px solid var(--con-paper); outline-offset: 3px; border-radius: 2px; }
.orange { --rk: #e2702b; }
.blue { --rk: #2d6cb3; }
.white { --rk: #ebe6d8; }
.black { --rk: #2a2827; }
/* The raised half catches the light; the half pressed in sits in shadow. */
.paddle {
  width: 16px; height: 30px; border-radius: 2px; border: 1px solid #000a;
  box-shadow: 0 0 0 2px #0008, 0 2px 3px #000a;
  background: linear-gradient(
    color-mix(in srgb, var(--rk) 55%, black) 0%, color-mix(in srgb, var(--rk) 70%, black) 48%,
    color-mix(in srgb, var(--rk) 85%, white) 52%, var(--rk) 100%
  );
}
.rk[aria-checked='true'] .paddle {
  background: linear-gradient(
    var(--rk) 0%, color-mix(in srgb, var(--rk) 85%, white) 48%,
    color-mix(in srgb, var(--rk) 70%, black) 52%, color-mix(in srgb, var(--rk) 55%, black) 100%
  );
}
.lab { font-size: 10px; font-weight: 500; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); line-height: 1.1; text-align: center; max-width: 76px; }
.rk[aria-checked='true'] .lab { color: var(--con-paper); }
</style>
