import { beforeEach, describe, expect, it } from 'vitest'
import { familyName, names, renameStrip, setNames, stripName } from './names'

describe('names (#127)', () => {
  beforeEach(() => setNames())

  it('defaults to Synth N and Group N until renamed', () => {
    expect(stripName(3)).toBe('Synth 4')
    expect(stripName(17)).toBe('Group 2')
    renameStrip(1, 'Low strings')
    expect(stripName(1)).toBe('Low strings')
  })

  it('trims, caps and clears back to the default', () => {
    renameStrip(16, '   Drum   bus ')
    expect(stripName(16)).toBe('Drum bus')
    renameStrip(16, 'x'.repeat(40))
    expect(stripName(16)).toHaveLength(24)
    renameStrip(16, '   ')
    expect(stripName(16)).toBe('Group 1')
    expect(names.strips).toEqual({})
  })

  it('replaces every name when a setup is applied', () => {
    renameStrip(2, 'Old')
    setNames({ strips: { 3: 'Pad' } })
    expect(names.strips).toEqual({ 3: 'Pad' })
  })

  it('names an instrument by its family with the lowest free number (#177)', () => {
    expect(familyName('drums', [])).toBe('Drum 1')
    expect(familyName('drums', ['Drum 1', 'Synth 1', 'Drum 3'])).toBe('Drum 2')
    expect(familyName('samplers', ['Drum 1'])).toBe('Sampler 1')
    expect(familyName('mono', ['Synth 1'])).toBe('Synth 2')
    expect(familyName('poly', ['Synth 1', 'Synth 2'])).toBe('Synth 3')
  })
})
