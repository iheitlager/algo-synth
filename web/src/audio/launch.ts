// Launch (#489, epic #491): Ableton's Session view in the song's words
// (ADR-0031). Pads and keys launch scenes and switch snapshots; the engine
// lands them (#487, #488). Rec turns the scenes launched into the song's
// `arrange` line, so a jam becomes text (ADR-0018).
import { reactive } from 'vue'
import { Quantize, type QuantizeId } from './params'

/** Keys by `KeyboardEvent.code`, so the layout doesn't matter: 1–0 launch scenes 1–10, z–m switch snapshots 1–7. */
export const SCENE_KEYS = ['Digit1', 'Digit2', 'Digit3', 'Digit4', 'Digit5', 'Digit6', 'Digit7', 'Digit8', 'Digit9', 'Digit0']
export const SNAPSHOT_KEYS = ['KeyZ', 'KeyX', 'KeyC', 'KeyV', 'KeyB', 'KeyN', 'KeyM']
/** The key a pad shows. */
export const keyLabel = (code: string | undefined) => (code ?? '').replace(/^(Digit|Key)/, '').toLowerCase()

/** The moments `[` and `]` step through; Shift launches now. */
export const QUANTIZES: QuantizeId[] = [Quantize.Bar, Quantize.End, Quantize.Phrase]
export const QUANTIZE_NAMES: Record<number, string> = { [Quantize.Bar]: 'Bar', [Quantize.End]: 'End', [Quantize.Phrase]: 'Phrase', [Quantize.Now]: 'Now' }

/** When a launch lands, as chosen. */
export const launchState = reactive({ when: Quantize.Bar as QuantizeId })

export type Action =
  | { kind: 'scene'; index: number; now: boolean }
  | { kind: 'snapshot'; index: number; now: boolean }
  | { kind: 'quantize'; by: -1 | 1 }
  | { kind: 'resume' }
  | { kind: 'cancel' }
  | { kind: 'transport' }

interface Key {
  code: string
  shiftKey: boolean
  metaKey: boolean
  ctrlKey: boolean
  altKey: boolean
  repeat: boolean
  target: EventTarget | null
}

/** Whether a key goes to a field being typed in, not to Launch. */
export function typing(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null
  if (!el || typeof el.tagName !== 'string') return false
  return ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName) || el.isContentEditable === true
}

/** What a key does in Launch, or null when it isn't one of its keys. */
export function keyAction(e: Key): Action | null {
  if (e.repeat || e.metaKey || e.ctrlKey || e.altKey || typing(e.target)) return null
  const scene = SCENE_KEYS.indexOf(e.code)
  if (scene >= 0) return { kind: 'scene', index: scene, now: e.shiftKey }
  const snap = SNAPSHOT_KEYS.indexOf(e.code)
  if (snap >= 0) return { kind: 'snapshot', index: snap, now: e.shiftKey }
  switch (e.code) {
    case 'BracketLeft': return { kind: 'quantize', by: -1 }
    case 'BracketRight': return { kind: 'quantize', by: 1 }
    case 'Backslash': return { kind: 'resume' }
    case 'Escape': return { kind: 'cancel' }
    case 'Space': return { kind: 'transport' }
  }
  return null
}

/** The next moment `[` (−1) or `]` (+1) chooses, stopping at the ends. */
export function stepQuantize(when: QuantizeId, by: -1 | 1): QuantizeId {
  const i = QUANTIZES.indexOf(when)
  const next = Math.min(QUANTIZES.length - 1, Math.max(0, (i < 0 ? 0 : i) + by))
  return QUANTIZES[next] ?? Quantize.Bar
}

/** What Rec has heard: each scene each time it started, in order. */
export interface Rec {
  on: boolean
  entries: number[]
  /** The launched scene and its step at the last look. */
  scene: number
  local: number
}

export const newRec = (): Rec => ({ on: false, entries: [], scene: -1, local: -1 })

/**
 * Note where Launch is: a scene that starts (launched, or looping back to
 * its first step) is one entry. A scene cut short by the next launch still
 * counts whole, so the line plays back as heard when launches wait for the
 * end of the scene.
 */
export function recordStep(rec: Rec, launched: number, local: number): void {
  if (!rec.on) return
  if (launched >= 0 && local >= 0 && (launched !== rec.scene || local < rec.local)) rec.entries.push(launched)
  rec.scene = launched
  rec.local = local
}

/** How to make the arrangement `entries`: the `arrange` edits, removing the `current` entries from the last, then inserting. */
export function arrangeOps(current: number, entries: number[]): ({ op: 'remove'; at: number } | { op: 'insert'; at: number; scene: number })[] {
  const out: ({ op: 'remove'; at: number } | { op: 'insert'; at: number; scene: number })[] = []
  for (let at = current - 1; at >= 0; at--) out.push({ op: 'remove', at })
  entries.forEach((scene, at) => out.push({ op: 'insert', at, scene }))
  return out
}

/** How far off a launch lands, in words: steps of a 16-step bar. */
export function landsIn(steps: number): string {
  if (steps < 0) return ''
  if (steps < 16) return `${Math.ceil((steps || 1) / 4)} beat${Math.ceil((steps || 1) / 4) === 1 ? '' : 's'}`
  const bars = Math.ceil(steps / 16)
  return `${bars} bar${bars === 1 ? '' : 's'}`
}
