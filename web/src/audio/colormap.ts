// Colours for spectra (#519): the "turbo" map, dark blue through green and
// yellow to dark red, as a polynomial fit (Google's Turbo, Apache-2.0). Only
// drawing: the levels themselves come from the engine.

/** The colour of a level from 0 to 1, as [r, g, b] in 0..255. */
export function turbo(t: number): [number, number, number] {
  const x = Math.min(1, Math.max(0, t))
  const r = 0.13572138 + x * (4.6153926 + x * (-42.66032258 + x * (132.13108234 + x * (-152.94239396 + x * 59.28637943))))
  const g = 0.09140261 + x * (2.19418839 + x * (4.84296658 + x * (-14.18503333 + x * (4.27729857 + x * 2.82956604))))
  const b = 0.1066733 + x * (12.64194608 + x * (-60.58204836 + x * (110.36276771 + x * (-89.90310912 + x * 27.34824973))))
  const byte = (v: number) => Math.round(Math.min(1, Math.max(0, v)) * 255)
  return [byte(r), byte(g), byte(b)]
}

/** The 256 colours of `turbo`, packed as r, g, b, so a byte level indexes it directly. */
export const TURBO: Uint8ClampedArray = (() => {
  const lut = new Uint8ClampedArray(256 * 3)
  for (let i = 0; i < 256; i++) lut.set(turbo(i / 255), i * 3)
  return lut
})()

/**
 * An RGBA image of a spectrogram: `frames` columns of `bands` byte levels
 * (band 0 the lowest), drawn with low frequencies at the bottom.
 */
export function spectrogramPixels(levels: Uint8Array, bands: number): { width: number; height: number; data: Uint8ClampedArray<ArrayBuffer> } {
  const width = bands > 0 ? Math.floor(levels.length / bands) : 0
  const data = new Uint8ClampedArray(new ArrayBuffer(width * bands * 4))
  for (let f = 0; f < width; f++) {
    for (let b = 0; b < bands; b++) {
      const level = levels[f * bands + b]
      const at = ((bands - 1 - b) * width + f) * 4
      data[at] = TURBO[level * 3]
      data[at + 1] = TURBO[level * 3 + 1]
      data[at + 2] = TURBO[level * 3 + 2]
      data[at + 3] = 255
    }
  }
  return { width, height: bands, data }
}
