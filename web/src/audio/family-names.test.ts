import { beforeEach, describe, expect, it } from 'vitest'
import { addSynth, params, removeSynth, renameSynth, stripName, synths } from './engine'
import { familyModels, type ModelDef } from './models'
import { names, setNames } from './names'
import { Param } from './params'

const first = (family: Parameters<typeof familyModels>[0]) => familyModels(family)[0] as ModelDef

// Without audio the engine is not running: adding shows the synth and names it,
// and the view's copy of each synth's model is set here as the engine would.
function add(model: ModelDef): number {
  expect(addSynth(model)).toBe(true)
  const s = synths.selected
  params.values[s] = Object.assign([], params.values[s], { [Param.Model]: model.id })
  return s
}

describe('instruments named by family (#177)', () => {
  beforeEach(() => {
    setNames()
    synths.list = [0]
    synths.selected = 0
    params.values = []
  })

  it('names drums, samplers and synths each from 1, in the order they are added', () => {
    const d1 = add(first('drums'))
    const d2 = add(first('drums'))
    const s1 = add(first('poly'))
    const m1 = add(first('samplers'))
    // The first synth, there from the start, is Synth 1; the next synth is Synth 2.
    expect(stripName(0)).toBe('Synth 1')
    expect([d1, d2, s1, m1].map((s) => stripName(s))).toEqual(['Drum 1', 'Drum 2', 'Synth 2', 'Sampler 1'])
  })

  it('takes the lowest free number again, and never renames the others', () => {
    const d1 = add(first('drums'))
    const d2 = add(first('drums'))
    removeSynth(d1)
    expect(names.strips[d1]).toBeUndefined()
    expect(stripName(d2)).toBe('Drum 2')
    const again = add(first('drums'))
    expect(stripName(again)).toBe('Drum 1')
    expect(stripName(d2)).toBe('Drum 2')
  })

  it('an empty name goes back to the family default; a group to Group N', () => {
    const d = add(first('drums'))
    renameSynth(d, 'Kit')
    expect(stripName(d)).toBe('Kit')
    renameSynth(d, '  ')
    expect(stripName(d)).toBe('Drum 1')
    renameSynth(17, 'Hats')
    renameSynth(17, '')
    expect(stripName(17)).toBe('Group 2')
  })
})
