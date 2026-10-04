// Which strip's preset panel is open (#152): one panel, shared, anchored to
// the strip tool that opened it, as the insert panel is.
import { reactive } from 'vue'

export const stripPanel = reactive({ open: false, strip: 0, title: '', x: 0, y: 0 })

const WIDTH = 300

export function showStripPanel(strip: number, title: string, el: HTMLElement) {
  const r = el.getBoundingClientRect()
  Object.assign(stripPanel, {
    open: true,
    strip,
    title,
    x: Math.min(Math.max(r.left, 8), Math.max(8, innerWidth - WIDTH - 8)),
    y: Math.min(r.bottom + 6, Math.max(8, innerHeight - 160)),
  })
}

export const closeStripPanel = () => { stripPanel.open = false }
