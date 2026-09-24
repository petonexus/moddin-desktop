/**
 * Stub for the collections feature.
 *
 * The full collections feature ships on the `feat/agent-mcp` branch. On the
 * `feat/ai-ui-ux-helper` branch we only need the AI assistant to be able to
 * call these functions without breaking — recommendations can still flow
 * through for capabilities (which install via the standard
 * `community_capability_install` command) while the collection-specific path
 * surfaces a clear "not yet supported" message.
 */

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

export interface CollectionPreset {
  installDir: string
  executableDir: string
  capabilities: Array<{
    id: string
    displayName: string
    rationale: string
  }>
}

export function listCollections(): Promise<CollectionSummary[]> {
  // No collections shipped on this branch yet. Return an empty catalog so
  // the AI composer can still recommend individual capabilities.
  return Promise.resolve([])
}

export async function installCollection(_request: {
  collectionId: string
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
}): Promise<unknown> {
  throw new Error(
    'Collections are not yet available on this build. Install individual capabilities instead.',
  )
}
