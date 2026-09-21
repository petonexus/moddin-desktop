import { parse } from 'yaml'
import { z } from 'zod'
import type { EnginePreset, GameCatalogEntry, ToolModuleDefinition } from '../types/game'

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

const presetSchema = z.object({
  id: z.string().min(1),
  displayName: z.string().min(1),
  description: z.string().min(1),
  modules: z.array(moduleSchema).default([]),
})

const presetFiles = import.meta.glob('../catalog/engines/*.yaml', {
  eager: true,
  import: 'default',
  query: '?raw',
}) as Record<string, string>

function readPreset(raw: string, source: string): EnginePreset {
  try {
    return presetSchema.parse(parse(raw))
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error)
    throw new Error(`Invalid Moddin engine preset ${source}: ${detail}`)
  }
}

function loadEnginePresets(): Map<string, EnginePreset> {
  const entries = Object.entries(presetFiles)
    .map(([source, raw]) => readPreset(raw, source))

  const map = new Map<string, EnginePreset>()
  for (const entry of entries) {
    if (map.has(entry.id)) {
      throw new Error(`Duplicate Moddin engine preset id: ${entry.id}`)
    }
    map.set(entry.id, entry)
  }
  return map
}

const presets = loadEnginePresets()

export function findEnginePreset(id: string): EnginePreset | undefined {
  return presets.get(id)
}

export function listEnginePresets(): EnginePreset[] {
  return Array.from(presets.values()).sort((left, right) => left.displayName.localeCompare(right.displayName))
}

/**
 * Merge an engine preset into a game's module list.
 *
 * Rules:
 * - Game modules win over preset modules with the same id (the game's
 *   `name`, `description`, `category`, `status`, and `config` are kept
 *   verbatim).
 * - Preset modules with ids the game does not declare are appended in
 *   the order they appear in the preset file.
 * - The merged result is the value the catalog loader publishes.
 */
export function mergePresetIntoGame(
  game: GameCatalogEntry,
  preset: EnginePreset | undefined,
): ToolModuleDefinition[] {
  if (!preset) return game.modules

  const gameModuleIds = new Set(game.modules.map((module) => module.id))
  const presetOnly = preset.modules.filter((module) => !gameModuleIds.has(module.id))

  return [...game.modules, ...presetOnly]
}

export function assertUniqueModuleIds(modules: ToolModuleDefinition[], context: string): void {
  const seen = new Set<string>()
  for (const module of modules) {
    if (seen.has(module.id)) {
      throw new Error(`Duplicate Moddin module id "${module.id}" in ${context}`)
    }
    seen.add(module.id)
  }
}