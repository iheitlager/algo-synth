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

// Writing back into the file (#465): the File System Access API gives a handle
// to the file picked, where the browser has it (Chromium). Elsewhere there is
// no handle, and Save downloads as Save song always did.

/** A file Save can write into: what the API's handle offers, and all this needs. */
export type SongHandle = {
  name: string
  createWritable(): Promise<{ write(text: string): Promise<void>; close(): Promise<void> }>
}

/** The API's pickers, where the browser has them. */
export type Pickers = {
  showOpenFilePicker?: (o: object) => Promise<(SongHandle & { kind?: string; getFile(): Promise<File> })[]>
  showSaveFilePicker?: (o: object) => Promise<SongHandle>
}

/** What Open… takes: a MIDI file, a setup or a song. */
const OPEN_TYPES = [{ description: 'MIDI file, setup or song', accept: { 'application/octet-stream': ['.mid', '.midi', '.json', '.song'] } }]
const SONG_TYPES = [{ description: 'Song', accept: { 'text/plain': ['.song'] } }]

const cancelled = (e: unknown) => e instanceof DOMException && e.name === 'AbortError'

/** Whether the browser can open and save files in place. */
export const canPick = (win: Pickers = globalThis as Pickers) =>
  typeof win.showOpenFilePicker === 'function' && typeof win.showSaveFilePicker === 'function'

/** Pick files to open, each with its handle; [] when the user cancels. */
export async function pickOpen(win: Pickers = globalThis as Pickers): Promise<{ file: File; handle: SongHandle }[]> {
  if (!win.showOpenFilePicker) return []
  try {
    const handles = await win.showOpenFilePicker({ multiple: true, types: OPEN_TYPES })
    return Promise.all(handles.map(async (handle) => ({ file: await handle.getFile(), handle })))
  } catch (e) {
    if (cancelled(e)) return []
    throw e
  }
}

/** Pick a new file to save the song into: its handle, null where the browser can't (download instead), or 'cancel'. */
export async function pickSave(suggested: string, win: Pickers = globalThis as Pickers): Promise<SongHandle | null | 'cancel'> {
  if (!win.showSaveFilePicker) return null
  try {
    return await win.showSaveFilePicker({ suggestedName: suggested, types: SONG_TYPES })
  } catch (e) {
    if (cancelled(e)) return 'cancel'
    throw e
  }
}

/** Write `text` into the file, replacing what it held. The first write may ask for permission. */
export async function writeSong(handle: SongHandle, text: string) {
  const out = await handle.createWritable()
  await out.write(text)
  await out.close()
}
