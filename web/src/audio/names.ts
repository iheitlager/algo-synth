// Names for the strips (synths 0–15 and their console strips, groups 16–23)
// and the MIDI lanes (#127). Labels only: they never reach the engine, and
// they are saved in the setup file like the console layout (ADR-0001).
import { reactive } from 'vue'
import { cleanName } from './setup'

export { cleanName, MAX_NAME } from './setup'
const GROUP_BASE = 16

/** What the user named, by strip index and by MIDI channel; absent is the default. */
export const names = reactive({ strips: {} as Record<number, string>, parts: {} as Record<number, string> })

export const defaultStripName = (s: number) => (s < GROUP_BASE ? `Synth ${s + 1}` : `Group ${s - GROUP_BASE + 1}`)

/** A lane as the file names it, or by its channel. */
export const defaultPartName = (part: { channel: number; name: string }) => cleanName(part.name) || `Channel ${part.channel + 1}`

export const partName = (part: { channel: number; name: string }) => names.parts[part.channel] ?? defaultPartName(part)

/**
 * A strip's name: the user's, else for a synth the name of the first lane it
 * plays (so a loaded file names its synths), else `Synth N` / `Group N`.
 */
export function stripName(s: number, parts: readonly { channel: number; name: string; synth: number }[] = []): string {
  const own = names.strips[s]
  if (own !== undefined) return own
  const lane = s < GROUP_BASE ? parts.find((p) => p.synth === s) : undefined
  return lane ? partName(lane) : defaultStripName(s)
}

/** Name strip `s`; an empty name goes back to the default. */
export function renameStrip(s: number, raw: string) {
  const name = cleanName(raw)
  if (name) names.strips[s] = name
  else delete names.strips[s]
}

/** Name the lane on `channel`; an empty name goes back to the file's. */
export function renamePart(channel: number, raw: string) {
  const name = cleanName(raw)
  if (name) names.parts[channel] = name
  else delete names.parts[channel]
}

/** Replace every name (a setup was applied); keys are strip indices and channels. */
export function setNames(next: { strips?: Record<string, string>; parts?: Record<string, string> } = {}) {
  names.strips = Object.fromEntries(Object.entries(next.strips ?? {}).map(([k, v]) => [Number(k), v]))
  names.parts = Object.fromEntries(Object.entries(next.parts ?? {}).map(([k, v]) => [Number(k), v]))
}
