// Which build is running (#197): the page's version and commit are baked in by vite.config.ts,
// the engine's come out of dsp.wasm. Display and comparison only; nothing musical.

export type Build = { version: string; build: string }

/** The page's own build. `build` is `dev` when the build did not pass a commit. */
export const page: Build & { built: string } = {
  version: __APP_VERSION__,
  build: __APP_BUILD__,
  built: __APP_BUILT__,
}

/** `0.30.0` from the engine's `version_code` (major·10000 + minor·100 + patch); '' for 0, an engine too old to say. */
export const versionOf = (code: number) =>
  code > 0 ? `${Math.floor(code / 10000)}.${Math.floor(code / 100) % 100}.${code % 100}` : ''

/** The commit from the engine's `build_id` (eight hex digits as a number); '' for 0, a build that did not say. */
export const buildOf = (id: number) => (id > 0 ? id.toString(16).padStart(8, '0') : '')

/** What is wrong when the engine and the page are not from the same build, or '' when they are. */
export function mismatch(engine: Build, view: Build = page): string {
  const hint = 'Hard-refresh, or rebuild with `podman build --no-cache` if it stays.'
  if (!engine.version) return `dsp.wasm is older than the page (v${view.version}). ${hint}`
  if (engine.version !== view.version) return `Page v${view.version} but engine v${engine.version}. ${hint}`
  const known = (b: string) => b !== '' && b !== 'dev'
  if (known(engine.build) && known(view.build) && !view.build.startsWith(engine.build) && !engine.build.startsWith(view.build)) {
    return `Page and engine are from different builds (${view.build} and ${engine.build}). ${hint}`
  }
  return ''
}

/** The details, as plain text to read or paste into a bug report. */
export function details(engine: Build): string {
  const at = (b: Build) => `v${b.version || '?'}${b.build && b.build !== 'dev' ? ` (${b.build})` : ''}`
  return `page ${at(page)}, built ${page.built}\nengine ${at(engine)}`
}
