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
  return collectMissing(declared, [root], new Set(installedModuleIds))
}