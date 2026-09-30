/** One entry of the community repo's `revoked-ids.json`. */
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
  /** False when `revoked-ids.json` could not be read at all. The
   *  catalog can be perfectly signed and still be unusable: Moddin
   *  refuses to install while this is false, because "I could not check
   *  the kill switch" and "nothing is revoked" must not look the same. */
  revocationsVerified: boolean
  /** Revoked capabilities stay in the list, flagged, so the panel can
   *  say why one of them cannot be installed instead of quietly hiding
   *  it. */
  revoked: CommunityRevocation[]
}

export interface CommunityCatalog {
  version: number
  generatedAt: string
  generator: string
  capabilities: CommunityCatalogEntry[]
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