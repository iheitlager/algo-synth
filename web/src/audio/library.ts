// The user's preset library (ADR-0014): kept in the browser's IndexedDB and
// exported or imported as `algo-synth.presets.json`. Where storage is not
// available (a private window, a blocked origin) it lives for the session
// and says so; nothing here throws at the view.
import { reactive } from 'vue'
import { cleanName } from './setup'
import { libraryText, merge, parseLibrary, type Kind, type PresetRegistry, type UserPreset } from './presets'

export const library = reactive({
  presets: [] as UserPreset[],
  /** False when the browser keeps nothing: the library ends with the session. */
  persistent: true,
  /** The last import's or storage's message, for the view. */
  notice: '',
})

/** What was copied, per kind: an unnamed preset that Paste applies (#152). */
export const clipboard = reactive<Partial<Record<Kind, UserPreset>>>({})

const DB = 'algo-synth'
const STORE = 'library'
const KEY = 'presets'

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    if (typeof indexedDB === 'undefined') return reject(new Error('no IndexedDB'))
    const req = indexedDB.open(DB, 1)
    req.onupgradeneeded = () => req.result.createObjectStore(STORE)
    req.onsuccess = () => resolve(req.result)
    req.onerror = () => reject(req.error)
  })
}

async function read(): Promise<string | undefined> {
  const d = await db()
  return new Promise((resolve, reject) => {
    const req = d.transaction(STORE).objectStore(STORE).get(KEY)
    req.onsuccess = () => resolve(req.result as string | undefined)
    req.onerror = () => reject(req.error)
  })
}

async function write(text: string): Promise<void> {
  const d = await db()
  return new Promise((resolve, reject) => {
    const tx = d.transaction(STORE, 'readwrite')
    tx.objectStore(STORE).put(text, KEY)
    tx.oncomplete = () => resolve()
    tx.onerror = () => reject(tx.error)
  })
}

let reg: PresetRegistry | null = null

/** Read the stored library once, at start. */
export async function loadLibrary(registry: PresetRegistry) {
  reg = registry
  try {
    const text = await read()
    if (!text) return
    const parsed = parseLibrary(text, registry)
    if (parsed.ok) {
      library.presets = parsed.library.presets
      if (parsed.warnings.length) library.notice = `Presets: ${parsed.warnings.join('; ')}`
    } else library.notice = `Presets: the stored library is ${parsed.error}; starting empty`
  } catch {
    library.persistent = false
  }
}

async function persist() {
  if (!library.persistent) return
  try {
    await write(libraryText(library.presets))
  } catch {
    library.persistent = false
  }
}

const same = (a: UserPreset, b: UserPreset) =>
  a.kind === b.kind && a.model === b.model && a.type === b.type && a.name === b.name

/** Store a preset: it replaces the one with its id, or the one of its kind with its name. */
export function savePreset(p: UserPreset) {
  const at = library.presets.findIndex((q) => q.id === p.id || same(q, p))
  if (at >= 0) library.presets[at] = { ...p, id: library.presets[at]?.id ?? p.id }
  else library.presets.push(p)
  void persist()
}

export function renamePreset(id: string, raw: string) {
  const p = library.presets.find((q) => q.id === id)
  const name = cleanName(raw)
  if (!p || !name) return
  p.name = name
  void persist()
}

export function deletePreset(id: string) {
  library.presets = library.presets.filter((q) => q.id !== id)
  void persist()
}

/** The presets of `kind`, for a model or an effect type when given, by name. */
export function presetsOf(kind: Kind, of: { model?: string; type?: string } = {}) {
  return library.presets
    .filter((p) => p.kind === kind && (of.model === undefined || p.model === of.model) && (of.type === undefined || p.type === of.type))
    .sort((a, b) => a.name.localeCompare(b.name))
}

/** Download the whole library. */
export function exportLibrary() {
  const link = document.createElement('a')
  link.href = URL.createObjectURL(new Blob([libraryText(library.presets)], { type: 'application/json' }))
  link.download = 'algo-synth.presets.json'
  link.click()
  setTimeout(() => URL.revokeObjectURL(link.href), 1000)
}

/** Add the presets of a library file; a taken name gets a number. */
export async function importLibrary(file: File) {
  if (!reg) return
  const parsed = parseLibrary(await file.text(), reg)
  if (!parsed.ok) {
    library.notice = `${file.name}: ${parsed.error}; nothing imported`
    return
  }
  library.presets = merge(library.presets, parsed.library.presets)
  library.notice = [`imported ${parsed.library.presets.length} presets from ${file.name}`, ...parsed.warnings].join('; ')
  void persist()
}
