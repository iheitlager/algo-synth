// The one value popover the console shares (#52): a knob being dragged shows
// its value, a knob that is clicked shows a slider to use with the mouse.
import { reactive } from 'vue'

export const pop = reactive({
  open: false,
  /** A drag only shows the value; a click lets the slider be used. */
  interactive: false,
  /** Which knob has it, so a knob only updates its own readout. */
  owner: null as symbol | null,
  label: '',
  color: '',
  text: '',
  /** The knob's value 0..1, shown on the slider. */
  value: 0,
  x: 0,
  y: 0,
  /** Where the arrow points, in px from the popover's left edge. */
  arrow: 0,
  below: false,
  apply: null as ((v: number) => void) | null,
})

const WIDTH = 188
const HEIGHT = 74

/** Open the popover over `el`; it sits above it, or below when there is no room. */
export function showPop(
  owner: symbol, el: HTMLElement, o: { label: string; color: string; text: string; value: number; interactive: boolean; apply: (v: number) => void },
) {
  const r = el.getBoundingClientRect()
  const left = Math.min(Math.max(r.left + r.width / 2 - WIDTH / 2, 8), Math.max(8, innerWidth - WIDTH - 8))
  const above = r.top - HEIGHT - 12
  pop.below = above < 8
  pop.y = pop.below ? r.bottom + 12 : above
  pop.x = left
  pop.arrow = r.left + r.width / 2 - left
  Object.assign(pop, { open: true, owner, ...o })
}

/** Keep the readout in step with the knob it belongs to. */
export function updatePop(owner: symbol, text: string, value: number) {
  if (pop.owner === owner) Object.assign(pop, { text, value })
}

export function closePop() {
  pop.open = false
  pop.owner = null
  pop.apply = null
}
