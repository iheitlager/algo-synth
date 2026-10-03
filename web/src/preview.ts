import { createApp, h } from 'vue'
import './style.css'
import './console.css'
import { params, status } from './audio/engine'
import { MODELS, scaleOf } from './audio/models'
import SynthFaceplate from './components/SynthFaceplate.vue'
import KnobPop from './components/console/KnobPop.vue'

status.running = true
const q = new URLSearchParams(location.search)
const only = q.get('m')
MODELS.forEach((m, s) => {
  const v: number[] = new Array(200).fill(0)
  for (const sec of m.sections) for (const c of sec.controls) {
    if (c.kind === 'knob') v[c.param] = scaleOf(c).toValue(0.4 + ((c.param * 37) % 50) / 100)
    if (c.kind === 'select') v[c.param] = c.options[(c.param % c.options.length)]![1]
    if (c.kind === 'switch') v[c.param] = c.param % 2
    if (c.kind === 'env') { v[c.a] = 0.04; if (c.d) v[c.d] = 0.4; if (c.s) v[c.s] = 0.6; if (c.r) v[c.r] = 0.9 }
  }
  params.values[s] = v
})
createApp({ render: () => h('div', { style: 'padding:12px;display:grid;gap:12px;max-width:1500px' },
  [...MODELS.entries()].filter(([, m]) => !only || m.name === only).map(([s, m]) => h(SynthFaceplate, { s, def: m }))
  .concat([h(KnobPop) as never])) }).mount('#app')
