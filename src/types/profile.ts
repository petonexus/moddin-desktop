import { z } from 'zod'

/**
 * A `ModdinProfile` captures the **applied state** of a single Moddin
 * game so the user can export it as a YAML file and re-apply it on
 * another machine (or another copy of the same game). Profiles never
 * bundle binaries or proxy DLLs — they only describe what *should* be
 * applied; the receiving install still pulls the artifacts from the
 * official upstream.
 *
 * Profiles are pure data; they do not invent new module definitions.
 * The catalog ships the recipe; the profile says "for game `elden-ring`
 * enable these catalog modules with these config overrides". Import
 * validation rejects ids the local catalog does not know.
 */
export const profileModuleEntrySchema = z.object({
  id: z.string().min(1),
  /** Module config overrides merged on top of the catalog recipe. */
  config: z.record(z.string(), z.union([z.string(), z.array(z.string())])).optional(),
})

export const profileSchema = z.object({
  kind: z.literal('moddin-profile'),
  version: z.literal(1),
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1).default(''),
  gameId: z.string().min(1),
  modules: z.array(profileModuleEntrySchema).min(1),
  openXrOverride: z
    .object({
      runtimeId: z.string().min(1),
    })
    .optional(),
  launchArguments: z.array(z.string()).optional(),
  graphicsSettings: z.record(z.string(), z.string()).optional(),
  createdAt: z.string().optional(),
})

export type ModdinProfile = z.infer<typeof profileSchema>
export type ProfileModuleEntry = z.infer<typeof profileModuleEntrySchema>

/**
 * A `ModdinModpack` is the multi-game counterpart: it bundles several
 * profiles into a single artifact that applies each one in sequence.
 * The apply flow is one transaction per game so partial failure is
 * local to the affected game.
 */
export const modpackSchema = z.object({
  kind: z.literal('moddin-modpack'),
  version: z.literal(1),
  id: z.string().min(1),
  name: z.string().min(1),
  description: z.string().min(1).default(''),
  profiles: z.array(profileSchema).min(1),
  createdAt: z.string().optional(),
})

export type ModdinModpack = z.infer<typeof modpackSchema>
