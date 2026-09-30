import type { ResolvedConfig } from '../../types/capability'

export interface CapabilitySummary {
  id: string
  displayName: string
  category: string
  status: string
  origin: 'builtIn' | 'local' | 'community'
}

export interface CommunityInstallRequest {
  capabilityId: string
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
  /**
   * `ResolvedConfig`, not a bare map: the backend's request struct wraps
   * the values in `values`, so sending `{}` here was rejected by the
   * deserializer before the install even started.
   */
  config: ResolvedConfig
  acceptUnsigned: boolean
  /** Install even when the spec's compatibility block rejects the game build. */
  force?: boolean
}

export interface CommunityInstallResult {
  capabilityId: string
  transaction: { id: string } | null
  steps: Array<{ kind: string; affectedPaths: string[] }>
  affectedPaths: string[]
}
