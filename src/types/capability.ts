export type CapabilityCategory = 'vr' | 'graphics' | 'qol' | 'system'
export type CapabilityStatus = 'available' | 'planned'
export type CheckCategory = 'global' | 'category' | 'modulespecific'
export type CheckSeverity = 'info' | 'warning' | 'blocker'
export type StepKind =
  | 'extract-zip'
  | 'verify-hash'
  | 'file-delete'
  | 'write-text-file'
  | 'spawn-process'
export type CheckKind =
  | 'process-running'
  | 'file-exists'
  | 'file-absent'
  | 'archive-reachable'
  | 'archive-sha256'

export interface ConfigFieldSpec {
  name: string
  type: 'string' | 'number' | 'boolean' | 'url' | 'sha256' | 'path' | 'enum'
  required?: boolean
  default?: string | number | boolean
  enumValues?: string[]
  description?: string
}

export interface CheckSpec {
  id: string
  label: string
  kind: CheckKind
  params?: Record<string, string | number | boolean | string[]>
  severity?: CheckSeverity
  category?: CheckCategory
  description?: string
}

export interface StepSpec {
  kind: StepKind
  params?: Record<string, string | number | boolean | string[]>
  description?: string
}

export interface CapabilitySpec {
  id: string
  displayName: string
  category: CapabilityCategory
  status: CapabilityStatus
  supportedEngines?: string[]
  configSchema?: ConfigFieldSpec[]
  checks?: CheckSpec[]
  install?: StepSpec[]
  uninstall?: StepSpec[]
  verify?: CheckSpec[]
  safetyNotes?: string[]
}

export interface ResolvedConfig {
  values: Record<string, string | number | boolean | string[]>
}

export interface InstallResult {
  capabilityId: string
  transaction: {
    id: string
    createdAt: number
    kind: string
    label: string
    gameId: string
    targetPath: string
    backupPath: string
    status: string
  } | null
  steps: Array<{
    kind: string
    description: string | null
    affectedPaths: string[]
  }>
  affectedPaths: string[]
}

/**
 * Three-layer UI layout. The desktop UI groups checks under three headings:
 *
 *  - `global` — every Moddin module evaluates (game present, not running).
 *  - `category` — every module in the same category shares (VR runtime for
 *    VR, GPU detection for Graphics, etc).
 *  - `modulespecific` — only this capability knows how to evaluate.
 */
export type UiCheckSection = 'global' | 'category' | 'modulespecific'

export interface UiCheck {
  id: string
  label: string
  passed: boolean
  detail?: string
  section: UiCheckSection
  severity: CheckSeverity
}

export interface UiCapabilityCard {
  spec: CapabilitySpec
  /** Pre-rendered verification row map, keyed by `CheckSpec.id`. */
  checks: Record<string, UiCheck>
  /** Resolved config (defaults merged) the UI should render in the form. */
  resolvedConfig: ResolvedConfig
}