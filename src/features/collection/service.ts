/**
 * The collections service: the only place in the frontend that talks to
 * the backend about collections.
 *
 * A collection is a curated list of capabilities, not a second install
 * path. `src-tauri/src/collection.rs` loads the YAML, refuses a set that
 * could not be installed, and returns the members in install order with
 * the engine verdict for the selected game already attached. Installing
 * one is the existing `community_capability_install` command, once per
 * member, driven by `useCollectionInstall`.
 *
 * The selected game is resolved here rather than in the panel, for the
 * same reason `capability_list` is called with an engine instead of a
 * game id: the catalogue is a frontend data set and the engine gate is a
 * backend rule, and this is the seam. Resolving it here means the panel
 * and the AI assistant get the same verdicts for the same game, and the
 * backend decides once which members the engine gate would hide.
 */
import { invokeDebug as invoke } from '../../debug'
import { findCatalogGameById } from '../../services/catalog'
import { readLocalValue } from '../../services/storage'
import type { EngineMatch } from '../../types/capability'

/**
 * Why the loader will not install a collection for the selected game.
 *
 * Data, not prose: the sentence is the locale's. `src-tauri` decides
 * *which* of these applies and the panel decides how to say it, so the
 * rule lives in one place and the wording in another.
 */
export interface CollectionBlocker {
  /** The collection is curated for another game. */
  kind: 'wrongGame' | 'engineMismatch'
  /** The member at fault, when the reason is about one member. */
  capabilityId: string | null
  /** The engines that member does declare. */
  supportedEngines: string[]
  /** The game the collection was curated for. */
  targetGame: string | null
}

export interface CollectionMember {
  id: string
  displayName: string
  /** Why this member is in the set, from the collection's own note. */
  rationale: string
  required?: boolean
  /** The engine verdict the backend decided. See {@link EngineMatch}. */
  engineMatch?: EngineMatch
}

/**
 * The install preview: the members in order, and the reasons the run
 * cannot start.
 *
 * `blocked` is empty when the collection can install for the selected
 * game, and absent means the caller sent no verdict at all — which is
 * treated as "nothing blocks", not as a reason to hide the set.
 */
export interface CollectionPreset {
  /** The members, in install order. */
  capabilities: CollectionMember[]
  /**
   * Empty when the collection can install for the selected game. Absent
   * means the caller sent no verdict at all, which is treated as
   * "nothing blocks" rather than as a reason to hide the set.
   */
  blocked?: CollectionBlocker[]
  /**
   * Not sent by the loader. The install directory and executable
   * directory are per-user paths, resolved at install time by
   * `resolveInstallTarget`; a collection file cannot know them, so these
   * stay optional rather than travelling as empty strings.
   */
  installDir?: string
  executableDir?: string
}

export interface CollectionSummary {
  id: string
  displayName: string
  category: string
  description: string
  targetGame: string | null
  capabilityCount: number
  requiredCount: number
  /** Optional preset data for the install preview; absent means "no preview". */
  preset?: CollectionPreset
}

/** What the loader is asked about: the game the set would go into. */
export interface CollectionRequest {
  gameId: string | null
  /** The game's `enginePreset`, or null when it declares none. */
  engine: string | null
}

/**
 * The selected game, in the shape the loader wants.
 *
 * A game that is in the library but not in the catalogue still gets its
 * id: that is what makes `wrongGame` a real verdict, because the loader
 * compares the collection's `targetGame` against it. Only the engine is
 * lost, and a missing engine is the loader's `noGameEngine` case, which
 * gates nothing.
 */
export function selectedGameRequest(): CollectionRequest {
  const gameId = readLocalValue('moddin-selected-appId')
  if (!gameId) return { gameId: null, engine: null }
  const game = findCatalogGameById(gameId)
  return { gameId: game?.id ?? gameId, engine: game?.enginePreset ?? null }
}

export function listCollections(): Promise<CollectionSummary[]> {
  return invoke<CollectionSummary[]>('collection_list', { request: selectedGameRequest() })
}
