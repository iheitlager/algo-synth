import { describe, expect, it } from 'vitest'
import { buildOf, details, mismatch, page, versionOf } from './buildinfo'

const view = { version: '0.30.0', build: 'abc12345' }

describe('which build is running (#197)', () => {
  it('reads the engine version and commit from their numbers', () => {
    expect(versionOf(300)).toBe('0.3.0')
    expect(versionOf(1230)).toBe('0.12.30')
    expect(versionOf(10203)).toBe('1.2.3')
    expect(versionOf(0)).toBe('')
    expect(buildOf(0xabc12345)).toBe('abc12345')
    expect(buildOf(0x07c6ffe4)).toBe('07c6ffe4')
    expect(buildOf(0)).toBe('')
  })

  it('says nothing when the engine and the page match', () => {
    expect(mismatch({ version: '0.30.0', build: 'abc12345' }, view)).toBe('')
  })

  it('does not compare commits when either build did not say', () => {
    expect(mismatch({ version: '0.30.0', build: '' }, view)).toBe('')
    expect(mismatch({ version: '0.30.0', build: 'abc12345' }, { version: '0.30.0', build: 'dev' })).toBe('')
  })

  it('flags an engine of another version, another commit, or one too old to say', () => {
    expect(mismatch({ version: '0.29.0', build: 'abc12345' }, view)).toMatch(/Page v0\.30\.0 but engine v0\.29\.0/)
    expect(mismatch({ version: '0.30.0', build: 'ffffffff' }, view)).toMatch(/different builds \(abc12345 and ffffffff\)/)
    expect(mismatch({ version: '', build: '' }, view)).toMatch(/older than the page/)
  })

  it('lists the page and the engine in the details', () => {
    const text = details({ version: '0.30.0', build: 'abc12345' })
    expect(text).toContain(`page v${page.version}`)
    expect(text).toContain('engine v0.30.0 (abc12345)')
    expect(details({ version: '', build: '' })).toContain('engine v?')
  })
})
