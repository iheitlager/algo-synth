<script setup lang="ts">
// A name that can be renamed in place (#127): double-click (or `editing`) shows
// a field; Enter or leaving it commits, Escape cancels. An empty name is sent
// as '' and the caller puts the default back.
import { nextTick, ref, watch } from 'vue'
import { MAX_NAME } from '../audio/names'

const props = defineProps<{ value: string; label: string; editing?: boolean }>()
const emit = defineEmits<{ rename: [name: string]; 'update:editing': [on: boolean] }>()

const on = ref(false)
const draft = ref('')
const field = ref<HTMLInputElement>()
let done = false

async function start() {
  draft.value = props.value
  on.value = true
  done = false
  emit('update:editing', true)
  await nextTick()
  field.value?.focus()
  field.value?.select()
}
function finish(commit: boolean) {
  if (done) return
  done = true
  on.value = false
  emit('update:editing', false)
  if (commit && draft.value !== props.value) emit('rename', draft.value)
}
watch(
  () => props.editing,
  (e) => {
    if (e && !on.value) start()
  },
  { immediate: true },
)
</script>

<template>
  <input
    v-if="on" ref="field" v-model="draft" class="name-edit" :maxlength="MAX_NAME" :aria-label="`Rename ${label}`"
    @keydown.enter.prevent="finish(true)" @keydown.esc.prevent="finish(false)" @keydown.stop @blur="finish(true)" @click.stop @dblclick.stop
  />
  <span v-else class="name" :title="`${value}: double-click to rename`" @dblclick.stop="start">{{ value }}</span>
</template>

<style scoped>
.name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.name-edit {
  font: inherit; letter-spacing: inherit; text-transform: none; color: var(--con-paper, inherit); background: #0d0f13;
  border: 1px solid var(--c, #888); border-radius: 3px; padding: 1px 4px; width: 100%; min-width: 6em; box-sizing: border-box;
}
</style>
