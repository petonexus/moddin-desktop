import { invokeDebug as invoke } from '../../debug'
import type { CommunityFetchResult } from '../../types/community'
import type { CapabilitySummary, CommunityInstallRequest, CommunityInstallResult } from './types'

export function listCapabilities() {
  return invoke<CapabilitySummary[]>('capability_list')
}

export function fetchCommunityCatalog(forceRefresh: boolean, ttlSeconds: number) {
  return invoke<CommunityFetchResult>('community_catalog_fetch', { forceRefresh, ttlSeconds })
}

export function setCommunityCatalogTtl(ttlSeconds: number) {
  return invoke<number>('community_catalog_set_ttl', { ttlSeconds })
}

export function installCommunityCapability(request: CommunityInstallRequest) {
  return invoke<CommunityInstallResult>('community_capability_install', { ...request })
}
