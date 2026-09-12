import { parse } from 'yaml'
import { z } from 'zod'
import type { GameCatalogEntry } from '../types/game'
import cyberpunkRaw from '../catalog/games/cyberpunk-2077.yaml?raw'
import eldenRingRaw from '../catalog/games/elden-ring.yaml?raw'

const moduleSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1),
  category: z.enum(['vr', 'graphics', 'qol', 'system']),
  status: z.enum(['available', 'planned']),
})

const gameSchema = z.object({
  id: z.string().min(1),
  name: z.string().min(1),
  steamAppId: z.string().min(1),
  executable: z.string().min(1),
  modules: z.array(moduleSchema).default([]),
})

function readCatalogEntry(raw: string): GameCatalogEntry {
  return gameSchema.parse(parse(raw))
}

export const gameCatalog: GameCatalogEntry[] = [
  readCatalogEntry(eldenRingRaw),
  readCatalogEntry(cyberpunkRaw),
]

export function findCatalogGameBySteamAppId(appId: string) {
  return gameCatalog.find((game) => game.steamAppId === appId)
}
