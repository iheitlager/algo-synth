<script setup lang="ts">
// The rail of synth tapes beside the faceplate (spec 003 Req 9): one per
// synth, with its model, its meter, mute and solo. A click selects it for the
// keyboard; a double-click on its name renames it (#178). Nothing here is
// decided: it shows what the engine reports.
// "+ Synth" opens the families and their models (#132).
import { nextTick, ref } from 'vue'
import { getEngine, levels, notInSong, params, status } from '../../audio/engine'
import { FAMILIES, familyModels, type ModelDef } from '../../audio/models'
import { Param, type ParamId } from '../../audio/params'
import LedMeter from '../console/LedMeter.vue'
import EditableName from '../EditableName.vue'

export interface Tape {
  s: number
  /** The synth's name (#127), its model's name and the model's accent. */
  name: string
  model: string
  accent: string
  /** The synth's own colour, as on the arranger's rows. */
  dot: string
  /** Where it is routed from, e.g. "Ch 1 · 3". */
  footer: string
  silenced: boolean
}

defineProps<{ tapes: Tape[]; selected: number; canAdd: boolean }>()
const emit = defineEmits<{ select: [s: number]; add: [model: ModelDef]; rename: [s: number, name: string] }>()
// The tape whose name is being edited, or −1. A field cannot sit inside the
// tape's button, so the button gives way to it while editing.
const editing = ref(-1)

// The add menu: a family adds its first model, a model itself.
const open = ref(false)
const menu = ref<HTMLElement>()
const items = () => [...(menu.value?.querySelectorAll<HTMLButtonElement>('button') ?? [])]
async function toggleMenu() {
  open.value = !open.value
  if (!open.value) return
  await nextTick()
  menu.value?.scrollIntoView({ block: 'nearest' })
  items()[0]?.focus()
}
function choose(model: ModelDef | undefined) {
  open.value = false
  if (model) emit('add', model)
}
function onKey(e: KeyboardEvent) {
  const list = items()
  const i = list.indexOf(document.activeElement as HTMLButtonElement)
  if (e.key === 'Escape') open.value = false
  else if (e.key === 'ArrowDown') list[(i + 1) % list.length]?.focus()
  else if (e.key === 'ArrowUp') list[(i - 1 + list.length) % list.length]?.focus()
  else return
  e.preventDefault()
  e.stopPropagation()
}

const val = (s: number, id: ParamId) => params.values[s]?.[id] ?? 0
const toggle = (s: number, id: ParamId) => getEngine()?.param(s, id, val(s, id) >= 0.5 ? 0 : 1)
const level = (s: number) => levels.values[s] ?? 0
</script>

<template>
  <nav class="rail" aria-label="Synths">
    <div
      v-for="t in tapes" :key="t.s" class="tape" :class="{ sel: selected === t.s, silenced: t.silenced }"
      :style="{ '--c': t.accent, '--dot': t.dot }"
    >
      <button
        v-if="editing !== t.s" class="pick" :aria-pressed="selected === t.s"
        :title="`Play ${t.name} (double-click to rename)`" @click="$emit('select', t.s)" @dblclick="editing = t.s"
      >
        <b><i class="dot" aria-hidden="true" />{{ t.name }}<i
          v-if="notInSong(t.s).length" class="live" role="img" :aria-label="`Not in the song: ${notInSong(t.s).join(', ')}`"
          :title="`Not in the song yet: ${notInSong(t.s).join(', ')}. It plays now, but a saved or reloaded song won't have it.`"
        >◐</i></b>
        <span>{{ t.model }} · {{ t.footer }}</span>
      </button>
      <div v-else class="pick">
        <b>
          <i class="dot" aria-hidden="true" />
          <EditableName
            :value="t.name" :label="t.name" :editing="true"
            @rename="emit('rename', t.s, $event)" @update:editing="(on) => { if (!on) editing = -1 }"
          />
        </b>
        <span>{{ t.model }} · {{ t.footer }}</span>
      </div>
      <div class="side">
        <LedMeter :level="level(t.s)" :height="46" :width="6" :segs="12" />
        <div class="ms">
          <button class="m" :aria-pressed="val(t.s, Param.Mute) >= 0.5" :disabled="!status.running" :aria-label="`Mute synth ${t.s + 1}`" title="Mute" @click="toggle(t.s, Param.Mute)">M</button>
          <button class="s" :aria-pressed="val(t.s, Param.Solo) >= 0.5" :disabled="!status.running" :aria-label="`Solo synth ${t.s + 1}`" title="Solo" @click="toggle(t.s, Param.Solo)">S</button>
        </div>
      </div>
    </div>
    <button class="add" :disabled="!status.running || !canAdd" :aria-expanded="open" aria-controls="add-menu" @click="toggleMenu">+ Synth</button>
    <div v-if="open" id="add-menu" ref="menu" class="menu" role="menu" aria-label="Add a synth" @keydown="onKey">
      <template v-for="f in FAMILIES" :key="f.id">
        <button class="family" role="menuitem" :title="`Add a ${familyModels(f.id)[0]?.name}`" @click="choose(familyModels(f.id)[0])">{{ f.label }}</button>
        <button
          v-for="m in familyModels(f.id)" :key="m.id" class="model" role="menuitem" :title="m.tagline"
          :style="{ '--c': m.theme.accent }" @click="choose(m)"
        >{{ m.name }}</button>
      </template>
    </div>
  </nav>
</template>

<style scoped>
.rail { display: flex; flex-direction: column; gap: 1px; width: 176px; flex: 0 0 176px; overflow-y: auto; background: var(--con-chassis); border-right: 2px solid #0b0d10; font-family: var(--con-font-silk); }
.tape { display: flex; align-items: stretch; gap: 4px; background: var(--con-panel); border-left: 5px solid var(--c); box-shadow: 0 1px 0 #0d0f13; }
.tape:nth-child(even) { background: #1b2028; }
.tape.sel { background: linear-gradient(#323843, #272c34); }
.tape.silenced .pick { opacity: 0.5; }
.pick { flex: 1; min-width: 0; text-align: left; padding: 7px 4px 7px 8px; background: none; border: 0; border-radius: 0; color: inherit; cursor: pointer; font: inherit; display: grid; gap: 1px; }
.pick:focus-visible { outline: 2px solid var(--con-paper); outline-offset: -2px; }
.pick b { color: var(--con-paper); font-size: 15px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.pick span { color: var(--con-silk-dim); font-size: 11px; letter-spacing: 0.1em; text-transform: uppercase; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; background: var(--dot); margin-right: 7px; vertical-align: 1px; }
.live { margin-left: 6px; font-style: normal; font-size: 11px; color: var(--accent); vertical-align: 1px; cursor: help; }
.side { display: flex; gap: 5px; align-items: center; padding: 4px 6px 4px 0; }
.ms { display: flex; flex-direction: column; gap: 3px; }
.ms button {
  width: 20px; height: 20px; padding: 0; border-radius: 3px; border: 1px solid #39414d; cursor: pointer; background: linear-gradient(#2e343d, #232830);
  font: 700 11px var(--con-font-silk); color: var(--con-silk); box-shadow: 0 1px 0 #0a0c0f;
}
.ms button:disabled { opacity: 0.5; cursor: default; }
.ms .m[aria-pressed='true'] { background: #7a2a22; color: #fff; border-color: #e0654f; box-shadow: 0 0 8px #e0654f55; }
.ms .s[aria-pressed='true'] { background: #8a6a10; color: #fff; border-color: #f0c23a; box-shadow: 0 0 8px #f0c23a55; }
.add { margin: 6px; padding: 6px; font: 600 12px var(--con-font-silk); letter-spacing: 0.12em; text-transform: uppercase; background: var(--con-panel-2); color: var(--con-silk); border: 1px dashed #39414d; border-radius: 3px; cursor: pointer; }
.add:hover:not(:disabled) { color: var(--con-paper); border-color: var(--con-silk-dim); }
.menu { display: flex; flex-direction: column; margin: 0 6px 6px; padding: 4px 0; background: var(--con-panel-2); border: 1px solid #39414d; border-radius: 3px; }
.menu button { text-align: left; background: none; border: 0; border-radius: 0; cursor: pointer; font: inherit; color: var(--con-silk); }
.menu button:hover, .menu button:focus-visible { background: #2c323b; color: var(--con-paper); outline: none; }
.menu .family { padding: 6px 10px 3px; font-size: 12px; font-weight: 700; letter-spacing: 0.14em; text-transform: uppercase; color: var(--con-paper); }
.menu .model { padding: 3px 10px 3px 18px; font-size: 12px; letter-spacing: 0.08em; border-left: 3px solid var(--c); }
</style>
