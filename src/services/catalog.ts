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

function readCatalogEntry(raw: string, source: string): GameCatalogEntry {
  try {
    return gameSchema.parse(parse(raw))
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error)
    throw new Error(`Invalid Moddin game catalog entry ${source}: ${detail}`)
  }
}

function loadGameCatalog(): GameCatalogEntry[] {
  const entries = Object.entries(catalogFiles)
    .map(([source, raw]) => readCatalogEntry(raw, source))
    .sort((left, right) => left.name.localeCompare(right.name))

  const ids = new Set<string>()
  const steamAppIds = new Set<string>()

  for (const game of entries) {
    if (ids.has(game.id)) {
      throw new Error(`Duplicate Moddin game catalog id: ${game.id}`)
    }
    if (steamAppIds.has(game.steamAppId)) {
      throw new Error(`Duplicate Moddin Steam App ID: ${game.steamAppId}`)
    }

    ids.add(game.id)
    steamAppIds.add(game.steamAppId)
  }

  return entries
}

export const gameCatalog: GameCatalogEntry[] = loadGameCatalog()

export function findCatalogGameBySteamAppId(appId: string) {
  return gameCatalog.find((game) => game.steamAppId === appId)
}
