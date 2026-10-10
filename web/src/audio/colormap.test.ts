// The spectrum colours (#519): the turbo map's ends, and a spectrogram's
// orientation (low bands at the bottom, frames left to right).
import { describe, expect, it } from 'vitest'
import { TURBO, spectrogramPixels, turbo } from './colormap'

describe('the turbo colours', () => {
  it('run from near black through blue and green to dark red', () => {
    expect(turbo(0).reduce((a, b) => a + b)).toBeLessThan(120)
    const [rBlue, gBlue, bBlue] = turbo(0.1)
    expect(bBlue).toBeGreaterThan(rBlue)
    expect(bBlue).toBeGreaterThan(gBlue)
    const [, gMid] = turbo(0.5)
    expect(gMid).toBeGreaterThan(200)
    const [r1, g1, b1] = turbo(1)
    expect(r1).toBeGreaterThan(g1)
    expect(r1).toBeGreaterThan(b1)
  })

  it('clamp outside 0..1 and fill the table', () => {
    expect(turbo(-1)).toEqual(turbo(0))
    expect(turbo(2)).toEqual(turbo(1))
    expect(TURBO).toHaveLength(256 * 3)
    expect(Array.from(TURBO.slice(255 * 3))).toEqual(turbo(1))
  })
})

describe('a spectrogram image', () => {
  it('puts band 0 at the bottom and frames left to right', () => {
    // Two frames of three bands: frame 0 loud in band 0, frame 1 loud in band 2.
    const px = spectrogramPixels(new Uint8Array([255, 0, 0, 0, 0, 255]), 3)
    expect([px.width, px.height]).toEqual([2, 3])
    const at = (x: number, y: number) => Array.from(px.data.slice((y * px.width + x) * 4, (y * px.width + x) * 4 + 3))
    expect(at(0, 2)).toEqual(turbo(1))
    expect(at(0, 0)).toEqual(turbo(0))
    expect(at(1, 0)).toEqual(turbo(1))
    expect(px.data[3]).toBe(255)
  })

  it('a buffer shorter than a frame is empty', () => {
    expect(spectrogramPixels(new Uint8Array(2), 3).width).toBe(0)
  })
})
