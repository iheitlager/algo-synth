<script setup lang="ts">
// The mixer console (spec 002 Req 2, spec 003): one thin strip per synth side
// by side, the four effect processors in the middle and the master section on
// the right. The view only sends values and draws what the engine reports.
import { computed } from 'vue'
import { modelDef } from '../audio/models'
import { params, player, synthColour, synths } from '../audio/engine'
import { Param } from '../audio/params'
import ChannelStrip from './console/ChannelStrip.vue'
import KnobPop from './console/KnobPop.vue'
import MasterSection from './console/MasterSection.vue'
import ProcessorModule from './console/ProcessorModule.vue'

defineEmits<{ 'open-synth': [s: number] }>()

const val = (s: number, id: number) => params.values[s]?.[id] ?? 0
const anySolo = computed(() => synths.list.some((s) => val(s, Param.Solo) >= 0.5))
const strips = computed(() =>
  synths.list.map((s) => {
    const channels = player.parts.filter((p) => p.synth === s).map((p) => p.channel + 1)
    return {
      s,
      title: `Synth ${s + 1}`,
      subtitle: modelDef(val(s, Param.Model)).name,
      color: synthColour(s),
      footer: channels.length ? `Ch ${channels.join('·')}` : '—',
      silenced: val(s, Param.Mute) >= 0.5 || (anySolo.value && val(s, Param.Solo) < 0.5),
    }
  }),
)
const procOff = computed(() => [Param.P1Type, Param.P2Type, Param.P3Type, Param.P4Type].map((id) => Math.round(val(0, id)) === 0))
</script>

<template>
  <section class="pane console" aria-label="Mixer console">
    <div class="strips">
      <ChannelStrip
        v-for="t in strips" :key="t.s" :s="t.s" :title="t.title" :subtitle="t.subtitle" :color="t.color" :footer="t.footer"
        :selected="synths.selected === t.s" :silenced="t.silenced" :proc-off="procOff"
        @select="synths.selected = t.s" @open="$emit('open-synth', t.s)"
      />
    </div>
    <div class="rack">
      <h2>Processors</h2>
      <ProcessorModule v-for="n in 4" :key="n" :n="n - 1" />
    </div>
    <div class="master"><h2>Master</h2><MasterSection /></div>
    <KnobPop />
  </section>
</template>

<style scoped>
.console {
  display: flex; align-items: stretch; background: linear-gradient(#171a20, #12151a); border-color: var(--con-line);
  box-shadow: 0 1px 0 #2a303a inset; overflow-x: auto; overflow-y: auto; font-family: var(--con-font-silk); color: var(--con-silk);
}
.strips { display: flex; flex: 0 0 auto; }
.rack, .master { flex: 0 0 auto; border-left: 2px solid #0b0d10; box-shadow: 1px 0 0 #2a303a inset; padding: 12px 14px; }
.rack { width: 458px; display: flex; flex-direction: column; gap: 10px; background: var(--con-panel); }
.master { width: 372px; background: #181c22; }
h2 { margin: 0 0 2px; font-size: 13px; letter-spacing: 0.24em; text-transform: uppercase; color: var(--con-silk-dim); font-weight: 600; }
</style>
