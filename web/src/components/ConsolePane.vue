<script setup lang="ts">
// The mixer console (spec 002 Req 2, spec 003): strips side by side, the four
// effect processors in the middle and the master section on the right. Groups
// are strips too; a tag under each tape says where a strip goes (ADR-0010).
// The view only sends values and draws what the engine reports; the order,
// collapsed and hidden strips are layout and live in the setup file.
import { computed } from 'vue'
import { feedsTag, GROUPS, groupColour, groupStrip, heardStrips, namedOuts, orderStrips, STRIPS } from '../audio/console'
import { addGroup, layout, moveStrip, params, player, removeGroup, renameSynth, setOut, status, stripName, synthColour, synths, toggleCollapsed, toggleHidden } from '../audio/engine'
import { modelDef } from '../audio/models'
import { Model, Pad, Param, type ParamId } from '../audio/params'
import ChannelStrip from './console/ChannelStrip.vue'
import InsertPanel from './console/InsertPanel.vue'
import StripPanel from './console/StripPanel.vue'
import MasterSection from './console/MasterSection.vue'
import ProcessorModule from './console/ProcessorModule.vue'

defineEmits<{ 'open-synth': [s: number] }>()

const val = (s: number, id: number) => params.values[s]?.[id] ?? 0
const shown = computed(() => [...synths.list, ...layout.groups.map(groupStrip)])
const order = computed(() => orderStrips(layout.order, shown.value).filter((id) => !layout.hidden.includes(id)))
const hidden = computed(() => shown.value.filter((id) => layout.hidden.includes(id)))

// A drum kit's pads go to groups by their own Out (#162): the groups it feeds.
const padOuts = Object.keys(Pad).map((n) => Param[`${n[0]?.toUpperCase()}${n.slice(1)}Out` as keyof typeof Param] as ParamId)
const feeds = (s: number) =>
  val(s, Param.Model) === Model.Tr808 ? padOuts.map((id) => Math.round(val(s, id))).filter((o) => o >= 1 && o <= GROUPS).map((o) => o - 1) : []
// Heard or silenced, as the engine's mixer decides it (mute, solo, groups).
const heard = computed(() =>
  heardStrips(
    Array.from({ length: STRIPS }, (_, i) => ({
      mute: val(i, Param.Mute) >= 0.5, solo: val(i, Param.Solo) >= 0.5, out: Math.round(val(i, Param.Out)), feeds: feeds(i),
    })),
  ),
)
const members = (g: number) => shown.value.filter((id) => id !== groupStrip(g) && Math.round(val(id, Param.Out)) === g + 1).length

const name = stripName
const strips = computed(() =>
  order.value.map((id) => {
    const group = id >= groupStrip(0)
    const g = id - groupStrip(0)
    const out = Math.round(val(id, Param.Out))
    const channels = group ? [] : player.parts.filter((p) => p.synth === id).map((p) => p.channel + 1)
    return {
      id,
      group,
      title: name(id),
      subtitle: group ? 'Bus' : modelDef(val(id, Param.Model)).name,
      color: group ? groupColour(g) : synthColour(id),
      footer: group ? `${members(g)} in` : channels.length ? `Ch ${channels.join('·')}` : '—',
      silenced: !heard.value[id],
      outs: namedOuts(id, layout.groups, name),
      feeds: feedsTag(out, name),
      collapsed: layout.collapsed.includes(id),
      g,
    }
  }),
)
const procOff = computed(() => [Param.P1Type, Param.P2Type, Param.P3Type, Param.P4Type].map((id) => Math.round(val(0, id)) === 0))
const canAdd = computed(() => status.running && layout.groups.length < GROUPS)
</script>

<template>
  <section class="pane console" aria-label="Mixer console">
    <div class="bar">
      <button :disabled="!canAdd" title="Add a group bus" @click="addGroup">+ Group</button>
      <button v-if="layout.collapsed.length" @click="layout.collapsed = []">Expand all</button>
      <span v-if="hidden.length" class="hid">Hidden:
        <button v-for="id in hidden" :key="id" :title="`Show ${name(id)} again`" @click="toggleHidden(id)">{{ name(id) }}</button>
      </span>
    </div>
    <div class="desk">
      <div class="strips">
        <ChannelStrip
          v-for="t in strips" :key="t.id" :s="t.id" :kind="t.group ? 'group' : 'synth'" :title="t.title" :subtitle="t.subtitle"
          :color="t.color" :footer="t.footer" :selected="!t.group && synths.selected === t.id" :silenced="t.silenced"
          :proc-off="procOff" :outs="t.outs" :feeds="t.feeds" :collapsed="t.collapsed"
          @select="!t.group && (synths.selected = t.id)" @open="!t.group && $emit('open-synth', t.id)"
          @collapse="toggleCollapsed(t.id)" @hide="toggleHidden(t.id)" @remove="removeGroup(t.g)"
          @move="(from) => moveStrip(from, t.id)" @set-out="(out) => setOut(t.id, out)" @rename="(n) => renameSynth(t.id, n)"
        />
      </div>
      <div class="rack">
        <h2>Processors</h2>
        <ProcessorModule v-for="n in 4" :key="n" :n="n - 1" />
      </div>
      <div class="master"><h2>Master</h2><MasterSection /></div>
    </div>
    <InsertPanel />
    <StripPanel />
  </section>
</template>

<style scoped>
.console {
  display: flex; flex-direction: column; background: linear-gradient(#171a20, #12151a); border-color: var(--con-line);
  box-shadow: 0 1px 0 #2a303a inset; overflow: auto; font-family: var(--con-font-silk); color: var(--con-silk);
}
.bar { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid #0b0d10; background: #15181e; font-size: 12px; position: sticky; left: 0; }
.bar button { font: 500 12px var(--con-font-silk); letter-spacing: 0.08em; text-transform: uppercase; padding: 2px 10px; }
.hid { display: flex; align-items: center; gap: 6px; color: var(--con-silk-dim); text-transform: uppercase; letter-spacing: 0.12em; }
.desk { display: flex; align-items: stretch; flex: 1 0 auto; min-width: min-content; }
.strips { display: flex; flex: 0 0 auto; }
.rack, .master { flex: 0 0 auto; border-left: 2px solid #0b0d10; box-shadow: 1px 0 0 #2a303a inset; padding: 12px 14px; }
.rack { width: 458px; display: flex; flex-direction: column; gap: 10px; background: var(--con-panel); }
.master { width: 372px; background: #181c22; }
h2 { margin: 0 0 2px; font-size: 13px; letter-spacing: 0.24em; text-transform: uppercase; color: var(--con-silk-dim); font-weight: 600; }
</style>
