import { beforeEach, describe, expect, it } from 'vitest'
import { names, partName, renamePart, renameStrip, setNames, stripName } from './names'

const parts = [
  { channel: 0, name: 'Violin I', synth: 0 },
  { channel: 1, name: '', synth: 1 },
  { channel: 2, name: 'Viola', synth: 1 },
]

describe('names (#127)', () => {
  beforeEach(() => setNames())

  it('defaults to Synth N, Group N and the file name or Channel N', () => {
    expect(stripName(3)).toBe('Synth 4')
    expect(stripName(17)).toBe('Group 2')
    expect(partName(parts[0]!)).toBe('Violin I')
    expect(partName(parts[1]!)).toBe('Channel 2')
  })

  it('names a synth after the first lane it plays until it is renamed', () => {
    expect(stripName(0, parts)).toBe('Violin I')
    expect(stripName(1, parts)).toBe('Channel 2')
    renamePart(1, 'Cello')
    expect(stripName(1, parts)).toBe('Cello')
    renameStrip(1, 'Low strings')
    expect(stripName(1, parts)).toBe('Low strings')
    expect(stripName(5, parts)).toBe('Synth 6')
  })

  it('trims, caps and clears back to the default', () => {
    renameStrip(16, '   Drum   bus ')
    expect(stripName(16)).toBe('Drum bus')
    renameStrip(16, 'x'.repeat(40))
    expect(stripName(16)).toHaveLength(24)
    renameStrip(16, '   ')
    expect(stripName(16)).toBe('Group 1')
    expect(names.strips).toEqual({})
    renamePart(0, 'Solo')
    renamePart(0, '')
    expect(partName(parts[0]!)).toBe('Violin I')
  })

  it('replaces every name when a setup is applied', () => {
    renameStrip(2, 'Old')
    setNames({ strips: { 3: 'Pad' }, parts: { 0: 'Lead' } })
    expect(names.strips).toEqual({ 3: 'Pad' })
    expect(partName(parts[0]!)).toBe('Lead')
  })
})
