import { describe, expect, it, vi } from 'vitest'
import { applySong, song } from './engine'
import { canPick, forgetSong, isSongFile, keepSong, lastSong, pickOpen, pickSave, SONG_KEY, songFileName, writeSong, type SongHandle } from './songfile'

const memory = () => {
  const items = new Map<string, string>()
  const store = {
    getItem: (k: string) => items.get(k) ?? null,
    setItem: (k: string, v: string) => void items.set(k, v),
    removeItem: (k: string) => void items.delete(k),
  }
  return () => store
}
const broken = () => {
  throw new Error('SecurityError')
}

describe('the song file (#105, spec 003 Req 5)', () => {
  it('is a .song file named after the MIDI file or the song it came from', () => {
    expect(isSongFile('groove.song')).toBe(true)
    expect(isSongFile('GROOVE.SONG')).toBe(true)
    expect(isSongFile('groove.mid')).toBe(false)
    expect(isSongFile('groove.synths.json')).toBe(false)
    expect(songFileName('canon.mid')).toBe('canon.song')
    expect(songFileName('canon.midi')).toBe('canon.song')
    expect(songFileName('groove.song')).toBe('groove.song')
    expect(songFileName('algo-synth')).toBe('algo-synth.song')
  })

  it('keeps the last song and gives it back', () => {
    const store = memory()
    expect(lastSong(store)).toBe('')
    keepSong('tempo 120\n', store)
    expect(lastSong(store)).toBe('tempo 120\n')
    expect(store().getItem(SONG_KEY)).toBe('tempo 120\n')
  })

  it('does not replace a kept song with an empty one', () => {
    const store = memory()
    keepSong('tempo 120\n', store)
    keepSong('', store)
    expect(lastSong(store)).toBe('tempo 120\n')
  })

  it('forgets the kept song, so the next start is fresh (#325)', () => {
    const store = memory()
    keepSong('tempo 120\n', store)
    forgetSong(store)
    expect(lastSong(store)).toBe('')
  })

  it('survives storage that is unavailable', () => {
    expect(() => keepSong('tempo 120\n', broken)).not.toThrow()
    expect(() => forgetSong(broken)).not.toThrow()
    expect(lastSong(broken)).toBe('')
  })

  it('a file that does not parse leaves the playing song and shows its text with the error', () => {
    const bytes = (s: string) => new TextEncoder().encode(s)
    const playing = 'tempo 120\nswing 50\n'
    applySong({ ok: true, text: bytes(playing), error: null, tracks: [], clips: [], tempo: 120, swing: 50 })
    song.draft = 'tempo nope\n'
    applySong({
      ok: false, text: bytes(playing), error: { line: 1, col: 7, msg: bytes('a tempo is a number') },
      tracks: [], clips: [], tempo: 120, swing: 50,
    })
    expect(song.text).toBe(playing)
    expect(song.draft).toBe('tempo nope\n')
    expect(song.error).toEqual({ line: 1, col: 7, msg: 'a tempo is a number' })
  })
})

/** A file the fake pickers hand out: what was written into it, and whether it was closed. */
const fakeFile = (name: string) => {
  const file = { name, text: '', closed: false }
  const handle: SongHandle & { kind: string; getFile(): Promise<File> } = {
    name,
    kind: 'file',
    getFile: async () => new File([file.text], name),
    createWritable: async () => ({
      write: async (t: string) => void (file.text = t),
      close: async () => void (file.closed = true),
    }),
  }
  return { file, handle }
}
const abort = () => Promise.reject(new DOMException('The user aborted a request.', 'AbortError'))

describe('saving in place (#465)', () => {
  it('needs both pickers', () => {
    expect(canPick({})).toBe(false)
    expect(canPick({ showOpenFilePicker: abort })).toBe(false)
    expect(canPick({ showOpenFilePicker: abort, showSaveFilePicker: abort })).toBe(true)
  })

  it('opens the files picked with their handles, and nothing when cancelled', async () => {
    const { handle } = fakeFile('groove.song')
    const opened = await pickOpen({ showOpenFilePicker: async () => [handle] })
    expect(opened.map((o) => [o.file.name, o.handle.name])).toEqual([['groove.song', 'groove.song']])
    expect(await pickOpen({ showOpenFilePicker: abort })).toEqual([])
    expect(await pickOpen({})).toEqual([])
  })

  it('picks a file to save into, suggesting the song\'s name; null without the API, cancel when cancelled', async () => {
    const { handle } = fakeFile('new.song')
    const pick = vi.fn(async () => handle)
    expect(await pickSave('groove.song', { showSaveFilePicker: pick })).toBe(handle)
    expect(pick).toHaveBeenCalledWith(expect.objectContaining({ suggestedName: 'groove.song' }))
    expect(await pickSave('groove.song', {})).toBeNull()
    expect(await pickSave('groove.song', { showSaveFilePicker: abort })).toBe('cancel')
    await expect(pickSave('x.song', { showSaveFilePicker: () => Promise.reject(new Error('denied')) })).rejects.toThrow('denied')
  })

  it('writes the text into the file and closes it', async () => {
    const { file, handle } = fakeFile('groove.song')
    await writeSong(handle, 'tempo 120\n')
    expect(file).toEqual({ name: 'groove.song', text: 'tempo 120\n', closed: true })
  })
})
