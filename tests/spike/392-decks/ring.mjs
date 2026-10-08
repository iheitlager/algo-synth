// Spike #392: a single-producer, single-consumer ring of stereo blocks in a
// SharedArrayBuffer. ctrl[0] = blocks written, ctrl[1] = blocks read. Each
// slot also records the block number it holds, to check decks stay aligned.
export const BLOCK = 128
export const SLOTS = 16

export function makeRing() {
  return { ctrl: new SharedArrayBuffer(8), audio: new SharedArrayBuffer(SLOTS * 2 * BLOCK * 4), seq: new SharedArrayBuffer(SLOTS * 4) }
}
export function view(r) {
  return { ctrl: new Int32Array(r.ctrl), audio: new Float32Array(r.audio), seq: new Int32Array(r.seq) }
}
