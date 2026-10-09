// Web MIDI input (#10, #257): every message from every input goes to the
// engine as it came, and the engine reads it (ADR-0001); nothing here knows
// what a message means. Chrome and Firefox; Safari has no Web MIDI.
import { reactive } from 'vue'

export const midi = reactive({
  /** The browser has Web MIDI. */
  supported: typeof navigator !== 'undefined' && 'requestMIDIAccess' in navigator,
  /** The inputs connected now, by name. */
  inputs: [] as string[],
  /** A message came in the last moment, for the light. */
  active: false,
  /** Why there is no MIDI, if access was refused. */
  error: '',
})

export type MidiInMsg = { t: 'midiIn'; b: [number, number, number] }

/** The engine message for one MIDI message: its first three bytes as they came. */
export function forward(data: ArrayLike<number> | null): MidiInMsg | null {
  if (!data || data.length === 0) return null
  return { t: 'midiIn', b: [data[0], data[1] ?? 0, data[2] ?? 0] }
}

let blink: ReturnType<typeof setTimeout> | undefined

/** Listen to every input `access` has, now and as devices come and go. */
export function listen(access: MIDIAccess, post: (msg: MidiInMsg) => void): void {
  const attach = () => {
    const names: string[] = []
    access.inputs.forEach((input) => {
      if (input.state !== 'connected') return
      names.push(input.name ?? 'MIDI input')
      input.onmidimessage = (e) => {
        const msg = forward(e.data)
        if (!msg) return
        post(msg)
        midi.active = true
        clearTimeout(blink)
        blink = setTimeout(() => (midi.active = false), 120)
      }
    })
    midi.inputs = names
  }
  access.onstatechange = attach
  attach()
}

/** Ask for MIDI access (no SysEx) and forward what comes in; a refusal is kept in `midi.error`. */
export async function startMidi(post: (msg: MidiInMsg) => void): Promise<void> {
  if (!midi.supported) return
  try {
    listen(await navigator.requestMIDIAccess({ sysex: false }), post)
  } catch (e) {
    midi.error = e instanceof Error ? e.message : String(e)
  }
}
