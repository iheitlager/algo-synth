// The composer's dividers (#373): how wide the song text is beside the grid,
// and how tall the arranger is under the composer. Kept in the browser as the
// rest of the view is (ADR-0027); null is the layout's own default. Storage can
// be unavailable: then a reload starts from the defaults.

import { reactive } from 'vue'

export const SPLIT_KEY = 'algo-synth:splits'

type Store = Pick<Storage, 'getItem' | 'setItem'>
const local = (): Store => localStorage

export interface Splits {
  /** The song text's width beside the composer's grid, in pixels. */
  code: number | null
  /** The arranger's height under the composer, in pixels. */
  arranger: number | null
  /** The Assistant's width beside the views (#387). */
  assistant: number | null
}

/** The smallest each pane may be, and the share of its container it may take. */
export const LIMITS = { code: 260, arranger: 120, assistant: 300, share: 0.7 } as const
/** Pixels an arrow key moves a divider. */
export const STEP = 16

/** `size` between `min` and `share` of `room`; never under `min`. */
export const clampSize = (size: number, min: number, room: number, share: number = LIMITS.share) =>
  Math.round(Math.max(min, Math.min(size, room * share)))

/** The size a drag leads to: the pane after the divider grows as the divider
 * moves back (left or up), by as much as it moved. */
export const dragged = (start: number, from: number, to: number) => start + (from - to)

/** The size an arrow key leads to, or null for any other key: left or up
 * moves the divider back, so the pane after it grows. */
export function stepped(size: number, key: string): number | null {
  if (key === 'ArrowLeft' || key === 'ArrowUp') return size + STEP
  if (key === 'ArrowRight' || key === 'ArrowDown') return size - STEP
  return null
}

const size = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) && v > 0 ? Math.round(v) : null)

/** The splits kept last; the defaults without them or when they don't read. */
export function lastSplits(store: () => Store = local): Splits {
  try {
    const raw = store().getItem(SPLIT_KEY)
    const v = raw ? (JSON.parse(raw) as Record<string, unknown>) : {}
    return { code: size(v.code), arranger: size(v.arranger), assistant: size(v.assistant) }
  } catch {
    return { code: null, arranger: null, assistant: null }
  }
}

/** Keep `splits` for the next visit. */
export function keepSplits(splits: Splits, store: () => Store = local) {
  try {
    store().setItem(SPLIT_KEY, JSON.stringify(splits))
  } catch {
    // Private window or storage full: the dividers start at their defaults next time.
  }
}

/** The dividers as they are now, shared by the composer and the layout. */
export const splits = reactive<Splits>(lastSplits())

/** Move divider `which` to `size` (null back to the default) and keep it. */
export function setSplit(which: keyof Splits, size: number | null) {
  splits[which] = size
  keepSplits({ ...splits })
}
