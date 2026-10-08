import { describe, expect, it, vi } from 'vitest'
import { AssistError, SseParser, assist, health, providers, toEvent, type AssistEvent } from './assist'

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })

/** A streamed response that sends `chunks` one by one. */
function stream(chunks: string[]) {
  const enc = new TextEncoder()
  return new Response(new ReadableStream({
    start(c) {
      for (const x of chunks) c.enqueue(enc.encode(x))
      c.close()
    },
  }), { status: 200, headers: { 'Content-Type': 'text/event-stream' } })
}

describe('the SSE parser', () => {
  it('joins an event split mid-line across chunks', () => {
    const p = new SseParser()
    expect(p.push('event: prog')).toEqual([])
    expect(p.push('ress\ndata: {"round":1,')).toEqual([])
    expect(p.push('"message":"Writing"}\n')).toEqual([])
    expect(p.push('\n')).toEqual([{ event: 'progress', data: '{"round":1,"message":"Writing"}' }])
  })

  it('reads several events in one chunk, CRLF too, and skips comments', () => {
    const p = new SseParser()
    const got = p.push(': keep-alive\n\nevent: text\ndata: {"text":"a"}\n\nevent: text\r\ndata:{"text":"b"}\r\n\r\nevent: done\ndata: {}\n\n')
    expect(got).toEqual([
      { event: 'text', data: '{"text":"a"}' },
      { event: 'text', data: '{"text":"b"}' },
      { event: 'done', data: '{}' },
    ])
  })

  it('holds a CR at the end of a chunk until it knows whether LF follows', () => {
    const p = new SseParser()
    expect(p.push('event: text\r')).toEqual([])
    expect(p.push('\ndata: {"text":"x"}\r\n\r\n')).toEqual([{ event: 'text', data: '{"text":"x"}' }])
  })

  it('joins data lines with newlines and names an unnamed event message', () => {
    expect(new SseParser().push('data: a\ndata: b\n\n')).toEqual([{ event: 'message', data: 'a\nb' }])
  })
})

describe('events', () => {
  it('are typed from their name and data', () => {
    expect(toEvent('tool', '{"round":2,"name":"check_song","ok":true,"summary":"parses"}'))
      .toEqual({ type: 'tool', round: 2, name: 'check_song', ok: true, summary: 'parses' })
    expect(toEvent('done', '{"rounds":3,"seconds":12.4,"usage":{"input":9000,"cached":7000,"output":1500}}'))
      .toEqual({ type: 'done', rounds: 3, seconds: 12.4, usage: { input: 9000, cached: 7000, output: 1500 } })
    expect(toEvent('song', '{"song":"tempo 90\\n","summary":"slower"}')).toEqual({ type: 'song', song: 'tempo 90\n', summary: 'slower' })
  })

  it('unknown or broken are ignored', () => {
    expect(toEvent('heartbeat', '{}')).toBeNull()
    expect(toEvent('text', 'not json')).toBeNull()
    expect(toEvent('song', '{"summary":"no song"}')).toBeNull()
  })
})

describe('the API client', () => {
  it('finds the server healthy only on 200 {"ok": true}', async () => {
    expect(await health(async () => json({ ok: true }))).toBe(true)
    expect(await health(async () => json({ ok: true }, 502))).toBe(false)
    expect(await health(async () => new Response('<html>', { status: 200 }))).toBe(false)
    expect(await health(async () => { throw new TypeError('refused') })).toBe(false)
  })

  it('lists the providers and the default', async () => {
    const p = await providers(async () => json({
      providers: [{ id: 'anthropic', name: 'Anthropic', models: [{ id: 'claude-opus-5-5', default: true }, { id: 'claude-sonnet-5-5', default: false }] }],
      default: { provider: 'anthropic', model: 'claude-opus-5-5' },
    }))
    expect(p.providers[0]?.models.map((m) => m.id)).toEqual(['claude-opus-5-5', 'claude-sonnet-5-5'])
    expect(p.default).toEqual({ provider: 'anthropic', model: 'claude-opus-5-5' })
    expect(await providers(async () => json({ providers: [], default: null }))).toEqual({ providers: [], default: null })
  })

  it('posts the request and hands out the stream as events', async () => {
    const fetcher = vi.fn(async (_url: string | URL | Request, _init?: RequestInit) => stream([
      'event: progress\ndata: {"round":1,"message":"Writing the song"}\n\nevent: to',
      'ol\ndata: {"round":1,"name":"check_song","ok":false,"summary":"line 3"}\n\nevent: ping\ndata: {}\n\n',
      'event: song\ndata: {"song":"tempo 90","summary":"slower"}\n\nevent: done\ndata: {"rounds":1,"seconds":1,"usage":{"input":1,"cached":0,"output":2}}',
    ]))
    const got: AssistEvent[] = []
    const req = { song: 'tempo 120', request: 'slower', provider: 'anthropic', model: 'claude-opus-5-5', focus: null }
    await assist(req, (e) => got.push(e), { fetcher: fetcher as typeof fetch })
    expect(fetcher.mock.calls[0]?.[0]).toBe('/api/assist')
    expect(fetcher.mock.calls[0]?.[1]?.method).toBe('POST')
    expect(JSON.parse(String(fetcher.mock.calls[0]?.[1]?.body))).toEqual(req)
    // The last event has no blank line after it: the end of the stream completes it.
    expect(got.map((e) => e.type)).toEqual(['progress', 'tool', 'song', 'done'])
  })

  it('reports a refusal before the stream with its status and message', async () => {
    for (const status of [400, 429, 503]) {
      const err = await assist(
        { song: '', request: 'x', provider: 'p', model: 'm', focus: null }, () => {},
        { fetcher: async () => json({ error: `no ${status}` }, status) },
      ).catch((e: unknown) => e)
      expect(err).toBeInstanceOf(AssistError)
      expect((err as AssistError).status).toBe(status)
      expect((err as AssistError).message).toBe(`no ${status}`)
    }
  })

  it('stops when aborted', async () => {
    const ctl = new AbortController()
    const fetcher = (async (_u: string, init?: RequestInit) => {
      if (init?.signal?.aborted) throw new DOMException('aborted', 'AbortError')
      return stream([])
    }) as typeof fetch
    ctl.abort()
    await expect(assist({ song: '', request: 'x', provider: 'p', model: 'm', focus: null }, () => {}, { signal: ctl.signal, fetcher }))
      .rejects.toThrow('aborted')
  })
})
