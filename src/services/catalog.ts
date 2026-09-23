import { parse } from 'yaml'
import { z } from 'zod'
import type { GameCatalogEntry, InstalledGame } from '../types/game'
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