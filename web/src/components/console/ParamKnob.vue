<script setup lang="ts">
// A knob bound to one engine parameter (#54): it shows the value the engine
// reports for `synth` (`params`), maps it through `scale`, and sends changes
// as messages. A knob the song's modulation drives is marked (ADR-0019).
import { computed } from 'vue'
import type { Scale } from '../../audio/console'
import { lin } from '../../audio/console'
import { getEngine, modKey, modulated, params } from '../../audio/engine'
import type { ParamId } from '../../audio/params'
import Knob from './Knob.vue'

const props = withDefaults(
  defineProps<{
    synth: number
    id: ParamId
    label: string
    name?: string
    scale?: Scale
    /** The readout for an engine value. */
    text?: (v: number) => string
    /** Where double-click puts it, as an engine value. */
    def?: number
    size?: number
    color?: string
    bipolar?: boolean
    noVal?: boolean
  }>(),
  { scale: () => lin(0, 1), def: 0 },
)

const value = computed(() => params.values[props.synth]?.[props.id] ?? 0)
const driven = computed(() => modulated.keys.has(modKey(props.synth, props.id)))
const pos = computed(() => props.scale.toPos(value.value))
const fmt = (t: number) => (props.text ? props.text(props.scale.toValue(t)) : `${Math.round(t * 100)}%`)
const send = (t: number) => getEngine()?.param(props.synth, props.id, props.scale.toValue(t))
</script>

<template>
  <Knob
    :model-value="pos" :label="label" :name="name" :def="scale.toPos(def)" :size="size" :color="color" :bipolar="bipolar"
    :no-val="noVal" :fmt="fmt" :modulated="driven" @update:model-value="send"
  />
</template>
