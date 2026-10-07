// What the screen looks like, kept in the browser (ADR-0027): the console's
// group buses, strip order, collapsed and hidden strips, and the names the
// user typed. The sound and the mix are the song's; this is only the view,
// so a reload looks as it was left. Storage can be unavailable: then a
// reload starts with the defaults.

export const VIEW_KEY = 'algo-synth:view'

type Store = Pick<Storage, 'getItem' | 'setItem'>
const local = (): Store => localStorage

export interface ViewState {
  groups: number[]
  order: number[]
  collapsed: number[]
  hidden: number[]
  /** Names typed by the user, by strip; the song's track names are not kept. */
  names: Record<number, string>
}

const ints = (v: unknown): number[] => (Array.isArray(v) ? v.filter((x): x is number => Number.isInteger(x)) : [])

/** Keep `state` as the view to come back to. */
export function keepView(state: ViewState, store: () => Store = local) {
  try {
    store().setItem(VIEW_KEY, JSON.stringify(state))
  } catch {
    // Private window or storage full: the view starts fresh next time.
  }
}

/** The view kept last, or null without one or when it doesn't read. */
export function lastView(store: () => Store = local): ViewState | null {
  try {
    const raw = store().getItem(VIEW_KEY)
    if (!raw) return null
    const v = JSON.parse(raw) as Record<string, unknown>
    const names: Record<number, string> = {}
    if (v.names && typeof v.names === 'object') {
      for (const [k, n] of Object.entries(v.names as Record<string, unknown>)) {
        if (Number.isInteger(Number(k)) && typeof n === 'string') names[Number(k)] = n
      }
    }
    return { groups: ints(v.groups), order: ints(v.order), collapsed: ints(v.collapsed), hidden: ints(v.hidden), names }
  } catch {
    return null
  }
}
