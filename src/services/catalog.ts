import { parse } from 'yaml'
import { z } from 'zod'
import type { GameCatalogEntry } from '../types/game'

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
  steamAppId: z.string().min(1),
  executable: z.string().min(1),
  modules: z.array(moduleSchema).default([]),
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
}

function readCatalogEntry(raw: string, source: string): GameCatalogEntry {
  try {
    return gameSchema.parse(parse(raw))
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error)
    throw new Error(`Invalid Moddin game catalog entry ${source}: ${detail}`)
  }
}

function assertUniqueModuleIds(game: GameCatalogEntry) {
  const moduleIds = new Set<string>()
  for (const module of game.modules) {
    if (moduleIds.has(module.id)) {
      throw new Error(`Duplicate Moddin module id "${module.id}" in game "${game.id}"`)
    }
    moduleIds.add(module.id)
  }
}

function loadGameCatalog(): LoadedCatalog {
  const entries = Object.entries(catalogFiles)
    .map(([source, raw]) => readCatalogEntry(raw, source))
    .sort((left, right) => left.name.localeCompare(right.name))

  const byId = new Map<string, GameCatalogEntry>()
  const bySteamAppId = new Map<string, GameCatalogEntry>()

  for (const game of entries) {
    if (byId.has(game.id)) {
      throw new Error(`Duplicate Moddin game catalog id: ${game.id}`)
    }
    if (bySteamAppId.has(game.steamAppId)) {
      throw new Error(`Duplicate Moddin Steam App ID: ${game.steamAppId}`)
    }

    assertUniqueModuleIds(game)
    byId.set(game.id, game)
    bySteamAppId.set(game.steamAppId, game)
  }

  return { entries, byId, bySteamAppId }
}

const catalog = loadGameCatalog()

export const gameCatalog: GameCatalogEntry[] = catalog.entries

export function findCatalogGameById(gameId: string) {
  return catalog.byId.get(gameId)
}

export function findCatalogGameBySteamAppId(appId: string) {
  return catalog.bySteamAppId.get(appId)
}
