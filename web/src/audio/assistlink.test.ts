// The main window and the Assistant's window over a fake BroadcastChannel: a
// message posted on one port reaches every other port of the same name, as
// in the browser, never the port that sent it.
import { afterEach, describe, expect, it, vi } from 'vitest'
import { connectMain, defaultFocus, serveWindow, type LinkState, type Port } from './assistlink'

class FakeChannel implements Port {
  static all: FakeChannel[] = []
  onmessage: ((e: MessageEvent) => void) | null = null
  closed = false
  sent: unknown[] = []
  constructor() { FakeChannel.all.push(this) }
  postMessage(msg: unknown) {
    this.sent.push(msg)
    for (const c of FakeChannel.all) if (c !== this && !c.closed) c.onmessage?.({ data: structuredClone(msg) } as MessageEvent)
  }
  close() { this.closed = true }
}

const STATE: LinkState = { song: 'tempo 120\n', focus: 'kit', tracks: ['kit', 'bass'], running: true }

function mainWindow(state = STATE) {
  const applied: string[] = []
  const popped: boolean[] = []
  const server = serveWindow(new FakeChannel(), {
    state: () => state,
    apply: (song) => { applied.push(song); return state.running },
    popped: (open) => popped.push(open),
  })
  return { server, applied, popped }
}

afterEach(() => {
  FakeChannel.all = []
  vi.useRealTimers()
})

describe('the Assistant window and the main window', () => {
  it('say hello and get the song, the cued clip and the clips', () => {
    const main = mainWindow()
    const got: (LinkState | null)[] = []
    connectMain(new FakeChannel(), (s) => got.push(s))
    expect(got).toEqual([STATE])
    expect(main.popped).toEqual([true])
  })

  it('apply a song in the main window only, and hear whether it went', async () => {
    const main = mainWindow()
    const link = connectMain(new FakeChannel(), () => {})
    await expect(link.apply('tempo 90\n')).resolves.toBe(true)
    expect(main.applied).toEqual(['tempo 90\n'])
  })

  it('keep the window current while it is open, and stop when it says bye', () => {
    const state = { ...STATE }
    const main = mainWindow(state)
    const got: (LinkState | null)[] = []
    const link = connectMain(new FakeChannel(), (s) => got.push(s))
    state.song = 'tempo 100\n'
    main.server.push()
    expect(got.at(-1)?.song).toBe('tempo 100\n')
    link.close()
    expect(main.popped).toEqual([true, false])
    const before = FakeChannel.all[0]!.sent.length
    main.server.push()
    expect(FakeChannel.all[0]!.sent.length).toBe(before)
  })

  it('tell the window when the main window goes, and reconnect when it comes back', () => {
    const main = mainWindow()
    const got: (LinkState | null)[] = []
    connectMain(new FakeChannel(), (s) => got.push(s))
    main.server.close()
    expect(got.at(-1)).toBeNull()
    mainWindow()
    expect(got.at(-1)).toEqual(STATE)
  })

  it('find the main window missing when nothing answers, and cannot apply', async () => {
    vi.useFakeTimers()
    const got: (LinkState | null)[] = []
    const link = connectMain(new FakeChannel(), (s) => got.push(s), 100)
    vi.advanceTimersByTime(100)
    expect(got).toEqual([null])
    const applied = link.apply('tempo 90\n')
    vi.advanceTimersByTime(100)
    await expect(applied).resolves.toBe(false)
  })

  it('ignore messages that are not theirs', () => {
    const main = mainWindow()
    const stray = new FakeChannel()
    stray.postMessage({ t: 'nonsense' })
    stray.postMessage(null)
    expect(main.applied).toEqual([])
    expect(main.popped).toEqual([])
  })
})

describe('the default focus', () => {
  const tracks = [{ name: 'kit', synth: 2 }, { name: 'lead', synth: 0 }]
  const clips = [{ track: 0 }]
  it('is the selected synth\'s track in the synths view, with or without clips', () => {
    expect(defaultFocus(tracks, clips, 0, 'synths', 0)).toBe('lead')
    expect(defaultFocus(tracks, [], -1, 'synths', 2)).toBe('kit')
    expect(defaultFocus(tracks, clips, 0, 'synths', 5)).toBeNull()
  })
  it('is the cued clip\'s track elsewhere, else the whole song', () => {
    expect(defaultFocus(tracks, clips, 0, 'composer', 0)).toBe('kit')
    expect(defaultFocus(tracks, clips, -1, 'mixer', 0)).toBeNull()
  })
})
