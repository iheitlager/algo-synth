// The song as a file (#105, ADR-0015): `.song` text, downloaded and opened
// like a setup, and the last song kept in localStorage. A convenience only:
// storage can be unavailable, and the file is the real save.

export const SONG_KEY = 'algo-synth:song'

type Store = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>
const local = (): Store => localStorage

export const isSongFile = (name: string) => /\.song$/i.test(name)

/** `<stem>.song`, the stem without a MIDI or song extension. */
export const songFileName = (stem: string) => `${stem.replace(/\.(midi?|song)$/i, '')}.song`

/** Keep `text` as the last song; an empty song is not kept, so it never replaces one that failed to load. */
export function keepSong(text: string, store: () => Store = local) {
  if (!text) return
  try {
    store().setItem(SONG_KEY, text)
  } catch {
    // Private window or storage full: keep playing.
  }
}

/** Forget the kept song (New, #325), so the next start is a fresh one. */
export function forgetSong(store: () => Store = local) {
  try {
    store().removeItem(SONG_KEY)
  } catch {
    // Storage unavailable: nothing was kept.
  }
}

/** The last session's song, or '' without one. */
export function lastSong(store: () => Store = local): string {
  try {
    return store().getItem(SONG_KEY) ?? ''
  } catch {
    return ''
  }
}
