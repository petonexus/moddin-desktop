import { describe, expect, it } from 'vitest'
// The two things this test compares the table against — the backend's
// command list and the frontend's own services — are files on disk, and
// the project's tsconfig does not carry the Node type definitions. The
// vitest runtime has them; the type checker is told so at each import.
// @ts-ignore node builtins, present in the vitest runtime
import { readFileSync, readdirSync } from 'node:fs'
// @ts-ignore node builtins, present in the vitest runtime
import { join } from 'node:path'
import { gameCatalog } from '../../../services/catalog'
import type { ToolModuleDefinition } from '../../../types/game'
import type { ObsVrRequest } from '../../../types/obs'
import type { UevrRequest } from '../../../types/uevr'
import type { VrLaunchRequest } from '../../../types/vr-launch'
import {
  ALL_MODULE_ENTRIES,
  CAPABILITY_BACKED_MODULE_IDS,
  DELEGATED_MODULE_ENTRIES,
  LIBRARY_MODULE_ENTRIES,
  cheekyCompatibilityNote,
  isCapabilityBackedModule,
  libraryModuleEntry,
  libraryOwnedModules,
  moduleEntry,
  moduleStateKey,
  moduleTransactionKind,
  resolveModuleUpdateSource,
  type ModuleGame,
  type ModuleRequestContext,
} from '../module-registry'
import { VERIFICATION_PROBES } from '../verification-probes'

/**
 * The module registry against the two things that can make a card a lie:
 * the catalogue and the backend.
 *
 * `reshade` and `ofxr-framegen` both shipped as grid cards whose action
 * did nothing — a Rust file that nothing dispatched to, so every branch
 * of the old `configureModule` fell through to "no action". The byte
 * budget cannot see that class of defect, and neither can a type checker:
 * a card with no handler is still a perfectly well-typed card. What can
 * see it is the catalogue, held against the table, and the backend's
 * command list, held against the table. That is what this file is.
 *
 * Nothing here mounts a component or calls Tauri. The registry is a
 * mapping, and a mapping is checked by comparing it to the two lists it
 * claims to mirror.
 */

// @ts-ignore `process` is a Node global, absent from the tsconfig types
const root = process.cwd()
const projectFile = (relative: string) => join(root, relative)

/**
 * Every command the app registers, from the invoke_handler list. Same
 * source and same shape as `scripts/validate-catalog.mjs`, on purpose:
 * two parsers of the same list would be two answers.
 */
const registeredCommands = new Set(
  [...readFileSync(projectFile('src-tauri/src/lib.rs'), 'utf8').matchAll(
    /^[ \t]{12}(?:[a-z][a-z0-9_]*::)*([a-z][a-z0-9_]+),/gm,
  )].map((match) => match[1]),
)

/** Recipes the binary actually embeds, from its `include_str!` list. */
const registeredRecipes = new Set(
  [...readFileSync(projectFile('src-tauri/src/capability_runner.rs'), 'utf8').matchAll(
    /include_str!\("\.\.\/capabilities\/([^"]+)"\)/g,
  )].map((match) => match[1]),
)

/**
 * Every command a feature service actually invokes. A name in the table
 * that no service calls is a card that renders and does nothing, which
 * is the same defect one layer down.
 */
const commandsInvokedByServices = new Set(
  readdirSync(projectFile('src/features'))
    .map((feature: string) => projectFile(`src/features/${feature}/service.ts`))
    .flatMap((file: string) => {
      let source: string
      try {
        source = readFileSync(file, 'utf8')
      } catch {
        return [] // a feature folder without a service is not a failure
      }
      return [...source.matchAll(/invoke<[^>]*>\(\s*'([a-z_]+)'/g)].map((match) => match[1])
    }),
)

const declaredCommands = ALL_MODULE_ENTRIES.flatMap((entry) =>
  [entry.previewCommand, entry.installCommand, entry.uninstallCommand].filter(
    (command): command is string => typeof command === 'string',
  ),
)

/** Every module the catalog offers, engine presets merged in. */
const catalogModules = gameCatalog.flatMap((game) =>
  game.modules.map((module) => ({ gameId: game.id, module })),
)

/** The ones the library grid renders as actionable cards. */
const gridOwnedModules = catalogModules.filter(
  ({ module }) => module.status === 'available' && !isCapabilityBackedModule(module),
)

/**
 * A module as the catalogue spells it, from the game this file's fixture
 * is. Two games can declare the same id with different config, and a
 * request is always built from the selected game — so the fixture and the
 * recipe have to come from the same one.
 */
function moduleOf(id: string, overrides: Partial<ToolModuleDefinition> = {}): ToolModuleDefinition {
  const eldenRing = gameCatalog.find((entry) => entry.id === 'elden-ring')!
  const fromGame = eldenRing.modules.find((module) => module.id === id)
  const fromCatalog = fromGame ?? catalogModules.find((entry) => entry.module.id === id)?.module
  if (!fromCatalog) throw new Error(`The catalogue declares no module called ${id}`)
  return { ...fromCatalog, ...overrides }
}

const game: ModuleGame = {
  installed: {
    store: 'steam',
    appId: '1245620',
    name: 'ELDEN RING',
    installDir: 'C:\\Games\\ELDEN RING',
    libraryPath: 'C:\\Games\\steamapps',
  },
  catalog: gameCatalog.find((entry) => entry.id === 'elden-ring')!,
}

function contextFor(
  module: ToolModuleDefinition,
  overrides: Partial<ModuleRequestContext> = {},
): ModuleRequestContext {
  return {
    module,
    game,
    // A translator that echoes the key, so a failure names the sentence
    // that is missing rather than an interpolated blob.
    t: (key) => key,
    uevrBackend: () => 'nightly',
    ...overrides,
  }
}

describe('the catalogue against the module registry', () => {
  it('gives every module the grid renders a row in the table', () => {
    const missing = gridOwnedModules
      .filter(({ module }) => !moduleEntry(module.id))
      .map(({ gameId, module }) => `${gameId} → ${module.id}`)

    expect(missing).toEqual([])
  })

  it('keeps every row pointing at a module the catalogue declares', () => {
    const catalogIds = new Set(catalogModules.map(({ module }) => module.id))
    const unknown = ALL_MODULE_ENTRIES
      .map((entry) => entry.id)
      .filter((id) => !catalogIds.has(id))

    expect(unknown).toEqual([])
  })

  it('declares a transaction kind per row, and never two rows share one', () => {
    for (const entry of ALL_MODULE_ENTRIES) {
      expect(entry.transactionKind, entry.id).toBeTruthy()
    }

    const kinds = ALL_MODULE_ENTRIES.map((entry) => entry.transactionKind)
    expect(new Set(kinds).size).toBe(kinds.length)
  })

  it('never puts the same module in both the grid and the capability list', () => {
    for (const id of CAPABILITY_BACKED_MODULE_IDS) {
      expect(libraryModuleEntry(id), id).toBeUndefined()
      expect(moduleTransactionKind(id), id).toBeNull()
    }
  })

  it('keeps the capability-backed ids backed by a registered recipe', () => {
    // The list means "a capability spec exists for this". A recipe file
    // nothing embeds is the same dead card one level down, and that is
    // how `reshade.rs` and `ofxr_module.rs` survived in the tree.
    const orphaned = [...CAPABILITY_BACKED_MODULE_IDS]
      .filter((id) => !registeredRecipes.has(`${id}.yaml`))
      .sort()

    expect(orphaned).toEqual([])
  })

  it('does not let a delegated row answer a grid request', () => {
    for (const entry of DELEGATED_MODULE_ENTRIES) {
      expect(libraryModuleEntry(entry.id), entry.id).toBeUndefined()
    }
    for (const entry of LIBRARY_MODULE_ENTRIES) {
      expect(DELEGATED_MODULE_ENTRIES.map((row) => row.id), entry.id).not.toContain(entry.id)
    }
  })

  it('gives every dialog a probe to read its preview with', () => {
    // A row whose dialog has no probe would verify a module into silence:
    // no checks, no status, and a card that looks like it never ran.
    const probed = new Set(Object.keys(VERIFICATION_PROBES))
    const unprobed = LIBRARY_MODULE_ENTRIES
      .map((entry) => entry.dialog)
      .filter((dialog) => !probed.has(dialog))

    expect([...new Set(unprobed)]).toEqual([])
  })
})

describe('the module registry against the backend', () => {
  it('names only commands the app registers', () => {
    const unknown = [...new Set(declaredCommands)]
      .filter((command) => !registeredCommands.has(command))
      .sort()

    expect(unknown).toEqual([])
  })

  it('names only commands a feature service actually invokes', () => {
    // Without this, the table and the service could each be right about
    // a different command and the card would still be a dead button.
    const unspoken = [...new Set(declaredCommands)]
      .filter((command) => !commandsInvokedByServices.has(command))
      .sort()

    expect(unspoken).toEqual([])
  })

  it('does not declare an uninstall the card cannot run', () => {
    // The card asks the row whether it can remove itself; a row that
    // names a command but exposes no remover would roll the last
    // transaction back instead, which is a different operation.
    const inconsistent = LIBRARY_MODULE_ENTRIES
      .filter((entry) => Boolean(entry.remove) !== Boolean(entry.uninstallCommand))
      .map((entry) => entry.id)

    expect(inconsistent).toEqual([])
  })
})

describe('request builders', () => {
  // Asked through the table rather than through the builders directly:
  // the table is what the card uses, and a builder that works on its own
  // while its row points somewhere else is not the thing being promised.
  it('builds an OBS request from the catalog, defaulting the collection name', () => {
    const request = libraryModuleEntry('obs-vr')!
      .buildRequest(contextFor(moduleOf('obs-vr'))) as ObsVrRequest | null

    expect(request).toMatchObject({
      gameId: 'elden-ring',
      gameName: 'Elden Ring',
      sceneName: 'vr',
      sourceName: 'Elden Ring VR',
      executableName: 'eldenring.exe',
      collectionName: 'Sem nome',
    })
  })

  it('refuses an OptiScaler request with no proxy candidate', () => {
    // An injected DLL with nowhere to load is not a slower install, it
    // is a game that runs exactly as before.
    const module = moduleOf('optiscaler', { config: { version: '0.9.4' } })

    expect(libraryModuleEntry('optiscaler')!.buildRequest(contextFor(module))).toBeNull()
  })

  it('refuses an OFXR request whose recipe is missing its download', () => {
    const module = moduleOf('ofxr-framegen', { config: { implementationVersion: '2' } })

    expect(libraryModuleEntry('ofxr-framegen')!.buildRequest(contextFor(module))).toBeNull()
  })

  it('reads VR launch patches and recommendations out of the config', () => {
    const request = libraryModuleEntry('vr-launch')!
      .buildRequest(contextFor(moduleOf('vr-launch'))) as VrLaunchRequest | null

    expect(request?.configPatches).toContainEqual({ section: 'VR', key: 'StereoMode', value: 'full' })
    expect(request?.recommendations).toContainEqual({ label: 'Stereo mode', value: 'full' })
    expect(request?.requiredFiles).toEqual(['Game/ERVR/ERVR.dll'])
  })

  it('points UEVR at the release API of the chosen backend', () => {
    const module = moduleOf('uevr')
    const entry = libraryModuleEntry('uevr')!
    const nightly = entry.buildRequest(contextFor(module)) as UevrRequest | null
    const afw = entry.buildRequest(contextFor(module, { uevrBackend: () => 'afw' })) as UevrRequest | null

    expect(nightly?.releaseApiUrl).toContain('UEVR-nightly')
    expect(afw?.releaseApiUrl).toContain('PureDark/UEVR')
  })

  it('never answers with another module\'s request', () => {
    // Each row re-checks the id, so a table asked for the wrong id
    // refuses instead of quietly returning OptiScaler bytes for Cheeky.
    for (const entry of LIBRARY_MODULE_ENTRIES) {
      const other = moduleOf(entry.id === 'obs-vr' ? 'optiscaler' : 'obs-vr')
      expect(entry.buildRequest(contextFor(other)), entry.id).toBeNull()
    }
  })

  it('folds the per-game Cheeky note into the safety notes', () => {
    // The note is what tells a Stalker 2 player that the add-on was not
    // the thing tested upstream; every other module contributes none.
    expect(cheekyCompatibilityNote(moduleOf('cheeky-foveated-dlss'), 'stalker-2', (key) => key))
      .toBe('cheekyNoteStalker2')
    expect(cheekyCompatibilityNote(moduleOf('optiscaler'), 'stalker-2', (key) => key)).toBe('')
    expect(cheekyCompatibilityNote(moduleOf('cheeky-foveated-dlss'), 'doom-2016', (key) => key)).toBe('')
  })
})

describe('the grid, and the sections below it', () => {
  it('hands the capability section only what the grid does not own', () => {
    const owned = libraryOwnedModules(gameCatalog.find((entry) => entry.id === 'stalker-2')!.modules)
    const ids = owned.map((module) => module.id)

    // Stalker 2 declares `reshade` as an available module. The grid must
    // not also offer it, or the card that used to do nothing is back.
    expect(gameCatalog.find((entry) => entry.id === 'stalker-2')!.modules
      .map((module) => module.id)).toContain('reshade')
    expect(ids).not.toContain('reshade')
    expect(ids).toContain('ofxr-framegen')
  })

  it('keeps a module that the grid owns out of the capability section', () => {
    const owned = libraryOwnedModules(gameCatalog.find((entry) => entry.id === 'elden-ring')!.modules)

    expect(owned.map((module) => module.id)).toContain('cheeky-foveated-dlss')
  })
})

describe('where a module remembers what it knows', () => {
  it('keys state by catalog id so it follows a game across stores', () => {
    expect(moduleStateKey('obs-vr', '1245620', 'elden-ring')).toBe('elden-ring:obs-vr')
  })

  it('falls back to the app id, then to nothing, rather than colliding', () => {
    expect(moduleStateKey('obs-vr', '1245620', null)).toBe('1245620:obs-vr')
    expect(moduleStateKey('obs-vr', null, null)).toBe('unknown:obs-vr')
  })
})

describe('where an update check looks', () => {
  it('uses the recipe URL when it declares one', () => {
    const module = moduleOf('cheeky-foveated-dlss')

    expect(resolveModuleUpdateSource(module, contextFor(module)))
      .toBe('https://api.github.com/repos/ClarkCheekyKent/CheekyFoveatedDLSS/releases/latest')
  })

  it('has nowhere to look for a recipe that declares no source', () => {
    const module = moduleOf('obs-vr')

    expect(resolveModuleUpdateSource(module, contextFor(module))).toBeNull()
  })

  it('follows UEVR to the API of the selected backend', () => {
    const module = moduleOf('uevr')

    expect(resolveModuleUpdateSource(module, contextFor(module, { uevrBackend: () => 'joey-afw' })))
      .toBe('https://api.github.com/repos/PureDark/UEVR/releases/latest')
  })
})
