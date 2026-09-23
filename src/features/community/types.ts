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
  config: Record<string, unknown>
  acceptUnsigned: boolean
}

export interface CommunityInstallResult {
  capabilityId: string
  transaction: { id: string } | null
  steps: Array<{ kind: string; affectedPaths: string[] }>
  affectedPaths: string[]
}
