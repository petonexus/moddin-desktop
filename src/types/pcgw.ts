/**
 * A `PcgwSummary` is the parsed, locally-cached projection of a
 * PCGamingWiki page. It is intentionally narrow: a few fields the
 * Moddin UI surfaces in the game detail view, plus a stable `pageUrl`
 * for users who want the full article.
 *
 * The shape mirrors `src-tauri/src/pcgw_cache.rs::PcgwSummary`.
 */
export interface PcgwSummary {
  slug: string
  pageUrl: string
  /** Epoch milliseconds when the local cache was populated. */
  fetchedAtMillis: number
  /** PCGW revision id at fetch time, when the API exposes it. */
  sourceRevisionId: number | null
  engine: string | null
  executableName: string | null
  installLocation: string | null
  saveGameDataLocation: string | null
  categories: string[]
  issueLines: string[]
  rawExcerpt: string | null
}

export interface PcgwLookupResult {
  summary: PcgwSummary
  /** Hours since the cache file was populated. `null` on cold cache. */
  stalenessHours: number | null
  /** `true` when this call hit the network. */
  fromNetwork: boolean
}
