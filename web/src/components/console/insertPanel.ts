// Which insert slot's panel is open (#58): one panel, shared, anchored to the
// slot button that opened it.
import { reactive } from 'vue'

export const insertPanel = reactive({ open: false, strip: 0, slot: 0, title: '', x: 0, y: 0 })

const WIDTH = 322

/** Open the panel for `slot` (0–2) of `strip`, below the button `el`. */
export function showInsertPanel(strip: number, slot: number, title: string, el: HTMLElement) {
  const r = el.getBoundingClientRect()
  Object.assign(insertPanel, {
    open: true,
    strip,
    slot,
    title,
    x: Math.min(Math.max(r.left, 8), Math.max(8, innerWidth - WIDTH - 8)),
    y: Math.min(r.bottom + 6, Math.max(8, innerHeight - 190)),
  })
}

export const closeInsertPanel = () => { insertPanel.open = false }
