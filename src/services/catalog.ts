import { parse } from 'yaml'
import { z } from 'zod'
import type { GameCatalogEntry, InstalledGame, ToolModuleDefinition } from '../types/game'
import {
  assertUniqueModuleIds,
  findEnginePreset,
  mergePresetIntoGame,
} from './preset'

const configValueSchema = z
  .union([z.string(), z.number(), z.boolean(), z.array(z.string())])
  .transform((value) => (typeof value === 'string' || Array.isArray(value) ? value : String(value)))

const moduleSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1),
  category: z.enum(['vr', 'graphics', 'qol', 'system']),
  status: z.enum(['available', 'planned']),
  config: z.record(z.string(), configValueSchema).optional(),
  dependencies: z.array(z.string().min(1)).optional(),
})

const gameSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  steamAppId: z.string().min(1).optional(),
  epicAppId: z.string().min(1).optional(),
  gogAppId: z.string().min(1).optional(),
  executable: z.string().min(1),
  enginePreset: z.string().min(1).optional(),
  pcgwSlug: z.string().min(1).optional(),
  modules: z.array(moduleSchema).default([]),
}).refine((game) => Boolean(game.steamAppId || game.epicAppId || game.gogAppId), {
  message: 'A Moddin game catalog entry must define steamAppId, epicAppId, or gogAppId',
})

const catalogFiles = import.meta.glob('../catalog/games/*.yaml', {
  eager: true,
  import: 'default',
  query: '?raw',
}) as Record<string, string>

interface LoadedCatalog {
  entries: GameCatalogEntry[]
  byId: Map<string, GameCatalogEntry>
  bySteamAppId: Map<string, GameCatalogEntry>
  byEpicAppId: Map<string, GameCatalogEntry>
  byGogAppId: Map<string, GameCatalogEntry>
}

function readCatalogEntry(raw: string, source: string): GameCatalogEntry {
  try {
    return gameSchema.parse(parse(raw))
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error)
    throw new Error(`Invalid Moddin game catalog entry ${source}: ${detail}`)
  }
}

function applyEnginePreset(game: GameCatalogEntry, source: string): GameCatalogEntry {
  if (!game.enginePreset) return game

  const preset = findEnginePreset(game.enginePreset)
  if (!preset) {
    throw new Error(
      `Moddin game "${game.id}" references unknown engine preset "${game.enginePreset}" (${source}). Available presets must live under src/catalog/engines/.`,
    )
  }

  const mergedModules = mergePresetIntoGame(game, preset)
  assertUniqueModuleIds(mergedModules, `merged catalog entry for game "${game.id}" (preset "${game.enginePreset}")`)

  return { ...game, modules: mergedModules }
}

function loadGameCatalog(): LoadedCatalog {
  const entries = Object.entries(catalogFiles)
    .map(([source, raw]) => readCatalogEntry(raw, source))
    .map((game) => applyEnginePreset(game, `src/catalog/games/${game.id}.yaml`))
    .sort((left, right) => left.name.localeCompare(right.name))

  const byId = new Map<string, GameCatalogEntry>()
  const bySteamAppId = new Map<string, GameCatalogEntry>()
  const byEpicAppId = new Map<string, GameCatalogEntry>()
  const byGogAppId = new Map<string, GameCatalogEntry>()

  for (const game of entries) {
    if (byId.has(game.id)) {
      throw new Error(`Duplicate Moddin game catalog id: ${game.id}`)
    }
    if (game.steamAppId && bySteamAppId.has(game.steamAppId)) {
      throw new Error(`Duplicate Moddin Steam App ID: ${game.steamAppId}`)
    }
    if (game.epicAppId && byEpicAppId.has(game.epicAppId)) {
      throw new Error(`Duplicate Moddin Epic App ID: ${game.epicAppId}`)
    }
    if (game.gogAppId && byGogAppId.has(game.gogAppId)) {
      throw new Error(`Duplicate Moddin GOG App ID: ${game.gogAppId}`)
    }

    byId.set(game.id, game)
    if (game.steamAppId) bySteamAppId.set(game.steamAppId, game)
    if (game.epicAppId) byEpicAppId.set(game.epicAppId, game)
    if (game.gogAppId) byGogAppId.set(game.gogAppId, game)
  }

  return { entries, byId, bySteamAppId, byEpicAppId, byGogAppId }
}

const catalog = loadGameCatalog()

export const gameCatalog: GameCatalogEntry[] = catalog.entries

export function findCatalogGameById(gameId: string) {
  return catalog.byId.get(gameId)
}

export function findCatalogGameBySteamAppId(appId: string) {
  return catalog.bySteamAppId.get(appId)
}

export function findCatalogGameByEpicAppId(appId: string) {
  return catalog.byEpicAppId.get(appId)
}

export function findCatalogGameByGogAppId(appId: string) {
  return catalog.byGogAppId.get(appId)
}

export function findCatalogGameByInstalledGame(game: Pick<InstalledGame, 'store' | 'appId'>) {
  switch (game.store) {
    case 'epic':
      return findCatalogGameByEpicAppId(game.appId)
    case 'gog':
      return findCatalogGameByGogAppId(game.appId)
    default:
      return findCatalogGameBySteamAppId(game.appId)
  }
}

/**
 * Inverse of {@link findCatalogGameByInstalledGame}: given a catalog id,
 * return the installed game it refers to.
 *
 * The UI holds a catalog id everywhere (the selected game, the AI
 * context, the capability card) and only the store scan produces a store
 * app id. Without this bridge every caller that starts from the catalog
 * silently fails to find its install directory.
 */
export function findInstalledGameForCatalogGame(
  games: readonly Pick<InstalledGame, 'store' | 'appId' | 'installDir'>[],
  catalogGameId: string,
) {
  const entry = findCatalogGameById(catalogGameId)
  if (!entry) return undefined
  return games.find((game) => {
    const matched = findCatalogGameByInstalledGame(game)
    return matched?.id === entry.id
  })
}

/**
 * The catalogue's own entry for one module of one game, engine preset
 * already merged in — the same object the game page's module cards
 * render, so a caller that finds a module here is looking at the data
 * the player was shown. `undefined` when the game declares no such
 * module, which is not an error: the catalogue is per-game and silent
 * about the capabilities it does not carry.
 */
export function findCatalogModule(gameId: string, moduleId: string): ToolModuleDefinition | undefined {
  return findCatalogGameById(gameId)?.modules.find((module) => module.id === moduleId)
}

/**
 * The `config:` values the catalogue holds for `(gameId, capabilityId)`,
 * keyed by config-field name.
 *
 * This is the one place that answers "what values does this recipe need
 * for this game". It reads the per-game `config:` block out of
 * `src/catalog/games/*.yaml` — the same block the module's own
 * configuration dialog is built from — and returns it verbatim. An empty
 * object means the game declares no such module, or the module declares
 * no `config:` block; it never invents a value, and a second table beside
 * a form is exactly the drift this catalogue exists to prevent.
 */
export function resolveCatalogConfig(
  gameId: string | null | undefined,
  capabilityId: string,
): Record<string, string | string[]> {
  if (!gameId) return {}
  return { ...(findCatalogModule(gameId, capabilityId)?.config ?? {}) }
}

/**
 * Depth-first collection of the modules that are not installed yet, in
 * dependency order: a dependency always appears before the module that
 * requires it. Already-installed modules are skipped together with their
 * subtree, dependency ids this game does not declare are ignored, and
 * cycles are broken at the repeated id so the result stays finite.
 */
function collectMissing(
  declared: Map<string, ToolModuleDefinition>,
  roots: ToolModuleDefinition[],
  installed: Set<string>,
): string[] {
  const missing: string[] = []
  const resolved = new Set<string>()

  const visit = (module: ToolModuleDefinition, inStack: Set<string>) => {
    if (resolved.has(module.id) || installed.has(module.id)) return
    if (inStack.has(module.id)) return // cycle: break at the repeated id
    inStack.add(module.id)
    for (const dependencyId of module.dependencies ?? []) {
      const dependency = declared.get(dependencyId)
      if (dependency) visit(dependency, inStack)
    }
    inStack.delete(module.id)
    resolved.add(module.id)
    missing.push(module.id)
  }

  for (const module of roots) visit(module, new Set())
  return missing
}

/**
 * Ids of modules the game declares that are missing from
 * `installedModuleIds` and must be installed first, in dependency
 * order. Use it to prompt for prerequisite modules before installing a
 * dependent one.
 */
export function getMissingDependencies(
  game: Pick<GameCatalogEntry, 'modules'>,
  installedModuleIds: Iterable<string>,
): string[] {
  return collectMissing(
    new Map(game.modules.map((module) => [module.id, module])),
    game.modules,
    new Set(installedModuleIds),
  )
}

/**
 * Same traversal as {@link getMissingDependencies}, but rooted at a
 * single module: returns only what that module needs, in the order it
 * must be installed. The module itself is never part of the result, and
 * an id the game does not declare yields an empty list.
 */
export function getMissingDependenciesForModule(
  game: Pick<GameCatalogEntry, 'modules'>,
  moduleId: string,
  installedModuleIds: Iterable<string>,
): string[] {
  const declared = new Map(game.modules.map((module) => [module.id, module]))
  const root = declared.get(moduleId)
  if (!root) return []
  // `collectMissing` reports every node it visits, and the node it was
  // called with is one of them. Here the root is the module the caller
  // already asked about, so its own id is dropped: the question is what
  // has to be installed *first*, and answering "this module" sends
  // `configureModule` back to the prerequisite dialog instead of
  // forward to the install flow.
  return collectMissing(declared, [root], new Set(installedModuleIds)).filter(
    (id) => id !== moduleId,
  )
}