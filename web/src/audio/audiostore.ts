// Dropped sound files kept in the browser (ADR-0033), so a recipe that names
// one by its hash (`table 1 = @3fa9…`) can be rebuilt after a reload. Its own
// IndexedDB database, apart from the preset library (ADR-0014). Without
// IndexedDB the files live for the session only.

const DB = 'algo-synth-audio'
const STORE = 'wav'
/** Hex digits of the SHA-256 a recipe names. */
export const HASH_DIGITS = 32

/** The first `HASH_DIGITS` hex digits of the SHA-256 of `bytes`. */
export async function hashOf(bytes: ArrayBuffer): Promise<string> {
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))
  return Array.from(digest, (b) => b.toString(16).padStart(2, '0')).join('').slice(0, HASH_DIGITS)
}

let opened: Promise<IDBDatabase | null> | null = null
function open(): Promise<IDBDatabase | null> {
  opened ??= new Promise((resolve) => {
    try {
      const req = indexedDB.open(DB, 1)
      req.onupgradeneeded = () => {
        if (!req.result.objectStoreNames.contains(STORE)) req.result.createObjectStore(STORE)
      }
      req.onsuccess = () => resolve(req.result)
      req.onerror = () => resolve(null)
    } catch {
      resolve(null)
    }
  })
  return opened
}

/** Keep `bytes` under their hash, and return it; the hash even when nothing could be kept. */
export async function keepFile(bytes: ArrayBuffer): Promise<string> {
  const hash = await hashOf(bytes)
  const db = await open()
  if (db) {
    await new Promise<void>((resolve) => {
      const tx = db.transaction(STORE, 'readwrite')
      tx.objectStore(STORE).put(bytes.slice(0), hash)
      tx.oncomplete = () => resolve()
      tx.onerror = () => resolve()
    })
  }
  return hash
}

/** The file kept under `hash`, or null. */
export async function keptFile(hash: string): Promise<ArrayBuffer | null> {
  const db = await open()
  if (!db) return null
  return new Promise((resolve) => {
    const req = db.transaction(STORE, 'readonly').objectStore(STORE).get(hash)
    req.onsuccess = () => resolve(req.result instanceof ArrayBuffer ? req.result : null)
    req.onerror = () => resolve(null)
  })
}
