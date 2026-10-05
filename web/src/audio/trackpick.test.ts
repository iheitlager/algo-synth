import { describe, expect, it } from 'vitest'
import type { SongTrack } from './engine'
import { modelDef } from './models'
import { Model, Preset } from './params'
import { choiceOf, modelOfPreset, modelsFor, parseChoice, presetChoices } from './trackpick'

// As the engine sends it: drums, synth, sampler × the 19 models.
const fits = [0, 1, 2].map((k) =>
  Array.from({ length: 19 }, (_, m) => {
    const drums = m === Model.Tr808 || m === Model.Tr909 || m === Model.PadSampler
    const sampler = m === Model.Sampler || m === Model.PadSampler
    return k === 0 ? drums : k === 2 ? sampler : !drums && !sampler
  }),
)
const track = (over: Partial<SongTrack>): SongTrack => ({ name: 't', synth: 0, kind: 'synth', preset: -1, setting: -1, ...over })

describe('the track picker (#213)', () => {
  it('offers the models that play the track kind', () => {
    expect(modelsFor('drums', fits).map((m) => m.id)).toEqual([Model.Tr808, Model.PadSampler, Model.Tr909])
    expect(modelsFor('sampler', fits).map((m) => m.id)).toEqual([Model.Sampler, Model.PadSampler])
    const synths = modelsFor('synth', fits).map((m) => m.id)
    expect(synths).toContain(Model.Minimoog)
    expect(synths).not.toContain(Model.Tr808)
    expect(modelsFor('synth', [[], [], []])).toEqual([])
  })

  it("lists a model's factory presets, then the song's settings on that model", () => {
    const settings = [{ name: 'nile', preset: Preset.MiniLead }, { name: 'acid', preset: Preset.AcidBass }]
    const choices = presetChoices(modelDef(Model.Minimoog), settings)
    expect(choices.map((c) => c.label)).toEqual(['MiniBass', 'MiniLead', 'LuckyMan', 'FunkBass', 'MoogStrings', 'nile (song)'])
    expect(choices.at(-1)?.value).toBe('s0')
    expect(modelOfPreset(Preset.AcidBass)?.id).toBe(Model.Sh101)
  })

  it('shows what a track plays and reads a choice back', () => {
    expect(choiceOf(track({ preset: Preset.MiniBass }))).toBe(`p${Preset.MiniBass}`)
    expect(choiceOf(track({ preset: Preset.MiniLead, setting: 2 }))).toBe('s2')
    expect(choiceOf(track({}))).toBe('')
    expect(parseChoice('p12')).toEqual({ kind: 'preset', id: 12 })
    expect(parseChoice('s0')).toEqual({ kind: 'setting', id: 0 })
    expect(parseChoice('x3')).toBeNull()
    expect(parseChoice('p')).toBeNull()
  })
})
