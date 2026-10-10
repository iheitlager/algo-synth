// The Assistant in its own window (#387) talks to the main window over one
// BroadcastChannel: the window says hello, the main window answers with the
// song and keeps it current while the window is open, and Apply in the window
// is a message, so only the main window, which owns the engine, loads a song.

import { reactive } from 'vue'

export const CHANNEL = 'algo-synth-assistant'

/** In the main window: whether the Assistant pane is shown, and whether a window of its own is open. */
export const assistant = reactive({ shown: false, popped: false })

/**
 * The track a request is about by default (#415, #430): in the synths view the
 * track on the selected synth, so an instrument without clips is in focus;
 * elsewhere the cued clip's track. Null means the whole song.
 */
export function defaultFocus(
  tracks: readonly { name: string; synth: number }[], clips: readonly { track: number }[], cued: number,
  main: string, selected: number,
): string | null {
  if (main === 'synths') return tracks.find((t) => t.synth === selected)?.name ?? null
  return tracks[clips[cued]?.track ?? -1]?.name ?? null
}

/** What the Assistant needs of the song: its text, the default focus (see `defaultFocus`) and the tracks to pick (#415). */
export interface LinkState {
  song: string
  focus: string | null
  tracks: string[]
  /** Whether the engine runs, so a song can be applied. */
  running: boolean
}

/** Where the Assistant gets the song and sends a song to apply: the app itself, or the main window over the channel. */
export interface AssistHost {
  /** The song as it is now; `connected` is false when nothing can apply (the main window closed). */
  readonly link: Readonly<LinkState & { connected: boolean }>
  /** Load `song` through the engine; false when it could not be sent. */
  apply(song: string): Promise<boolean>
}

export type ToMain = { t: 'hello' } | { t: 'apply'; id: number; song: string } | { t: 'bye' }
export type ToWindow = { t: 'state'; state: LinkState } | { t: 'applied'; id: number; ok: boolean } | { t: 'ready' } | { t: 'gone' }

/** The part of a BroadcastChannel used here, so a test can pass a fake. */
export interface Port {
  postMessage(msg: unknown): void
  onmessage: ((e: MessageEvent) => void) | null
  close(): void
}

/**
 * The main window's side: answers a window's hello with the state, applies
 * what it sends, and says when it goes. `popped` hears a window open and close.
 */
export function serveWindow(port: Port, opts: { state: () => LinkState; apply: (song: string) => boolean; popped: (open: boolean) => void }) {
  let open = false
  const send = (m: ToWindow) => port.postMessage(m)
  const push = () => { if (open) send({ t: 'state', state: opts.state() }) }
  port.onmessage = (e: MessageEvent) => {
    const m = e.data as ToMain
    if (m?.t === 'hello') {
      open = true
      opts.popped(true)
      push()
    } else if (m?.t === 'bye') {
      open = false
      opts.popped(false)
    } else if (m?.t === 'apply' && typeof m.song === 'string') {
      send({ t: 'applied', id: m.id, ok: opts.apply(m.song) })
    }
  }
  // A window opened before this page (a reload) says hello again.
  send({ t: 'ready' })
  return {
    /** Send the state again, when the song or the cue changed. */
    push,
    close() {
      send({ t: 'gone' })
      port.close()
    },
  }
}

/**
 * The window's side: hello on connect and whenever the main window comes
 * back; `onState` gets the song, or null when the main window is gone or did
 * not answer within `waitMs`.
 */
export function connectMain(port: Port, onState: (s: LinkState | null) => void, waitMs = 1500) {
  let next = 1
  const pending = new Map<number, (ok: boolean) => void>()
  let silent: ReturnType<typeof setTimeout> | undefined
  const hello = () => {
    port.postMessage({ t: 'hello' } satisfies ToMain)
    clearTimeout(silent)
    silent = setTimeout(() => onState(null), waitMs)
  }
  port.onmessage = (e: MessageEvent) => {
    const m = e.data as ToWindow
    if (m?.t === 'state') {
      clearTimeout(silent)
      onState(m.state)
    } else if (m?.t === 'applied') {
      pending.get(m.id)?.(m.ok)
      pending.delete(m.id)
    } else if (m?.t === 'gone') {
      onState(null)
    } else if (m?.t === 'ready') {
      hello()
    }
  }
  hello()
  return {
    apply(song: string): Promise<boolean> {
      const id = next++
      return new Promise((resolve) => {
        const timer = setTimeout(() => {
          pending.delete(id)
          resolve(false)
        }, waitMs)
        pending.set(id, (ok) => {
          clearTimeout(timer)
          resolve(ok)
        })
        port.postMessage({ t: 'apply', id, song } satisfies ToMain)
      })
    },
    close() {
      clearTimeout(silent)
      port.postMessage({ t: 'bye' } satisfies ToMain)
      port.close()
    },
  }
}
