import { invokeDebug as invoke } from '../../debug'
import type { PcgwLookupResult, PcgwSummary } from '../../types/pcgw'

/**
 * Look up a PCGamingWiki summary for the given slug. When the local
 * cache already holds a recent copy, this call returns it without
 * touching the network. Pass `forceRefresh: true` to bypass the cache.
 *
 * The slug must be the **canonical page title**, e.g.
 * `"Elden Ring"` for `https://www.pcgamingwiki.com/wiki/Elden_Ring`
 * (the API accepts spaces).
 */
export async function lookupPcgwSummary(
  slug: string,
  options: { forceRefresh?: boolean; signal?: AbortSignal } = {},
): Promise<PcgwLookupResult> {
  const { forceRefresh = false, signal } = options
  if (!slug.trim()) {
    throw new Error('PCGamingWiki slug is required.')
  }
  return invoke<PcgwLookupResult>('lookup_pcgw_summary', {
    request: { slug, forceRefresh },
    signal,
  })
}

/**
 * Read the cached summary without triggering a network call. Returns
 * `null` when the cache has never been populated for this slug — the
 * caller can then choose to call `lookupPcgwSummary` to fill it.
 */
export async function getPcgwCache(slug: string): Promise<PcgwSummary | null> {
  if (!slug.trim()) {
    return null
  }
  return invoke<PcgwSummary | null>('get_pcgw_cache', { slug })
}

export async function clearPcgwCache(slug: string): Promise<void> {
  if (!slug.trim()) {
    return
  }
  await invoke('clear_pcgw_cache', { slug })
}
