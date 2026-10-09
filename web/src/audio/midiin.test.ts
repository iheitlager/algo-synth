import { describe, expect, it, vi } from 'vitest'
import { forward, listen, midi } from './midiin'

describe('forward', () => {
  it('passes a message on as it came, three bytes', () => {
    expect(forward(new Uint8Array([0x90, 60, 100]))).toEqual({ t: 'midiIn', b: [0x90, 60, 100] })
    // Two-byte messages and anything longer: the engine reads the bytes, not this.
    expect(forward(new Uint8Array([0xd0, 5]))).toEqual({ t: 'midiIn', b: [0xd0, 5, 0] })
    expect(forward(new Uint8Array([0xf8]))).toEqual({ t: 'midiIn', b: [0xf8, 0, 0] })
    expect(forward(new Uint8Array([]))).toBeNull()
    expect(forward(null)).toBeNull()
  })
})

/** A MIDIAccess with the given inputs, enough for `listen`. */
function fakeAccess(names: string[]) {
  const inputs = new Map(names.map((name, i) => [String(i), { name, state: 'connected', onmidimessage: null as null | ((e: { data: Uint8Array }) => void) }]))
  return { inputs, onstatechange: null as null | (() => void) }
}

describe('listen', () => {
  it('forwards every input and lists them', () => {
    const access = fakeAccess(['MPK mini Plus Port 1', 'MPK mini Plus Port 2'])
    const post = vi.fn()
    listen(access as unknown as MIDIAccess, post)
    expect(midi.inputs).toEqual(['MPK mini Plus Port 1', 'MPK mini Plus Port 2'])
    access.inputs.get('0')!.onmidimessage!({ data: new Uint8Array([0xb0, 70, 21]) })
    access.inputs.get('1')!.onmidimessage!({ data: new Uint8Array([0x80, 60, 0]) })
    expect(post.mock.calls.map((c) => c[0].b)).toEqual([[0xb0, 70, 21], [0x80, 60, 0]])
    expect(midi.active).toBe(true)
  })

  it('follows devices coming and going', () => {
    const access = fakeAccess(['A', 'B'])
    listen(access as unknown as MIDIAccess, vi.fn())
    access.inputs.get('1')!.state = 'disconnected'
    access.onstatechange!()
    expect(midi.inputs).toEqual(['A'])
  })
})
