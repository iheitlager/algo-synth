<script setup lang="ts">
// The Assistant (#387, ADR-0028): ask the assist server, in words, to change
// the song. The server runs the model and checks its song with the engine; this
// pane only sends the request, draws the events as they stream in, and shows the
// proposed song as a diff against the current one. Apply hands it to the host,
// which loads it through the engine's parser (ADR-0001, ADR-0012). The same
// pane runs in the app and in its own window (`assistant.html`); the host is
// the app itself or the main window over a BroadcastChannel.
import { computed, nextTick, onMounted, reactive, ref, watch } from 'vue'
import { AssistError, assist, canClear, health, pendingProposals, providers as fetchProviders, type AssistEvent, type Provider, type Usage, type Warning } from '../audio/assist'
import type { AssistHost } from '../audio/assistlink'
import Markdown from './Markdown.vue'
import { changes, collapse, diffLines } from '../audio/linediff'

const props = defineProps<{ host: AssistHost; popout?: boolean }>()
const emit = defineEmits<{ popout: [] }>()

/** Requests kept in this session. */
const KEEP = 10

interface Step { kind: 'progress' | 'tool' | 'text' | 'error'; text: string; name?: string; ok?: boolean }
interface Turn {
  request: string
  model: string
  focus: string | null
  steps: Step[]
  proposal: { song: string; summary: string; warnings: Warning[] } | null
  /** The song before Apply, for Undo. */
  before: string
  state: 'running' | 'done' | 'stopped' | 'failed'
  result: 'pending' | 'applied' | 'discarded' | 'undone'
  done: { rounds: number; seconds: number; usage: Usage } | null
  note: string
}

const avail = ref<'checking' | 'up' | 'down'>('checking')
const list = ref<Provider[]>([])
const provider = ref('')
const model = ref('')
const request = ref('')
const focus = ref<string>('')
const turns = ref<Turn[]>([])
const log = ref<HTMLElement | null>(null)
let controller: AbortController | null = null
const running = computed(() => turns.value.some((t) => t.state === 'running'))
const models = computed(() => list.value.find((p) => p.id === provider.value)?.models ?? [])

async function check() {
  avail.value = 'checking'
  if (!(await health())) {
    avail.value = 'down'
    return
  }
  avail.value = 'up'
  try {
    const p = await fetchProviders()
    list.value = p.providers
    const first = p.providers[0]
    const d = p.default && p.providers.some((x) => x.id === p.default!.provider) ? p.default : null
    provider.value = d?.provider ?? first?.id ?? ''
    model.value = d?.model ?? pickModel(provider.value)
  } catch {
    avail.value = 'down'
  }
}
/** A provider's default model, else its first. */
function pickModel(id: string) {
  const ms = list.value.find((p) => p.id === id)?.models ?? []
  return (ms.find((m) => m.default) ?? ms[0])?.id ?? ''
}
function onProvider(id: string) {
  provider.value = id
  model.value = pickModel(id)
}

// The cued clip's track (#375) is the default focus; the user may pick
// another. With a track in focus the server refuses changes outside it (#415).
watch(() => props.host.link.focus, (f) => (focus.value = f ?? ''), { immediate: true })

function scrollDown() {
  void nextTick(() => log.value?.scrollTo({ top: log.value.scrollHeight }))
}

function take(turn: Turn, e: AssistEvent) {
  switch (e.type) {
    case 'progress': turn.steps.push({ kind: 'progress', text: e.message }); break
    case 'tool': turn.steps.push({ kind: 'tool', name: e.name, ok: e.ok, text: e.summary }); break
    // Each text event is a whole message of the model's (#385), a step of its own.
    case 'text': turn.steps.push({ kind: 'text', text: e.text }); break
    case 'song': turn.proposal = { song: e.song, summary: e.summary, warnings: e.warnings }; turn.result = 'pending'; break
    case 'error': turn.steps.push({ kind: 'error', text: e.message }); turn.state = 'failed'; break
    case 'done': turn.done = { rounds: e.rounds, seconds: e.seconds, usage: e.usage }; break
  }
  scrollDown()
}

async function send() {
  const text = request.value.trim()
  if (!text || running.value || !provider.value || !model.value) return
  const turn = reactive<Turn>({
    request: text, model: model.value, focus: focus.value || null, steps: [], proposal: null, before: '',
    state: 'running', result: 'pending', done: null, note: '',
  })
  turns.value = [...turns.value, turn].slice(-KEEP)
  request.value = ''
  scrollDown()
  controller = new AbortController()
  try {
    await assist(
      { song: props.host.link.song, request: text, provider: provider.value, model: model.value, focus: turn.focus },
      (e) => take(turn, e), { signal: controller.signal },
    )
    if (turn.state === 'running') turn.state = 'done'
  } catch (e) {
    if (controller.signal.aborted) turn.state = 'stopped'
    else {
      turn.state = 'failed'
      turn.steps.push({ kind: 'error', text: e instanceof AssistError ? e.message : String(e) })
    }
  } finally {
    controller = null
    scrollDown()
  }
}
const stop = () => controller?.abort()

async function apply(turn: Turn) {
  if (!turn.proposal) return
  const before = props.host.link.song
  if (await props.host.apply(turn.proposal.song)) {
    turn.before = before
    turn.result = 'applied'
    turn.note = ''
  } else turn.note = 'Could not apply: is audio on in the main window?'
}
async function undo(turn: Turn) {
  if (await props.host.apply(turn.before)) turn.result = 'undone'
  else turn.note = 'Could not undo: is audio on in the main window?'
}
const discard = (turn: Turn) => (turn.result = 'discarded')

// Start fresh (#451): the model keeps nothing between requests, so this only
// empties the conversation; a proposal not yet applied or discarded is asked about.
function clear() {
  if (!canClear(turns.value)) return
  const pending = pendingProposals(turns.value)
  if (pending && !window.confirm(pending === 1 ? 'Discard the pending proposal?' : `Discard ${pending} pending proposals?`)) return
  turns.value = []
}

// The diff of what Apply would do now, against the song as it is.
const diffOf = (turn: Turn) => diffLines(props.host.link.song, turn.proposal?.song ?? '')
const stats = (d: NonNullable<Turn['done']>) =>
  `${d.rounds} ${d.rounds === 1 ? 'round' : 'rounds'} · ${d.seconds.toFixed(1)} s · ${d.usage.input} in (${d.usage.cached} cached) / ${d.usage.output} out tokens`

function onKey(e: KeyboardEvent) {
  if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
    e.preventDefault()
    void send()
  }
}

onMounted(check)
</script>

<template>
  <section class="pane assistant">
    <div class="pane-head">
      <span>Assistant</span>
      <button class="small clear" title="Clear the conversation and start fresh" :disabled="!canClear(turns)" @click="clear">Clear</button>
      <button v-if="popout" class="small" title="Open the Assistant in its own window" @click="emit('popout')">Pop out ↗</button>
    </div>
    <p v-if="avail === 'checking'" class="note muted">Looking for the assist server…</p>
    <p v-else-if="avail === 'down'" class="note">
      The assistant needs the app's server: <code>make dev</code> or <code>make serve</code> — see README.
      <button class="small" @click="check">Check again</button>
    </p>
    <template v-else>
      <p v-if="!host.link.connected" class="note warn">The main window is closed: a song can't be applied. Open the app again and it reconnects.</p>
      <p v-else-if="!list.length" class="note">The server has no provider keys.</p>
      <div ref="log" class="log">
        <p v-if="!turns.length" class="muted">Ask for a change to the song: “a busier snare in the drop”, “a second arp in fives”.</p>
        <div v-for="(turn, k) in turns" :key="k" class="turn">
          <div class="request">
            <button class="reuse" title="Ask this again" @click="request = turn.request">{{ turn.request }}</button>
            <span class="muted">{{ turn.model }}<template v-if="turn.focus"> · on {{ turn.focus }}</template></span>
          </div>
          <div v-for="(s, i) in turn.steps" :key="i" class="step" :class="s.kind">
            <template v-if="s.kind === 'tool'"><span :class="s.ok ? 'ok' : 'bad'">{{ s.ok ? '✓' : '✗' }}</span> <b>{{ s.name }}</b> {{ s.text }}</template>
            <template v-else-if="s.kind === 'progress'">… {{ s.text }}</template>
            <Markdown v-else-if="s.kind === 'text'" :text="s.text" />
            <template v-else>{{ s.text }}</template>
          </div>
          <p v-if="turn.state === 'running'" class="muted step">working…</p>
          <p v-if="turn.state === 'stopped'" class="muted step">Stopped.</p>
          <div v-if="turn.proposal && turn.result !== 'discarded'" class="proposal">
            <Markdown v-if="turn.proposal.summary" class="summary" :text="turn.proposal.summary" />
            <ul v-if="turn.result === 'pending' && turn.proposal.warnings.length" class="warn warnings">
              <li v-for="(w, i) in turn.proposal.warnings" :key="i">{{ w.message }}</li>
            </ul>
            <template v-if="turn.result === 'pending'">
              <div class="diff-head">
                <span class="muted">{{ changes(diffOf(turn)).added }} added, {{ changes(diffOf(turn)).removed }} removed</span>
                <span class="actions">
                  <button class="primary" :disabled="!host.link.connected || !host.link.running" @click="apply(turn)">Apply</button>
                  <button @click="discard(turn)">Discard</button>
                </span>
              </div>
              <pre class="diff"><template v-for="(d, i) in collapse(diffOf(turn))" :key="i"><span v-if="d.op === '…'" class="skip">  ⋯ {{ d.count }} unchanged</span><span v-else :class="{ add: d.op === '+', del: d.op === '-' }">{{ d.op }} {{ d.text }}</span></template></pre>
              <p v-if="!host.link.running" class="muted">Power on to apply.</p>
            </template>
            <p v-else-if="turn.result === 'applied'" class="muted">Applied. <button class="small" @click="undo(turn)">Undo</button></p>
            <p v-else class="muted">Undone.</p>
            <p v-if="turn.note" class="warn">{{ turn.note }}</p>
          </div>
          <p v-if="turn.done" class="muted stats">{{ stats(turn.done) }}</p>
        </div>
      </div>
      <form class="ask" @submit.prevent="send" @keydown.stop>
        <div class="pickers">
          <select class="picker" aria-label="Provider" :value="provider" @change="onProvider(($event.target as HTMLSelectElement).value)">
            <option v-for="p in list" :key="p.id" :value="p.id">{{ p.name }}</option>
          </select>
          <select v-model="model" class="picker" aria-label="Model">
            <option v-for="m in models" :key="m.id" :value="m.id">{{ m.id }}</option>
          </select>
          <select v-model="focus" class="picker" aria-label="Focus" title="Change one instrument only, or the whole song">
            <option value="">Whole song</option>
            <option v-for="t in host.link.tracks" :key="t" :value="t">{{ t }}</option>
          </select>
        </div>
        <textarea v-model="request" rows="3" placeholder="What should change? Ctrl+Enter sends" aria-label="Request" @keydown="onKey" />
        <div class="buttons">
          <button v-if="running" type="button" @click="stop">■ Stop</button>
          <button type="submit" class="primary" :disabled="running || !request.trim() || !model">Send</button>
        </div>
      </form>
    </template>
  </section>
</template>

<style scoped>
.assistant { display: flex; flex-direction: column; overflow: hidden; }
.small { padding: 2px 8px; font-size: 11px; text-transform: none; letter-spacing: 0; }
.clear { margin-left: auto; }
.note { margin: 0; padding: 10px 12px; }
.note code { font-family: var(--font-mono); color: var(--accent); }
.warn { color: var(--accent); }
.warnings { margin: 4px 0; padding-left: 18px; }
.muted { color: var(--muted); }
.log { flex: 1; min-height: 0; overflow: auto; padding: 8px 12px; display: flex; flex-direction: column; gap: 14px; }
.log p { margin: 0; }
.turn { display: flex; flex-direction: column; gap: 4px; }
.request { display: flex; align-items: baseline; gap: 8px; flex-wrap: wrap; }
.reuse { text-align: left; white-space: pre-wrap; border-color: color-mix(in srgb, var(--accent) 40%, var(--line)); }
.step { font-size: 12px; white-space: pre-wrap; }
.step.progress { color: var(--muted); }
.step.tool { font-family: var(--font-mono); font-size: 11px; color: var(--muted); }
.step.tool b { color: var(--text); font-weight: 500; }
.step.error { color: #ff9a85; }
.ok { color: #7fd18b; }
.bad { color: #ff9a85; }
.proposal { border: 1px solid var(--line); border-radius: 4px; padding: 6px 8px; display: flex; flex-direction: column; gap: 6px; background: var(--panel-2); }
.summary { font-size: 12px; }
.diff-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.actions { display: flex; gap: 6px; }
.primary { border-color: var(--accent); color: var(--accent); }
.diff { margin: 0; max-height: 40vh; overflow: auto; font: 11px/1.45 var(--font-mono); background: var(--bg); border-radius: 3px; padding: 4px 0; }
.diff span { display: block; padding: 0 8px; white-space: pre; }
.diff .add { background: #1f3a24; color: #b9f0c2; }
.diff .del { background: #45201b; color: #ffc4b8; }
.diff .skip { color: var(--muted); font-style: italic; }
.stats { font-size: 11px; font-variant-numeric: tabular-nums; }
.ask { border-top: 1px solid var(--line); padding: 8px 12px; display: flex; flex-direction: column; gap: 6px; }
.pickers { display: flex; gap: 6px; flex-wrap: wrap; }
.pickers .picker { min-width: 0; flex: 1; }
textarea {
  font: 12px/1.4 var(--font-mono); color: var(--text); background: var(--bg); resize: vertical;
  border: 1px solid var(--line); border-radius: 4px; padding: 6px 8px;
}
textarea:focus { outline: none; border-color: var(--accent); }
.buttons { display: flex; justify-content: flex-end; gap: 6px; }
</style>
