<script setup lang="ts">
// A preset picker for one target (ADR-0014): factory presets when the target
// has them, the user's from the library, a dot when the target has changed
// since a user preset was loaded, and Save, Save as, Rename, Delete, Export
// and Import. Values go out through `applyPreset`; nothing is decided here.
import { computed, reactive, ref } from 'vue'
import { applyPreset, capturePreset, presetModified, status } from '../audio/engine'
import { clipboard, deletePreset, exportLibrary, importLibrary, library, presetsOf, renamePreset, savePreset } from '../audio/library'
import type { Target } from '../audio/presets'
import EditableName from './EditableName.vue'

const props = defineProps<{
  target: Target
  /** Only presets for this model or effect type. */
  of?: { model?: string; type?: string }
  /** Factory presets as [label, value]; choosing one emits `factory`. */
  factory?: [string, number][]
  label?: string
  /** Picker and menu only: Save and Save as move into the menu, for a narrow rack module. */
  compact?: boolean
}>()
const emit = defineEmits<{ factory: [value: number] }>()

const key = computed(() => JSON.stringify(props.target))
const users = computed(() => presetsOf(props.target.kind, props.of ?? {}))
// The same kind for other models or types: loading one switches the target over.
const others = computed(() => presetsOf(props.target.kind).filter((p) => !users.value.includes(p)))
const all = computed(() => [...users.value, ...others.value])
const current = computed(() => chosen[key.value])
const user = computed(() => (current.value?.startsWith('u:') ? all.value.find((p) => `u:${p.id}` === current.value) : undefined))
const value = computed(() => (current.value?.startsWith('f:') || user.value ? current.value : ''))
const dirty = computed(() => !!user.value && presetModified(user.value, props.target))

const naming = ref<'' | 'save' | 'rename'>('')
const menu = ref(false)

function choose(e: Event) {
  const select = e.target as HTMLSelectElement
  const v = select.value
  chosen[key.value] = v
  if (v.startsWith('f:')) emit('factory', Number(v.slice(2)))
  else if (v.startsWith('u:')) {
    const p = all.value.find((q) => q.id === v.slice(2))
    if (p) applyPreset(p, props.target)
  }
  // Hand the keys back to the computer keyboard.
  select.blur()
}
function save() {
  if (!user.value) return
  savePreset(capturePreset(user.value.name, props.target, user.value.id))
}
function saveAs(name: string) {
  naming.value = ''
  if (!name.trim()) return
  const p = capturePreset(name, props.target)
  savePreset(p)
  const stored = presetsOf(props.target.kind, props.of ?? {}).find((q) => q.name === p.name)
  if (stored) chosen[key.value] = `u:${stored.id}`
}
// Copy and paste: the clipboard holds one of each kind, so a paste can only be
// of the kind it is pasted on.
function copy() {
  menu.value = false
  clipboard[props.target.kind] = capturePreset('Clipboard', props.target)
}
function paste() {
  menu.value = false
  const p = clipboard[props.target.kind]
  if (p && applyPreset(p, props.target)) chosen[key.value] = ''
}
function rename(name: string) {
  naming.value = ''
  if (user.value) renamePreset(user.value.id, name)
}
function remove() {
  menu.value = false
  if (user.value && confirm(`Delete the preset “${user.value.name}”?`)) {
    deletePreset(user.value.id)
    chosen[key.value] = ''
  }
}
async function onImport(e: Event) {
  const input = e.target as HTMLInputElement
  const file = input.files?.[0]
  input.value = ''
  menu.value = false
  if (file) await importLibrary(file)
}
</script>

<script lang="ts">
/** The preset last chosen per target, shared by every bar ('f:id', 'u:id' or ''). */
const chosen = reactive<Record<string, string>>({})
</script>

<template>
  <div class="presets" :class="{ compact }" @keydown.stop>
    <label><span v-if="!compact">{{ label ?? 'Preset' }}</span>
      <select :disabled="!status.running" :value="value" :aria-label="compact ? (label ?? 'Preset') : undefined" @change="choose">
        <option value="">—</option>
        <optgroup v-if="factory?.length" label="Factory">
          <option v-for="[name, v] in factory" :key="v" :value="`f:${v}`">{{ name }}</option>
        </optgroup>
        <optgroup v-if="users.length" label="User">
          <option v-for="p in users" :key="p.id" :value="`u:${p.id}`">{{ p.name }}</option>
        </optgroup>
        <optgroup v-if="others.length" :label="target.kind === 'synth' ? 'Other models' : 'Other types'">
          <option v-for="p in others" :key="p.id" :value="`u:${p.id}`">{{ p.name }} ({{ p.model ?? p.type }})</option>
        </optgroup>
      </select>
    </label>
    <i v-if="dirty" class="dot" title="Changed since the preset was loaded" aria-label="modified">●</i>
    <EditableName
      v-if="naming" editing class="naming" :value="naming === 'rename' ? (user?.name ?? '') : ''" label="preset"
      @rename="naming === 'rename' ? rename($event) : saveAs($event)" @update:editing="(on) => !on && (naming = '')"
    />
    <template v-else>
      <template v-if="!compact">
        <button :disabled="!status.running || !user" :title="user ? `Save over “${user.name}”` : 'Load a user preset to save over it'" @click="save">Save</button>
        <button :disabled="!status.running" title="Save as a new user preset" @click="naming = 'save'">Save as…</button>
      </template>
      <span class="more">
        <button :aria-expanded="menu" title="Presets" @click="menu = !menu">⋯</button>
        <span v-if="menu" class="menu" role="menu">
          <template v-if="compact">
            <button role="menuitem" :disabled="!status.running || !user" @click="menu = false; save()">Save</button>
            <button role="menuitem" :disabled="!status.running" @click="menu = false; naming = 'save'">Save as…</button>
          </template>
          <button role="menuitem" :disabled="!user" @click="menu = false; naming = 'rename'">Rename</button>
          <button role="menuitem" :disabled="!user" @click="remove">Delete</button>
          <button role="menuitem" :disabled="!status.running" @click="copy">Copy</button>
          <button role="menuitem" :disabled="!status.running || !clipboard[target.kind]" @click="paste">Paste</button>
          <button role="menuitem" @click="menu = false; exportLibrary()">Export library</button>
          <label class="file" role="menuitem">Import library<input type="file" accept=".json" @change="onImport" /></label>
          <small v-if="!library.persistent">Not stored in this browser; export to keep</small>
        </span>
      </span>
    </template>
  </div>
</template>

<style scoped>
.presets { display: flex; gap: 6px; align-items: center; position: relative; font-size: 11px; letter-spacing: 0.12em; text-transform: uppercase; color: var(--con-silk-dim); }
label { display: flex; gap: 6px; align-items: center; }
select { font: 500 13px var(--con-font-silk); letter-spacing: 0.06em; background: var(--con-inset); color: var(--con-paper); border: 1px solid #343b46; border-radius: 3px; padding: 3px 6px; max-width: 180px; }
button, .file { font: 600 11px var(--con-font-silk); letter-spacing: 0.08em; text-transform: uppercase; background: var(--con-panel-2); color: var(--con-silk); border: 1px solid #39414d; border-radius: 3px; padding: 3px 7px; cursor: pointer; }
button:disabled { opacity: 0.45; cursor: default; }
.dot { color: var(--c, #f0a23b); font-style: normal; font-size: 12px; }
.naming { width: 150px; }
.compact select { max-width: 120px; font-size: 12px; padding: 2px 4px; }
.compact .naming { width: 120px; }
.more { position: relative; }
.menu { position: absolute; right: 0; top: calc(100% + 4px); z-index: 20; display: flex; flex-direction: column; gap: 2px; min-width: 170px; padding: 4px; background: #0f1217; border: 1px solid #3a424e; border-radius: 4px; box-shadow: 0 10px 24px #000c; }
.menu button, .menu .file { text-align: left; border: 0; background: none; padding: 5px 8px; }
.menu button:hover:not(:disabled), .menu .file:hover { background: #2c323b; color: var(--con-paper); }
.file input { display: none; }
small { padding: 4px 8px; text-transform: none; letter-spacing: 0; color: var(--con-silk-dim); }
</style>
