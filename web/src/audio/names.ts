// Names for the strips (synths 0–15 and their console strips, groups 16–23,
// #127). Labels only: they never reach the engine, and they are saved in the
// setup file like the console layout (ADR-0001).
import { reactive } from 'vue'
import { cleanName } from './setup'

export { cleanName, MAX_NAME } from './setup'
const GROUP_BASE = 16

/** What the user named, by strip index; absent is the default. */
export const names = reactive({ strips: {} as Record<number, string> })

export const defaultStripName = (s: number) => (s < GROUP_BASE ? `Synth ${s + 1}` : `Group ${s - GROUP_BASE + 1}`)

/** The word an instrument of a family is named with (#177): a drum is not a synth. */
const FAMILY_WORD: Record<string, string> = { drums: 'Drum', samplers: 'Sampler' }

/** A name the app gave (`Drum N`, `Sampler N`, `Synth N`), not one the user typed. */
export const isFamilyName = (name: string) => /^(Drum|Sampler|Synth) \d+$/.test(name)

/** `Drum N`, `Sampler N` or `Synth N` with the lowest N none of `taken` has. */
export function familyName(family: string, taken: readonly string[]): string {
  const word = FAMILY_WORD[family] ?? 'Synth'
  let n = 1
  while (taken.includes(`${word} ${n}`)) n++
  return `${word} ${n}`
}

/** A strip's name: the user's, else `Synth N` / `Group N`. */
export const stripName = (s: number): string => names.strips[s] ?? defaultStripName(s)

/** Name strip `s`; an empty name goes back to the default. */
export function renameStrip(s: number, raw: string) {
  const name = cleanName(raw)
  if (name) names.strips[s] = name
  else delete names.strips[s]
}

/** Replace every name (a setup was applied); keys are strip indices. */
export function setNames(next: { strips?: Record<string, string> } = {}) {
  names.strips = Object.fromEntries(Object.entries(next.strips ?? {}).map(([k, v]) => [Number(k), v]))
}
