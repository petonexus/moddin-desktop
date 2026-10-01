import { describe, expect, it } from 'vitest'
import {
  ACTION_IDS,
  AGENT_DETAIL_IDS,
  CAPABILITY_CHECK_IDS,
  CAPABILITY_IDS,
  OPENXR_WARNING_IDS,
  actionText,
  agentDetailText,
  capabilityCheckLabel,
  capabilityDescriptionKey,
  capabilityNameKey,
  openXrWarningText,
  type BackendTextKey,
} from '../backendIds'
import en from '../locales/en'
import es from '../locales/es'
import ptBR from '../locales/pt-BR'
import { shouldPersistAction } from '../../services/activity-log'

/**
 * ROADMAP UX-21: the boundary is a table, and the table's contract is
 * that it is exhaustive over the ids this repository's backend can emit.
 *
 * The first half of this file proves every declared id resolves. The
 * second half reads the sources those ids come from — the persistent
 * action list, `openxr.rs`, `inspect_agent`, the shipped recipes — and
 * fails when one of them is missing here. That is the difference between
 * a mapping and a suggestion: a new backend string has to be a red test
 * on this file before a pt-BR user can read it in English.
 *
 * The sources come in through `import.meta.glob` rather than `node:fs`:
 * the project ships no Node type declarations, and a glob also makes a
 * renamed or deleted recipe a build failure rather than a runtime
 * surprise in one test.
 */

const RUST_SOURCES = import.meta.glob(
  '../../../src-tauri/src/{openxr,ai_assistant_setup}.rs',
  { query: '?raw', import: 'default', eager: true },
) as Record<string, string>

const RECIPES = import.meta.glob('../../../src-tauri/capabilities/*.yaml', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

const ACTIVITY_LOG = import.meta.glob('../../services/activity-log.ts', {
  query: '?raw',
  import: 'default',
  eager: true,
}) as Record<string, string>

const LOCALES = { en, es, 'pt-BR': ptBR } as const

/** One Rust file by its name, so the test says which source it means. */
function rustSource(file: string): string {
  const entry = Object.entries(RUST_SOURCES).find(([path]) => path.endsWith(`/${file}.rs`))
  if (!entry) throw new Error(`src-tauri/src/${file}.rs was not found by the glob`)
  return entry[1]
}

/** Every locale has to answer for a key, in prose, not with the key. */
function expectResolved(key: BackendTextKey, source: string) {
  for (const [locale, messages] of Object.entries(LOCALES)) {
    const value = messages[key]
    expect(value, `${locale} is missing ${key} (from ${source})`).toBeTypeOf('string')
    expect(value.trim(), `${locale}.${key} is empty`).not.toBe('')
  }
  expect(en[key], `${key} is not translated, it is the id again`).not.toBe(source)
}

/** String literals of a Rust expression list, whitespace normalized. */
function rustLiterals(source: string, pattern: RegExp): string[] {
  return [...source.matchAll(pattern)].map((match) => match[1].replace(/\s+/g, ' ').trim())
}

describe('every declared backend id resolves to a locale key', () => {
  it('covers every action-log command', () => {
    for (const id of ACTION_IDS) {
      const resolved = actionText(id)
      expect(resolved.text).toBe(id)
      expect(resolved.key, `no key for action id "${id}"`).toBeDefined()
      expectResolved(resolved.key as BackendTextKey, id)
    }
  })

  it('covers every OpenXR warning', () => {
    for (const id of OPENXR_WARNING_IDS) {
      const resolved = openXrWarningText(id)
      expect(resolved.text).toBe(id)
      expect(resolved.key, `no key for OpenXR warning "${id}"`).toBeDefined()
      expectResolved(resolved.key as BackendTextKey, id)
    }
  })

  it('covers every Local AI detail, and carries the interpolated value', () => {
    for (const template of AGENT_DETAIL_IDS) {
      // `{}` is Rust's placeholder; a probe stands in for the path or
      // command the backend would have interpolated.
      const rendered = template.replaceAll('{}', 'C:\\tools\\claude.exe')
      const resolved = agentDetailText(rendered)
      expect(resolved.key, `no key for agent detail "${template}"`).toBeDefined()
      expectResolved(resolved.key as BackendTextKey, template)
      if (template.includes('{}')) {
        expect(resolved.params?.value).toBe('C:\\tools\\claude.exe')
      } else {
        expect(resolved.params).toBeUndefined()
      }
    }
  })

  it('covers every capability id and every check id the recipes declare', () => {
    for (const id of CAPABILITY_IDS) {
      const key = capabilityDescriptionKey(id)
      expect(key, `no description key for capability "${id}"`).toBeDefined()
      expectResolved(key as BackendTextKey, id)
    }
    for (const id of CAPABILITY_CHECK_IDS) {
      const key = capabilityCheckLabel(id)
      expect(key, `no label key for check id "${id}"`).toBeDefined()
      expectResolved(key as BackendTextKey, id)
    }
  })

  it('covers every capability name, in every locale', () => {
    // The card title is the one string on the card a pt-BR user reads
    // before anything else, and it was the last field of the capability
    // still coming straight out of the English recipe. Same contract as
    // the description: every shipped id, every locale, prose not the id.
    for (const id of CAPABILITY_IDS) {
      const key = capabilityNameKey(id)
      expect(key, `no name key for capability "${id}"`).toBeDefined()
      expectResolved(key as BackendTextKey, id)
    }
  })
})

describe('the tables are exhaustive over the sources the ids come from', () => {
  it('matches PERSISTENT_ACTION_COMMANDS in both directions', () => {
    const source = ACTIVITY_LOG[Object.keys(ACTIVITY_LOG)[0]]
    const block = source.slice(source.indexOf('PERSISTENT_ACTION_COMMANDS = new Set(['))
    const declared = [...block.slice(0, block.indexOf('])')).matchAll(/'([^']+)'/g)].map((m) => m[1])

    expect(declared.length).toBeGreaterThan(0)
    // Both directions: a label for something that is not logged, and a
    // logged command with no label, are each half a boundary.
    expect([...ACTION_IDS].sort()).toEqual([...declared].sort())
    for (const id of ACTION_IDS) expect(shouldPersistAction(id)).toBe(true)
  })

  it('matches every string openxr.rs pushes as a warning', () => {
    const warnings = rustLiterals(rustSource('openxr'), /warnings\.push\(\s*"([^"]+)"/g)

    expect(warnings.length).toBeGreaterThan(0)
    expect([...OPENXR_WARNING_IDS].sort()).toEqual([...warnings].sort())
  })

  it('matches every detail inspect_agent can write', () => {
    const source = rustSource('ai_assistant_setup')
    const body = source.slice(
      source.indexOf('fn inspect_agent'),
      source.indexOf('fn detect_moddin_entry'),
    )
    // `Some("…")` and `Some(format!("…"))` are the templates; the
    // `Some(msg)` read-failure arm passes a real I/O error through and
    // is deliberately not one of them.
    const templates = [
      ...rustLiterals(body, /Some\(\s*"([^"]+)"/g),
      ...rustLiterals(body, /Some\(\s*format!\(\s*"([^"]+)"/g),
    ]

    expect(templates.length).toBeGreaterThan(0)
    expect([...AGENT_DETAIL_IDS].sort()).toEqual([...templates].sort())
  })

  it('matches every id the shipped recipes declare', () => {
    const recipes = Object.values(RECIPES)
    expect(recipes.length).toBeGreaterThan(0)

    const capabilityIds = new Set<string>()
    const everyId = new Set<string>()
    for (const source of recipes) {
      for (const match of source.matchAll(/^\s*-?\s*id:\s*(\S+)\s*$/gm)) everyId.add(match[1])
      const top = source.match(/^id:\s*(\S+)\s*$/m)
      if (top) capabilityIds.add(top[1])
    }

    // The description table is the whole shipped set, not a subset of it.
    expect([...capabilityIds].sort()).toEqual([...CAPABILITY_IDS].sort())
    // The card title is translated from that same id set, read straight
    // off the recipes. `CAPABILITY_NAMES` is a `Record<CapabilityId, …>`,
    // so a row for an id no recipe declares is a typecheck failure rather
    // than a row nothing ever reads.
    for (const id of capabilityIds) {
      const nameKey = capabilityNameKey(id)
      expect(nameKey, `no name key for shipped capability "${id}"`).toBeDefined()
      expectResolved(nameKey as BackendTextKey, id)
    }
    // Every id a shipped recipe names — capability or check — is declared
    // in one of the two tables.
    for (const id of everyId) {
      expect(
        CAPABILITY_IDS.includes(id as (typeof CAPABILITY_IDS)[number])
        || capabilityCheckLabel(id) !== undefined,
        `recipe id "${id}" is not declared in the UX-21 tables`,
      ).toBe(true)
    }
  })
})

describe('the fallback for an id nobody declared', () => {
  // Written down in the header of `src/i18n/backendIds.ts`: an
  // undeclared id renders the backend's own text. Refusing to render a
  // card, a log row or a warning would be worse than English.
  it('keeps the action id and reports no key', () => {
    expect(actionText('capability_install_tomorrow')).toEqual({
      text: 'capability_install_tomorrow',
      key: undefined,
    })
  })

  it('keeps a warning sentence the backend has not written yet', () => {
    const sentence = 'A brand new OpenXR sentence nobody has declared.'
    expect(openXrWarningText(sentence)).toEqual({ text: sentence, key: undefined })
  })

  it('keeps an agent detail that is a real I/O error', () => {
    const ioError = 'EACCES: permission denied (os error 5)'
    expect(agentDetailText(ioError)).toEqual({ text: ioError, key: undefined })
  })

  it('keeps a community recipe\'s own check label and description', () => {
    expect(capabilityCheckLabel('community-authored-check')).toBeUndefined()
    expect(capabilityDescriptionKey('community-authored-mod')).toBeUndefined()
  })

  it('keeps a community recipe\'s own display name', () => {
    // A card with no title is a card the user cannot pick, and the name
    // on a community recipe is its author's copy — so the table declines
    // it rather than inventing a translation for it.
    expect(capabilityNameKey('community-authored-mod')).toBeUndefined()
  })

  it('does not resolve a prototype member as a translation', () => {
    expect(actionText('toString').key).toBeUndefined()
    expect(capabilityCheckLabel('constructor')).toBeUndefined()
    expect(capabilityNameKey('toString')).toBeUndefined()
  })
})
