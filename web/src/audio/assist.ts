// The assist server's client (#387, ADR-0028): same origin, under /api. The
// server holds the keys and runs the model's loop; the view asks, reads the
// streamed events and draws them. Nothing here writes or judges a song: the
// engine parses whatever is applied (ADR-0001, ADR-0012).

export interface Model { id: string; default: boolean }
export interface Provider { id: string; name: string; models: Model[] }
export interface Providers { providers: Provider[]; default: { provider: string; model: string } | null }

export interface AssistRequest {
  song: string
  request: string
  provider: string
  model: string
  /** A fragment's name to work on, or null for the whole song. */
  focus: string | null
}

export interface Usage { input: number; cached: number; output: number }

/** One server-sent event, as the view uses it. */
export type AssistEvent =
  | { type: 'progress'; round: number; message: string }
  | { type: 'tool'; round: number; name: string; ok: boolean; summary: string }
  | { type: 'text'; text: string }
  | { type: 'song'; song: string; summary: string }
  | { type: 'error'; message: string; retryable: boolean }
  | { type: 'done'; rounds: number; seconds: number; usage: Usage }

/** The server refused before streaming (400, 429, 503, …), or could not be reached. */
export class AssistError extends Error {
  constructor(message: string, readonly status: number) {
    super(message)
  }
}

type Fetch = typeof fetch
const API = '/api'

/** Whether the assist server answers. */
export async function health(fetcher: Fetch = fetch): Promise<boolean> {
  try {
    const r = await fetcher(`${API}/health`)
    if (!r.ok) return false
    const body = (await r.json()) as { ok?: unknown }
    return body.ok === true
  } catch {
    return false
  }
}

const str = (v: unknown, or = '') => (typeof v === 'string' ? v : or)
const num = (v: unknown) => (typeof v === 'number' && Number.isFinite(v) ? v : 0)

/** The providers the server has keys for, each with its models. */
export async function providers(fetcher: Fetch = fetch): Promise<Providers> {
  const r = await fetcher(`${API}/providers`)
  if (!r.ok) throw new AssistError(await reason(r), r.status)
  const body = (await r.json()) as { providers?: unknown; default?: { provider?: unknown; model?: unknown } | null }
  const list = Array.isArray(body.providers) ? body.providers : []
  const out: Provider[] = list.map((p: { id?: unknown; name?: unknown; models?: unknown }) => ({
    id: str(p.id),
    name: str(p.name, str(p.id)),
    models: (Array.isArray(p.models) ? p.models : []).map((m: { id?: unknown; default?: unknown }) => ({ id: str(m.id), default: m.default === true })),
  }))
  const d = body.default
  return { providers: out, default: d && typeof d.provider === 'string' && typeof d.model === 'string' ? { provider: d.provider, model: d.model } : null }
}

/** Why a response is not 200: its `{"error": …}`, else its status. */
async function reason(r: Response): Promise<string> {
  try {
    const body = (await r.json()) as { error?: unknown }
    if (typeof body.error === 'string' && body.error) return body.error
  } catch {
    // Not JSON: say the status.
  }
  return `The server answered ${r.status}${r.statusText ? ` ${r.statusText}` : ''}`
}

/**
 * Server-sent events from text arriving in chunks: a chunk may end mid-line or
 * hold several events. `push` returns the events completed by the chunk.
 */
export class SseParser {
  private buffer = ''
  private event = ''
  private data: string[] = []

  push(chunk: string): { event: string; data: string }[] {
    this.buffer += chunk
    const out: { event: string; data: string }[] = []
    let nl: number
    while ((nl = this.buffer.search(/\r\n|\r|\n/)) >= 0) {
      // A \r at the very end may be half of \r\n: wait for the next chunk.
      if (this.buffer[nl] === '\r' && nl === this.buffer.length - 1) break
      const line = this.buffer.slice(0, nl)
      this.buffer = this.buffer.slice(nl + (this.buffer.startsWith('\r\n', nl) ? 2 : 1))
      if (line === '') {
        if (this.data.length) out.push({ event: this.event || 'message', data: this.data.join('\n') })
        this.event = ''
        this.data = []
        continue
      }
      if (line.startsWith(':')) continue
      const colon = line.indexOf(':')
      const field = colon < 0 ? line : line.slice(0, colon)
      let value = colon < 0 ? '' : line.slice(colon + 1)
      if (value.startsWith(' ')) value = value.slice(1)
      if (field === 'event') this.event = value
      else if (field === 'data') this.data.push(value)
    }
    return out
  }
}

/** The view's event for an SSE event, or null for an unknown name or bad data. */
export function toEvent(name: string, data: string): AssistEvent | null {
  let d: Record<string, unknown>
  try {
    const v: unknown = JSON.parse(data)
    if (!v || typeof v !== 'object') return null
    d = v as Record<string, unknown>
  } catch {
    return null
  }
  switch (name) {
    case 'progress': return { type: 'progress', round: num(d.round), message: str(d.message) }
    case 'tool': return { type: 'tool', round: num(d.round), name: str(d.name), ok: d.ok === true, summary: str(d.summary) }
    case 'text': return { type: 'text', text: str(d.text) }
    case 'song': return typeof d.song === 'string' ? { type: 'song', song: d.song, summary: str(d.summary) } : null
    case 'error': return { type: 'error', message: str(d.message, 'The assistant failed'), retryable: d.retryable === true }
    case 'done': {
      const u = (d.usage ?? {}) as Record<string, unknown>
      return { type: 'done', rounds: num(d.rounds), seconds: num(d.seconds), usage: { input: num(u.input), cached: num(u.cached), output: num(u.output) } }
    }
    default: return null
  }
}

/**
 * Ask the server to change the song: `onEvent` gets each event as it arrives.
 * Throws an `AssistError` when the server refuses before streaming; an abort
 * through `signal` rejects with the fetch's AbortError.
 */
export async function assist(
  req: AssistRequest, onEvent: (e: AssistEvent) => void, opts: { signal?: AbortSignal; fetcher?: Fetch } = {},
): Promise<void> {
  const fetcher = opts.fetcher ?? fetch
  let r: Response
  try {
    r = await fetcher(`${API}/assist`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', Accept: 'text/event-stream' },
      body: JSON.stringify(req),
      signal: opts.signal,
    })
  } catch (e) {
    if (opts.signal?.aborted) throw e
    throw new AssistError('The assist server did not answer', 0)
  }
  if (!r.ok) throw new AssistError(await reason(r), r.status)
  if (!r.body) return
  const reader = r.body.getReader()
  const decoder = new TextDecoder()
  const parser = new SseParser()
  for (;;) {
    const { done, value } = await reader.read()
    const text = done ? decoder.decode() + '\n\n' : decoder.decode(value, { stream: true })
    for (const raw of parser.push(text)) {
      const e = toEvent(raw.event, raw.data)
      if (e) onEvent(e)
    }
    if (done) return
  }
}
