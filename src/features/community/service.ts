import { invokeDebug as invoke } from '../../debug'
import type { CommunityFetchResult } from '../../types/community'
import type { CapabilitySummary, CommunityInstallRequest, CommunityInstallResult } from './types'

export function listCapabilities() {
  return invoke<CapabilitySummary[]>('capability_list')
}

export function fetchCommunityCatalog(forceRefresh: boolean, ttlSeconds: number) {
  return invoke<CommunityFetchResult>('community_catalog_fetch', { request: { forceRefresh, ttlSeconds } })
}

export function setCommunityCatalogTtl(ttlSeconds: number) {
  return invoke<number>('community_catalog_set_ttl', { request: { ttlSeconds } })
}

export function installCommunityCapability(request: CommunityInstallRequest) {
  return invoke<CommunityInstallResult>('community_capability_install', { request })
}

/**
 * Re-read the local capability override directory and return what is
 * loaded now. Called alongside the catalog refresh so a YAML the user
 * just dropped into `%LOCALAPPDATA%\Moddin\capabilities\` shows up
 * without an app restart.
 */
export function reloadCapabilities() {
  return invoke<CapabilitySummary[]>('capability_reload', { request: {} })
}
