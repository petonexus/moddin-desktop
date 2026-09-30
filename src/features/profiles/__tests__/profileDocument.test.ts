import { describe, expect, it } from 'vitest'
import type { CapabilitySpec, CapabilitySummary } from '../../../types/capability'
import {
  PROFILE_KIND,
  PROFILE_SCHEMA_VERSION,
  buildProfileDocument,
  engineSlug,
  looksLikeLocalPath,
  parseProfileDocument,
  planProfileImport,
  screenConfigValues,
  applicableEntries,
  type ModdinProfileDocument,
  type ProfileImportContext,
} from '../types'

/**
 * The profile document is a file that leaves this machine and comes
 * back from an inbox, so the three things under test are the ones that
 * would hurt most if they were wrong: a file that cannot be read back
 * the way it was written, a version that gets misread instead of
 * refused, and a capability that gets installed when it should have
 * been reported.
 */

function summary(overrides: Partial<CapabilitySummary> = {}): CapabilitySummary {
  return {
    id: 'reshade',
    displayName: 'ReShade',
    category: 'graphics',
    status: 'available',
    origin: 'builtIn',
    supportedEngines: [],
    engineMatch: { verdict: 'engineAgnostic' },
    ...overrides,
  }
}

function spec(overrides: Partial<CapabilitySpec> = {}): CapabilitySpec {
  return {
    id: 'reshade',
    displayName: 'ReShade',
    category: 'graphics',
    status: 'available',
    // A real recipe declares the fields it reads, which is what the
    // secrets rule keys off. A spec with no `configSchema` withholds
    // every value, and says so.
    configSchema: [
      { name: 'preset', type: 'string' },
      { name: 'intensity', type: 'number' },
      { name: 'modDirectory', type: 'path' },
    ],
    ...overrides,
  }
}

function context(overrides: Partial<ProfileImportContext> = {}): ProfileImportContext {
  return {
    capabilities: new Map([['reshade', summary()]]),
    specs: new Map([['reshade', spec()]]),
    installedByGame: new Map(),
    targets: new Map([
      ['elden-ring', {
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        installDir: 'C:\\games\\elden-ring',
        executableDir: 'C:\\games\\elden-ring\\Game',
        engine: 'Unreal Engine',
        engineVersion: 'UE5',
      }],
    ]),
    revoked: new Map(),
    ...overrides,
  }
}

function documentWith(
  games: ModdinProfileDocument['games'],
  overrides: Partial<ModdinProfileDocument> = {},
): ModdinProfileDocument {
  return {
    kind: PROFILE_KIND,
    schemaVersion: PROFILE_SCHEMA_VERSION,
    appVersion: '0.1.0',
    exportedAt: '2026-01-31T10:00:00.000Z',
    games,
    ...overrides,
  }
}

describe('profile round trip', () => {
  it('reads back exactly what an export wrote', () => {
    const built = buildProfileDocument({
      appVersion: '0.1.0',
      exportedAt: '2026-01-31T10:00:00.000Z',
      games: [
        {
          gameId: 'elden-ring',
          gameName: 'Elden Ring',
          capabilities: [
            {
              id: 'reshade',
              displayName: 'ReShade',
              spec: spec({
                configSchema: [
                  { name: 'preset', type: 'string' },
                  { name: 'intensity', type: 'number' },
                  { name: 'enabled', type: 'boolean' },
                  { name: 'downloadUrl', type: 'url' },
                ],
              }),
              values: {
                preset: 'ultra',
                intensity: 0.8,
                enabled: true,
                downloadUrl: 'https://example.invalid/reshade.zip',
              },
            },
          ],
        },
      ],
    })

    // Through the wire, not straight from the builder.
    const parsed = parseProfileDocument(`${JSON.stringify(built, null, 2)}\n`)

    expect(parsed.ok).toBe(true)
    if (!parsed.ok) return
    expect(parsed.document).toEqual(built)
    // And a second export of the parsed document is byte-identical, so
    // exporting what was imported cannot drift.
    expect(JSON.stringify(parsed.document)).toBe(JSON.stringify(built))
  })

  it('carries the schema version so the file can still be read later', () => {
    const built = buildProfileDocument({
      appVersion: '0.1.0',
      exportedAt: '2026-01-31T10:00:00.000Z',
      games: [],
    })
    expect(built.schemaVersion).toBe(PROFILE_SCHEMA_VERSION)
    expect(built.kind).toBe('moddin-profile')
  })
})

describe('profile schema versions', () => {
  const base = documentWith([])

  it('refuses a newer format with the version named, instead of misreading it', () => {
    const parsed = parseProfileDocument(
      JSON.stringify({ ...base, schemaVersion: PROFILE_SCHEMA_VERSION + 1 }),
    )

    expect(parsed).toEqual({
      ok: false,
      code: 'future-version',
      detail: String(PROFILE_SCHEMA_VERSION + 1),
    })
  })

  it('refuses an older format the build no longer understands', () => {
    // A schema below the supported floor. There is none today, so this
    // pins the refusal rather than the number.
    const parsed = parseProfileDocument(
      JSON.stringify({ ...base, schemaVersion: 0 }),
    )
    expect(parsed.ok).toBe(false)
    if (parsed.ok) return
    expect(parsed.code).toBe('old-version')
  })

  it('refuses a file that is not a profile at all', () => {
    expect(parseProfileDocument('not json at all')).toMatchObject({ ok: false, code: 'not-json' })
    expect(parseProfileDocument('[]')).toMatchObject({ ok: false, code: 'not-profile' })
    expect(parseProfileDocument('{"kind":"something-else"}')).toMatchObject({
      ok: false,
      code: 'not-profile',
    })
  })

  it('refuses a well-formed document that is missing a field', () => {
    const parsed = parseProfileDocument(
      JSON.stringify({ ...base, games: [{ gameId: 'elden-ring' }] }),
    )
    expect(parsed).toMatchObject({ ok: false, code: 'invalid' })
  })
})

describe('the secrets rule', () => {
  it('never writes a field typed as a path', () => {
    const screened = screenConfigValues(
      spec({
        configSchema: [
          { name: 'preset', type: 'string' },
          { name: 'modDirectory', type: 'path' },
        ],
      }),
      { preset: 'ultra', modDirectory: 'C:\\Users\\marco\\Mods' },
    )

    expect(screened.values).toEqual({ preset: 'ultra' })
    expect(screened.omitted).toEqual(['modDirectory'])
  })

  it('never writes a field the recipe does not declare', () => {
    const screened = screenConfigValues(
      spec({ configSchema: [{ name: 'preset', type: 'string' }] }),
      { preset: 'ultra', mysteryField: 'anything' },
    )

    expect(screened.values).toEqual({ preset: 'ultra' })
    expect(screened.omitted).toEqual(['mysteryField'])
  })

  it('withholds a path hidden in a string field', () => {
    const screened = screenConfigValues(
      spec({ configSchema: [{ name: 'target', type: 'string' }] }),
      { target: 'D:\\Games\\Mods' },
    )

    expect(screened.values).toEqual({})
    expect(screened.omitted).toEqual(['target'])
  })

  it('leaves a url alone: it is a public value, not a location', () => {
    const screened = screenConfigValues(
      spec({ configSchema: [{ name: 'downloadUrl', type: 'url' }] }),
      { downloadUrl: 'https://example.invalid/x.zip' },
    )

    expect(screened.values).toEqual({ downloadUrl: 'https://example.invalid/x.zip' })
    expect(screened.omitted).toEqual([])
  })

  it('recognises the shapes that name a place on this machine', () => {
    expect(looksLikeLocalPath('C:\\Users\\marco\\Mods')).toBe(true)
    expect(looksLikeLocalPath('\\\\NAS\\mods')).toBe(true)
    expect(looksLikeLocalPath('~/mods')).toBe(true)
    expect(looksLikeLocalPath('E:\\Games\\Mods')).toBe(true)
    expect(looksLikeLocalPath('ultra')).toBe(false)
    expect(looksLikeLocalPath('https://example.invalid/x.zip')).toBe(false)
  })

  it('names what it withheld in the file it writes', () => {
    const built = buildProfileDocument({
      appVersion: '0.1.0',
      exportedAt: '2026-01-31T10:00:00.000Z',
      games: [{
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        capabilities: [{
          id: 'bepinex',
          displayName: 'BepInEx',
          spec: spec({
            id: 'bepinex',
            configSchema: [
              { name: 'target', type: 'string' },
              { name: 'modDirectory', type: 'path' },
            ],
          }),
          values: { target: 'C:\\Users\\marco\\Mods', modDirectory: 'D:\\Mods' },
        }],
      }],
    })

    expect(built.games[0].capabilities[0].config.values).toEqual({})
    expect(built.games[0].capabilities[0].omittedSecrets).toEqual(['target', 'modDirectory'])
  })

  it('re-runs the rule on import, so a hand-edited file cannot widen it', () => {
    const document = documentWith([
      {
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        capabilities: [{
          id: 'reshade',
          displayName: 'ReShade',
          // A file someone edited by hand to smuggle a folder through.
          config: { values: { modDirectory: 'C:\\Users\\marco\\Mods', preset: 'ultra' } },
          omittedSecrets: [],
        }],
      },
    ], {})

    const plan = planProfileImport(document, context())

    expect(plan.games[0].capabilities[0].config).toEqual({ preset: 'ultra' })
    expect(plan.games[0].capabilities[0].omittedSecrets).toEqual(['modDirectory'])
  })

  it('withholds every value when the recipe declares no fields', () => {
    const plan = planProfileImport(
      documentWith([{
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        capabilities: [{
          id: 'reshade',
          displayName: 'ReShade',
          config: { values: { preset: 'ultra' } },
          omittedSecrets: [],
        }],
      }]),
      context({ specs: new Map([['reshade', spec({ configSchema: undefined })]]) }),
    )

    expect(plan.games[0].capabilities[0].config).toEqual({})
    expect(plan.games[0].capabilities[0].omittedSecrets).toEqual(['preset'])
  })
})

describe('previewing an import', () => {
  function planFor(entries: ModdinProfileDocument['games'][number]['capabilities'], ctx = context()) {
    return planProfileImport(
      documentWith([{ gameId: 'elden-ring', gameName: 'Elden Ring', capabilities: entries }]),
      ctx,
    )
  }

  it('reports a capability this build does not have, and does not offer to install it', () => {
    const plan = planFor([
      {
        id: 'mod-that-was-removed',
        displayName: 'Mod That Was Removed',
        config: { values: {} },
        omittedSecrets: [],
      },
    ])

    const entry = plan.games[0].capabilities[0]
    expect(entry.blocked).toBe(true)
    expect(entry.reasons).toContain('unknown-capability')
    expect(plan.applyCount).toBe(0)
    expect(applicableEntries(plan)).toEqual([])
  })

  it('reports a revoked capability instead of installing it', () => {
    const ctx = context({
      capabilities: new Map([['fps-unlocker', summary({ id: 'fps-unlocker', origin: 'community' })]]),
      revoked: new Map([['fps-unlocker', 'withdrawn by the author']]),
    })
    const plan = planFor(
      [{
        id: 'fps-unlocker',
        displayName: 'FPS Unlocker',
        config: { values: {} },
        omittedSecrets: [],
      }],
      ctx,
    )

    const entry = plan.games[0].capabilities[0]
    expect(entry.reasons).toContain('community-capability')
    expect(entry.reasons).toContain('revoked')
    expect(plan.applyCount).toBe(0)
  })

  it('blocks a game this PC does not have', () => {
    const plan = planProfileImport(
      documentWith([{
        gameId: 'cyberpunk-2077',
        gameName: 'Cyberpunk 2077',
        capabilities: [{
          id: 'reshade',
          displayName: 'ReShade',
          config: { values: {} },
          omittedSecrets: [],
        }],
      }]),
      context(),
    )

    expect(plan.games[0].installed).toBe(false)
    expect(plan.games[0].capabilities[0].reasons).toContain('game-not-installed')
    expect(plan.applyCount).toBe(0)
  })

  it('blocks a capability whose engine the game does not run', () => {
    const plan = planFor([{
      id: 'reshade',
      displayName: 'ReShade',
      config: { values: {} },
      omittedSecrets: [],
    }], context({
      specs: new Map([['reshade', spec({ supportedEngines: ['unity'] })]]),
    }))

    expect(plan.games[0].capabilities[0].reasons).toContain('engine-mismatch')
  })

  it('blocks when the engine could not be detected at all', () => {
    const plan = planFor([{
      id: 'reshade',
      displayName: 'ReShade',
      config: { values: {} },
      omittedSecrets: [],
    }], context({
      specs: new Map([['reshade', spec({ supportedEngines: ['unity'] })]]),
      targets: new Map([['elden-ring', {
        gameId: 'elden-ring',
        gameName: 'Elden Ring',
        installDir: 'C:\\games\\elden-ring',
        executableDir: 'C:\\games\\elden-ring\\Game',
        engine: null,
        engineVersion: null,
      }]]),
    }))

    expect(plan.games[0].capabilities[0].reasons).toContain('engine-unknown')
  })

  it('offers an install for a capability that is known and fits', () => {
    const plan = planFor([{
      id: 'reshade',
      displayName: 'ReShade',
      config: { values: { preset: 'ultra' } },
      omittedSecrets: [],
    }])

    expect(plan.applyCount).toBe(1)
    expect(plan.games[0].capabilities[0]).toMatchObject({
      action: 'install',
      blocked: false,
      config: { preset: 'ultra' },
    })
  })

  it('calls a repeat a reinstall, because the transaction log already has it', () => {
    const plan = planFor([{
      id: 'reshade',
      displayName: 'ReShade',
      config: { values: {} },
      omittedSecrets: [],
    }], context({
      installedByGame: new Map([['elden-ring', new Set(['reshade'])]]),
    }))

    expect(plan.games[0].capabilities[0].action).toBe('reinstall')
  })

  it('applies the rest of the file when one capability is blocked', () => {
    const plan = planFor([
      {
        id: 'mod-that-was-removed',
        displayName: 'Mod That Was Removed',
        config: { values: {} },
        omittedSecrets: [],
      },
      {
        id: 'reshade',
        displayName: 'ReShade',
        config: { values: {} },
        omittedSecrets: [],
      },
    ])

    expect(plan.blockedCount).toBe(1)
    expect(plan.applyCount).toBe(1)
    expect(applicableEntries(plan).map((entry) => entry.capability.id)).toEqual(['reshade'])
  })
})

describe('engine slugs', () => {
  it('maps what the inspector reports onto what a recipe declares', () => {
    expect(engineSlug('Unity', null)).toBe('unity')
    expect(engineSlug('Unreal Engine', 'UE5')).toBe('unreal5')
    expect(engineSlug('RE Engine', null)).toBe('re-engine')
    expect(engineSlug('REDengine', null)).toBe('redengine')
  })

  it('never guesses: an Unreal 4 game is not an Unreal 5 one', () => {
    // No shipped recipe declares `unreal4`, and rounding UE4 up to
    // `unreal5` is exactly how a mod lands in an engine it was never
    // written for.
    expect(engineSlug('Unreal Engine', 'UE4')).toBeNull()
    expect(engineSlug('Unreal Engine', null)).toBeNull()
  })

  it('has no slug for an engine it does not know', () => {
    expect(engineSlug(null, null)).toBeNull()
    expect(engineSlug('Frostbite', null)).toBeNull()
  })
})
