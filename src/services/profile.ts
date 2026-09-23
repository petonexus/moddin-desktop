import { stringify, parse } from 'yaml'
import {
  ModdinProfile,
  ModdinModpack,
  modpackSchema,
  profileSchema,
} from '../types/profile'
import { findCatalogGameById } from './catalog'

/**
 * Parse a YAML string into a `ModdinProfile` and validate that every
 * module id is known to the local catalog. Unknown ids are rejected so
 * the UI can surface a clear error before the user wastes an apply.
 */
export function parseProfile(yaml: string): ModdinProfile {
  const raw = parse(yaml)
  if (!raw || typeof raw !== 'object') {
    throw new Error('Moddin profile is empty or not a YAML object.')
  }
  if ((raw as Record<string, unknown>).kind !== 'moddin-profile') {
    throw new Error('This YAML is not a Moddin profile (missing `kind: moddin-profile`).')
  }
  const profile = profileSchema.parse(raw)
  validateProfileAgainstCatalog(profile)
  return profile
}

/**
 * Parse a YAML string into a `ModdinModpack`. Same catalog validation
 * rules as `parseProfile`, applied to every embedded profile.
 */
export function parseModpack(yaml: string): ModdinModpack {
  const raw = parse(yaml)
  if (!raw || typeof raw !== 'object') {
    throw new Error('Moddin modpack is empty or not a YAML object.')
  }
  if ((raw as Record<string, unknown>).kind !== 'moddin-modpack') {
    throw new Error('This YAML is not a Moddin modpack (missing `kind: moddin-modpack`).')
  }
  const modpack = modpackSchema.parse(raw)
  for (const profile of modpack.profiles) {
    validateProfileAgainstCatalog(profile)
  }
  return modpack
}

/**
 * Serialize a profile to YAML. Caller-controlled metadata (timestamps,
 * version) is filled in when missing.
 */
export function serializeProfile(profile: ModdinProfile): string {
  const enriched: ModdinProfile = {
    ...profile,
    createdAt: profile.createdAt ?? new Date().toISOString(),
  }
  return stringify(enriched)
}

export function serializeModpack(modpack: ModdinModpack): string {
  const enriched: ModdinModpack = {
    ...modpack,
    createdAt: modpack.createdAt ?? new Date().toISOString(),
  }
  return stringify(enriched)
}

/**
 * Build a profile from the catalog + the user's currently-enabled
 * modules. The frontend calls this when the user clicks "Export
 * profile" on a game detail view.
 */
export function buildProfileForGame(gameId: string, name: string, description: string): ModdinProfile {
  const game = findCatalogGameById(gameId)
  if (!game) {
    throw new Error(`Unknown Moddin game id: ${gameId}`)
  }
  return {
    kind: 'moddin-profile',
    version: 1,
    id: `${gameId}-${Date.now().toString(36)}`,
    name,
    description,
    gameId,
    modules: game.modules.map((module) => ({
      id: module.id,
      ...(module.config ? { config: normalizeConfig(module.config) } : {}),
    })),
  }
}

function normalizeConfig(
  config: Record<string, string | string[]>,
): Record<string, string | string[]> {
  // Catalog config already enforces the right shape; copy verbatim so
  // YAML serialization stays deterministic.
  return { ...config }
}

function validateProfileAgainstCatalog(profile: ModdinProfile): void {
  if (profile.kind !== 'moddin-profile') {
    throw new Error('Profile kind mismatch.')
  }
  const game = findCatalogGameById(profile.gameId)
  if (!game) {
    throw new Error(
      `Profile references unknown Moddin game '${profile.gameId}'. Add it to the catalog before importing.`,
    )
  }
  const catalogIds = new Set(game.modules.map((module) => module.id))
  for (const entry of profile.modules) {
    if (!catalogIds.has(entry.id)) {
      throw new Error(
        `Profile module '${entry.id}' is not in the catalog for game '${profile.gameId}'.`,
      )
    }
  }
}
