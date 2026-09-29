export type CapabilityCategory = 'vr' | 'graphics' | 'qol' | 'system'
export type CapabilityStatus = 'available' | 'planned'
export type CheckCategory = 'global' | 'category' | 'modulespecific'
export type CheckSeverity = 'info' | 'warning' | 'blocker'
/**
 * Step kinds the runner can execute.
 *
 * Kept in lockstep with `builtin_steps::known_kinds()` — the order
 * mirrors the dispatch arm order there. `scripts/check-capability-kind-parity.mjs`
 * fails the build if this list, the community validator's
 * `KNOWN_STEP_KINDS` and the Rust list ever disagree, because a step
 * kind that exists in the backend but not here cannot be typed by the
 * AI/MCP authoring path at all.
 */
export type StepKind =
  | 'download-file'
  | 'extract-zip'
  | 'verify-hash'
  | 'file-delete'
  | 'write-text-file'
  | 'write-binary-file'
  | 'spawn-process'
  | 'move-file'
  | 'kill-process'
  | 'registry-write'
  | 'registry-delete'

/** Check kinds, mirroring `builtin_checks`. See {@link StepKind}. */
export type CheckKind =
  | 'process-running'
  | 'file-exists'
  | 'file-absent'
  | 'archive-reachable'
  | 'archive-sha256'
  | 'exe-version'

/**
 * Game-build compatibility window declared by a capability. Mirrors
 * `crate::capability::CompatibilitySpec` (serde camelCase). Absent (or
 * declared without any bound) means "compatible with every build"; the
 * backend then skips the exe-version probe entirely.
 */
export interface CapabilityCompatibility {
  /** Executable whose `FileVersion` is checked, relative to the game's executable directory. */
  gameExe?: string
  /** Lowest accepted `FileVersion`, inclusive. */
  minExeVersion?: string
  /** Highest accepted `FileVersion`, inclusive. */
  maxExeVersion?: string
  /** Exact `FileVersion`s that must never run with this capability. */
  blockedExeVersions?: string[]
}

/**
 * Where a capability recipe came from. Drives the UI badge, the
 * activity log level, and (for community + unsigned) whether the
 * install prompts for confirmation.
 *
 * Mirrors `crate::capability::SpecOrigin` in Rust.
 */
export type CapabilityOrigin = 'builtIn' | 'local' | 'community'

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
  /** One sentence written for a person: what the mod does for the
   * player. Rendered as the card's supporting line; the UI falls back to
   * the technical id when a recipe does not declare one. */
  description?: string
  category: CapabilityCategory
  status: CapabilityStatus
  supportedEngines?: string[]
  /** Ids of capabilities that must be installed before this one. */
  dependencies?: string[]
  compatibility?: CapabilityCompatibility
  configSchema?: ConfigFieldSpec[]
  checks?: CheckSpec[]
  install?: StepSpec[]
  uninstall?: StepSpec[]
  verify?: CheckSpec[]
  safetyNotes?: string[]
}

/** Lightweight summary the desktop uses to render the capability
 * gallery (one card per entry). Mirrors
 * `crate::capability_runner::CapabilitySummary`. */
export interface CapabilitySummary {
  id: string
  displayName: string
  /** Player-facing one-liner. Falls back to `id` in the UI when a recipe
   * does not declare one. */
  description?: string
  category: CapabilityCategory
  status: CapabilityStatus
  origin: CapabilityOrigin
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
  /** Ids auto-installed as dependencies of this install, in install order. */
  installedDependencies?: string[]
  /**
   * Evaluated exe-version outcome when the spec declares a
   * `compatibility` block. Present (and possibly `passed: false`) even
   * on a forced install, so the UI can surface what was overridden.
   */
  compatibility?: CapabilityCheckOutcome
}

/**
 * One evaluated check as the backend reports it. Mirrors Rust
 * `crate::module::CheckOutcome` (serde camelCase): `id` is optional
 * (absent for synthesized rows on some paths), `label`/`passed` are
 * always present, `detail` is `null` when the check carries no detail.
 */
export interface CapabilityCheckOutcome {
  id?: string | null
  label: string
  passed: boolean
  detail?: string | null
  category?: string
  severity?: string
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