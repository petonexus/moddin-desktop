/** One capability-level revocation, carried *inside* the signed
 *  catalog. It arrives inlined from `revoked-ids.json` by the community
 *  repo's `regenerate_catalog.py`. */
export interface CommunityRevocation {
  id: string
  reason: string
}

/**
 * Mirrors `crate::community_catalog::CommunityFetchResult` in Rust.
 * Returned by the `community_catalog_fetch` Tauri command.
 */
export interface CommunityFetchResult {
  catalog: CommunityCatalog
  cached: boolean
  cachedAt: number
  ttlSeconds: number
  signatureVerified: boolean
  /** First 16 hex chars of the SHA-256 fingerprint of the pinned
   *  public key that verified (or attempted to verify) the catalog. */
  bootstrapPublicKeyFingerprint: string
  /** When `signatureVerified` is false OR the fetch failed, this
   *  carries the underlying error message for the UI banner. */
  lastError: string | null
}

export interface CommunityCatalog {
  version: number
  generatedAt: string
  generator: string
  capabilities: CommunityCatalogEntry[]
  /** The maintainers' kill switch, covered by the same signature as
   *  `capabilities`. An absent field in a validly signed catalog is a
   *  statement that nothing is revoked, not a failed load — see
   *  `moddin-community-capabilities/SECURITY.md`, "Layer 6 — kill
   *  switch". There is no `revocationsVerified` flag: the list is part
   *  of the signed bytes, so `signatureVerified` already covers it. */
  revoked: CommunityRevocation[]
}

export interface CommunityCatalogEntry {
  id: string
  version: string
  displayName: string
  category: string
  status: string
  homepage: string | null
  downloadUrl: string | null
  configSchema: unknown[]
  safetyNotes: string[]
  /** True when the entry ships a SIGNED-BY in the community repo. */
  signed: boolean
}

/**
 * Mirrors `crate::community_catalog::CommunityConfig`.
 * Persisted client-side to remember the user's TTL choice across
 * restarts.
 */
export interface CommunityConfig {
  ttlSeconds: number
}

/** Common TTL choices the Settings → Community panel exposes. */
export const COMMUNITY_TTL_PRESETS: Array<{ label: string; seconds: number }> = [
  { label: '1 hour', seconds: 60 * 60 },
  { label: '6 hours', seconds: 6 * 60 * 60 },
  { label: '24 hours (default)', seconds: 24 * 60 * 60 },
  { label: '7 days', seconds: 7 * 24 * 60 * 60 },
  { label: 'Manual only', seconds: 0 }, // 0 = never auto-refresh; user clicks Refresh
]