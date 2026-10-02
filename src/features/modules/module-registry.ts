import type { CheekyFoveatedDlssPreview, CheekyFoveatedDlssRequest } from '../../types/cheeky'
import type { GameCatalogEntry, InstalledGame, ToolModuleDefinition } from '../../types/game'
import type { ObsVrPreview, ObsVrRequest } from '../../types/obs'
import type { OfxrPreview, OfxrRequest } from '../../types/ofxr'
import type { OptiScalerPreview, OptiScalerRequest } from '../../types/optiscaler'
import type { TransactionRecord } from '../../types/transaction'
import type { UevrBackend, UevrPreview, UevrRequest } from '../../types/uevr'
import type { VrLaunchPreview, VrLaunchRequest } from '../../types/vr-launch'
import {
  previewCheekyFoveatedDlss,
  previewObsVr,
  previewOfxr,
  previewOptiScaler,
  previewUevr,
  previewVrLaunch,
  uninstallCheekyFoveatedDlss,
  uninstallObsVr,
  uninstallOfxr,
  uninstallOptiScaler,
  uninstallUevr,
} from './service'
import { VERIFICATION_PROBES, type VerificationOutcome, type VerificationProbe } from './verification-probes'

/**
 * The library grid's per-module registry: what Moddin does for each
 * module id the catalog declares.
 *
 * It used to be a `module.id === '...'` chain in `App.vue`, repeated
 * once per action (preview, install, uninstall) and once per card. A
 * card for an id no branch mentioned rendered an Install button that
 * ended in `moduleNoAction` — `reshade` and `ofxr-framegen` both shipped
 * that way before the capability section took them over. The table is
 * the one place an id maps to a command, so
 * `__tests__/module-registry.test.ts` can hold the catalogue against
 * it: every module the grid renders must appear here or be
 * capability-backed, and every command named here must be registered in
 * `src-tauri/src/lib.rs`.
 *
 * Nothing in this file reaches Tauri directly; the native calls live in
 * `service.ts`. What is here is the mapping and the request each command
 * needs, which is what a card needs to know.
 */

/** vue-i18n's `t`, narrowed to what a builder asks of it. */
export type Translate = (key: string, named?: Record<string, unknown>) => string

/** The selected game together with the catalog entry that matched it. */
export interface ModuleGame {
  installed: InstalledGame
  catalog: GameCatalogEntry
}

/** Everything a request builder is allowed to read. */
export interface ModuleRequestContext {
  /** The module being acted on, straight from the catalog. */
  module: ToolModuleDefinition
  /** Selected game and its catalog entry, or null when none is selected. */
  game: ModuleGame | null
  t: Translate
  /** The backend variant chosen for this module. Only UEVR reads it. */
  uevrBackend: (module: ToolModuleDefinition) => UevrBackend
}

/** Module ids the library grid still owns (it has an installer for them). */
export type LibraryModuleId =
  | 'vr-launch'
  | 'obs-vr'
  | 'optiscaler'
  | 'ofxr-framegen'
  | 'cheeky-foveated-dlss'
  | 'uevr'
  | 'desktop-shortcut'

/** What the backend answers when asked for a module's change preview. */
export interface ModulePreviewByDialog {
  'vr-launch': VrLaunchPreview
  'obs-vr': ObsVrPreview
  optiscaler: OptiScalerPreview
  ofxr: OfxrPreview
  cheeky: CheekyFoveatedDlssPreview
  uevr: UevrPreview
}

/** The request each dialog's preview is built from. */
export interface ModuleRequestByDialog {
  'vr-launch': VrLaunchRequest
  'obs-vr': ObsVrRequest
  optiscaler: OptiScalerRequest
  ofxr: OfxrRequest
  cheeky: CheekyFoveatedDlssRequest
  uevr: UevrRequest
}

export type ModuleDialogName = keyof ModulePreviewByDialog

/**
 * A module's change preview, tagged with the dialog it belongs in. The
 * caller narrows on `dialog` to place it, which is why the tag travels
 * with the payload instead of being looked up again.
 */
export type ModulePreviewResult = {
  [K in ModuleDialogName]: {
    dialog: K
    request: ModuleRequestByDialog[K]
    preview: ModulePreviewByDialog[K]
  }
}[ModuleDialogName]

/**
 * One entry per module the grid can act on itself.
 *
 * `dialog` is the discriminant: narrowing it in a `switch` narrows
 * `buildRequest` and `preview` with it, so a caller never has to cast a
 * payload into the dialog it is about to render. That is why the entry
 * carries the preview call and not only the command name — the command
 * is still declared here, and the test checks that it is registered.
 */
export type ModuleEntry<D extends ModuleDialogName = ModuleDialogName> = {
  [K in D]: {
    /** Module id as the catalog spells it. */
    id: LibraryModuleId
    /** Which dialog this module's changes are shown in. */
    dialog: K
    /** Native command that answers "what will this change". */
    previewCommand: string
    /** Native command that applies the preview. */
    installCommand: string
    /**
     * Native command that removes it, or null when the only way back is
     * rolling this module's last transaction back.
     */
    uninstallCommand: string | null
    /** Kind this module's changes are recorded under in the store. */
    transactionKind: string
    /** Catalog module plus selected game -> the preview's request. */
    buildRequest: (context: ModuleRequestContext) => ModuleRequestByDialog[K] | null
    preview: (request: ModuleRequestByDialog[K]) => Promise<ModulePreviewByDialog[K]>
    /** Build the request, ask the backend, tag the answer for its dialog. */
    openPreview: (context: ModuleRequestContext) => Promise<Extract<ModulePreviewResult, { dialog: K }>>
    /** Preview, then read it as a checklist the card can show. */
    verify: (context: ModuleRequestContext, t: Translate) => Promise<VerificationOutcome>
    /**
     * Remove this module through its own command, or null when its only
     * undo is the transaction it recorded.
     *
     * Unlike install, removal is uniform enough to belong here: the app
     * still owns the confirm, the success line and the refreshes, but
     * every module answers with a `TransactionRecord` or nothing at all.
     */
    remove: ((context: ModuleRequestContext) => Promise<void>) | null
  }
}[D]

/**
 * A module the grid dispatches to another feature instead of invoking
 * itself. Its commands are still declared, so the catalogue test can see
 * that this card has a live path rather than a dead button.
 */
export interface DelegatedModuleEntry {
  id: LibraryModuleId
  /** The feature that owns this module's native flow. */
  delegate: string
  previewCommand: string
  installCommand: string
  uninstallCommand: string | null
  transactionKind: string
}

export type AnyModuleEntry = ModuleEntry | DelegatedModuleEntry

/**
 * Catalog modules Moddin installs through the capability pipeline
 * (`src-tauri/capabilities/`) instead of a dedicated Rust installer.
 *
 * They are rendered by the capability section, which already has the
 * config form, the preflight checklist, the compatibility warning and
 * the rollback — so listing them in the library grid too would show the
 * same recipe twice and give the grid a button that can only fail.
 * Keep this list in step with the specs that exist in that folder.
 *
 * `reshade` and `ofxr-bridge` were missing from this list. Both were
 * grid cards whose only backing was a file nothing dispatched to —
 * `reshade.rs` and `ofxr_module.rs` had no caller, so the cards fell
 * through every branch of `configureModule` and threw "no action". A
 * button that advertises an install and cannot perform it is the exact
 * defect this list exists to prevent. Both recipes are registered and
 * available, so the capability section is the path that works and the
 * grid card is the one that had to go.
 */
export const CAPABILITY_BACKED_MODULE_IDS = new Set([
  'bepinex',
  'ue4ss',
  'reframework',
  'reshade',
  'ofxr-bridge',
])

/** True when the capability section, not the library grid, owns the card. */
export function isCapabilityBackedModule(module: ToolModuleDefinition) {
  return CAPABILITY_BACKED_MODULE_IDS.has(module.id)
}

/** Catalog modules the library grid still owns (it has an installer). */
export function libraryOwnedModules(modules: readonly ToolModuleDefinition[]): ToolModuleDefinition[] {
  return modules.filter((module) => !isCapabilityBackedModule(module))
}

/**
 * Card title per module id, in the player's words. Ids without an entry
 * fall back to the catalog's own name.
 */
const LOCALIZED_MODULE_NAME_KEYS: Record<string, string> = {
  'vr-launch': 'moduleVrLaunch',
  'obs-vr': 'moduleObsVr',
  optiscaler: 'moduleOptiScaler',
  'ofxr-framegen': 'moduleOfxr',
  'cheeky-foveated-dlss': 'moduleCheeky',
  uevr: 'moduleUevr',
  openxr: 'moduleOpenXr',
  'desktop-shortcut': 'moduleDesktopShortcut',
}

/**
 * Card description per module id, same fallback. A capability-backed
 * card never reaches this map — it renders in the capability section
 * with the recipe's own copy.
 */
const LOCALIZED_MODULE_DESCRIPTION_KEYS: Record<string, string> = {
  'vr-launch': 'moduleVrLaunchDescription',
  'obs-vr': 'moduleObsVrDescription',
  optiscaler: 'moduleOptiScalerDescription',
  'ofxr-framegen': 'moduleOfxrDescription',
  'cheeky-foveated-dlss': 'moduleCheekyDescription',
  uevr: 'moduleUevrDescription',
  openxr: 'moduleOpenXrDescription',
  'desktop-shortcut': 'moduleDesktopShortcutDescription',
}

/** Localized card title for a module id, falling back to `fallback`. */
export function moduleNameKey(id: string, fallback = id) {
  return LOCALIZED_MODULE_NAME_KEYS[id] ?? fallback
}

/** Localized card description for a module, falling back to the catalog's. */
export function moduleDescriptionKey(module: ToolModuleDefinition) {
  return LOCALIZED_MODULE_DESCRIPTION_KEYS[module.id] ?? module.description
}

/**
 * Where a module's "is there a newer version?" check looks. Null means
 * the recipe declares no update source, which is not an error: the card
 * hides the update area instead of advertising a check the backend
 * cannot run.
 */
export function resolveModuleUpdateSource(
  module: ToolModuleDefinition,
  context: Pick<ModuleRequestContext, 'uevrBackend'>,
): string | null {
  const config = module.config ?? {}
  const configured = config.updateUrl
  if (typeof configured === 'string' && configured.trim()) return configured.trim()

  if (module.id === 'uevr') {
    const backend = context.uevrBackend(module)
    const sourceKey = backend === 'afw' || backend === 'joey-afw' ? 'afwReleaseApiUrl' : 'releaseApiUrl'
    const source = config[sourceKey]
    return typeof source === 'string' && source.trim() ? source.trim() : null
  }

  return null
}

export function moduleHasUpdateSource(
  module: ToolModuleDefinition,
  context: Pick<ModuleRequestContext, 'uevrBackend'>,
) {
  return Boolean(resolveModuleUpdateSource(module, context))
}

/**
 * Stable key for everything Moddin remembers per (game, module): the
 * verification, the update check and the compatibility report. The
 * catalog id is preferred over the store app id so state follows a game
 * across stores, and 'unknown' is the deliberate last resort for state
 * recorded before a game was selected.
 */
export function moduleStateKey(moduleId: string, appId: string | null, catalogGameId: string | null) {
  return `${catalogGameId ?? appId ?? 'unknown'}:${moduleId}`
}

// ---------------------------------------------------------------------------
// Catalog config readers
// ---------------------------------------------------------------------------

/**
 * A config value that may arrive as a YAML list or as one comma-separated
 * string. Returns the list either way, so a builder never has to know
 * which form the catalog author used.
 */
export function configList(config: ToolModuleDefinition['config'], key: string): string[] {
  const value = config?.[key]
  if (Array.isArray(value)) return value
  if (typeof value !== 'string' || !value.trim()) return []
  return value.split(',').map((item) => item.trim()).filter(Boolean)
}

function configBool(
  config: ToolModuleDefinition['config'],
  key: string,
  fallback = false,
): boolean {
  const value = config?.[key]
  if (typeof value !== 'string') return fallback
  return ['1', 'true', 'yes', 'on'].includes(value.trim().toLowerCase())
}

/** The version a recipe pins, or '?' when it pins none. */
export function moduleVersion(module: ToolModuleDefinition) {
  return typeof module.config?.version === 'string' ? module.config.version : '?'
}

// ---------------------------------------------------------------------------
// Per-module compatibility note
// ---------------------------------------------------------------------------

/**
 * The Cheeky research note for the selected game, folded into the
 * module's safety notes and shown next to the recorded compatibility.
 * Only Cheeky has one; every other module gets an empty string so the
 * caller can filter without a type check.
 */
export function cheekyCompatibilityNote(
  module: ToolModuleDefinition,
  gameId: string | undefined,
  t: Translate,
): string {
  if (module.id !== 'cheeky-foveated-dlss') return ''
  if (gameId === 'cyberpunk-2077') return t('cheekyNoteCyberpunk')
  if (gameId === 'elden-ring') return t('cheekyNoteEldenRing')
  if (gameId === 'stalker-2') return t('cheekyNoteStalker2')
  return ''
}

// ---------------------------------------------------------------------------
// Request builders
// ---------------------------------------------------------------------------

function buildObsRequest(context: ModuleRequestContext): ObsVrRequest | null {
  const { module } = context
  if (module.id !== 'obs-vr' || !context.game) return null

  const { catalog } = context.game
  const config = module.config ?? {}
  const executableName = typeof config.executableName === 'string'
    ? config.executableName
    : catalog.executable.split(/[\\/]/).pop()
  if (!executableName) return null

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    collectionName: typeof config.collectionName === 'string' ? config.collectionName : context.t('unnamedCollection'),
    sceneName: typeof config.sceneName === 'string' ? config.sceneName : 'vr',
    sourceName: typeof config.sourceName === 'string' ? config.sourceName : `${catalog.name} VR`,
    executableName,
  }
}

function optiScalerSafetyNotes(gameId: string, t: Translate): string[] {
  const notes = [t('optiSafetyOnline')]
  if (gameId === 'elden-ring') notes.push(t('optiSafetyEldenRing'))
  if (gameId === 'stalker-2') notes.push(t('optiSafetyStalker2'))
  return notes
}

function buildOptiScalerRequest(context: ModuleRequestContext): OptiScalerRequest | null {
  const { module } = context
  if (module.id !== 'optiscaler' || !context.game) return null

  const { catalog, installed } = context.game
  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  if (!version || !downloadUrl || !sha256) return null

  // No proxy candidate means the game would load nothing: OptiScaler is
  // an injected DLL, so there is no sensible "install it anyway".
  const proxyCandidates = configList(config, 'proxyCandidates')
  if (!proxyCandidates.length) return null

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executable: catalog.executable,
    version,
    downloadUrl,
    sha256,
    proxyCandidates,
    safetyNotes: optiScalerSafetyNotes(catalog.id, context.t),
  }
}

function cheekySafetyNotes(t: Translate): string[] {
  return [
    t('cheekySafetyReshade'),
    t('cheekySafetyOpenXr'),
    t('cheekySafetyOneIntegration'),
    t('cheekySafetyEnableDlss'),
  ]
}

function buildCheekyRequest(context: ModuleRequestContext): CheekyFoveatedDlssRequest | null {
  const { module } = context
  if (module.id !== 'cheeky-foveated-dlss' || !context.game) return null

  const { catalog, installed } = context.game
  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  const addonFile = typeof config.addonFile === 'string' ? config.addonFile : 'CheekyFoveatedDLSS.addon64'
  if (!version || !downloadUrl || !sha256 || !addonFile) return null

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executable: catalog.executable,
    version,
    downloadUrl,
    sha256,
    addonFile,
    safetyNotes: [
      ...cheekySafetyNotes(context.t),
      cheekyCompatibilityNote(module, catalog.id, context.t),
    ].filter(Boolean),
  }
}

function buildUevrRequest(context: ModuleRequestContext): UevrRequest | null {
  const { module } = context
  if (module.id !== 'uevr' || !context.game) return null

  const { catalog, installed } = context.game
  const config = module.config ?? {}
  const backend = context.uevrBackend(module)
  const versionPolicy = config.versionPolicy === 'pinned' ? 'pinned' : 'latest'
  const releaseApiUrl = backend === 'afw' || backend === 'joey-afw'
    ? (typeof config.afwReleaseApiUrl === 'string'
      ? config.afwReleaseApiUrl
      : 'https://api.github.com/repos/PureDark/UEVR/releases/latest')
    : (typeof config.releaseApiUrl === 'string'
      ? config.releaseApiUrl
      : 'https://api.github.com/repos/praydog/UEVR-nightly/releases/latest')
  const backendReleaseApiUrl = typeof config.backendReleaseApiUrl === 'string'
    ? config.backendReleaseApiUrl
    : null
  const releaseTag = typeof config.releaseTag === 'string' && config.releaseTag.trim()
    ? config.releaseTag
    : null
  const backendReleaseTag = typeof config.backendReleaseTag === 'string' && config.backendReleaseTag.trim()
    ? config.backendReleaseTag
    : null

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executable: catalog.executable,
    backend,
    versionPolicy,
    releaseApiUrl,
    releaseTag,
    backendReleaseApiUrl,
    backendReleaseTag,
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

/**
 * The OFXR bridge's request.
 *
 * Exported on its own because the VR launch carries it: the launch arms
 * an already-installed bridge instead of installing one, so it needs the
 * same request the OFXR card would build, from the same recipe.
 */
export function buildOfxrRequest(context: ModuleRequestContext): OfxrRequest | null {
  const { module } = context
  if (module.id !== 'ofxr-framegen' || !context.game) return null

  const { catalog, installed } = context.game
  const config = module.config ?? {}
  const version = typeof config.version === 'string' ? config.version : null
  const implementationVersion = typeof config.implementationVersion === 'string'
    ? Number(config.implementationVersion)
    : null
  const downloadUrl = typeof config.downloadUrl === 'string' ? config.downloadUrl : null
  const sha256 = typeof config.sha256 === 'string' ? config.sha256 : null
  const backend = typeof config.backend === 'string' ? config.backend : 'fidelityfx'
  const nvidiaPreset = typeof config.nvidiaPreset === 'string' ? config.nvidiaPreset : 'medium'
  const nvidiaInputScale = typeof config.nvidiaInputScale === 'string' ? Number(config.nvidiaInputScale) : 50
  if (!version || !implementationVersion || !downloadUrl || !sha256 || !Number.isFinite(nvidiaInputScale)) return null

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executable: catalog.executable,
    version,
    implementationVersion,
    downloadUrl,
    sha256,
    backend,
    nvidiaPreset,
    nvidiaInputScale,
    nvidiaBidirectional: configBool(config, 'nvidiaBidirectional'),
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

function buildVrLaunchRequest(context: ModuleRequestContext): VrLaunchRequest | null {
  const { module } = context
  if (module.id !== 'vr-launch' || !context.game) return null

  const { catalog, installed } = context.game
  const config = module.config ?? {}
  const configPath = typeof config.configPath === 'string' ? config.configPath : null
  const configPatches = configList(config, 'configPatches').flatMap((value) => {
    const [section, key, ...parts] = value.split('|')
    if (!section || !key || !parts.length) return []
    return [{ section, key, value: parts.join('|') }]
  })
  const recommendations = configList(config, 'recommendations').map((value) => {
    const separator = value.indexOf('=')
    if (separator < 0) return { label: value, value: '' }
    return { label: value.slice(0, separator), value: value.slice(separator + 1) }
  })

  return {
    gameId: catalog.id,
    gameName: catalog.name,
    installDir: installed.installDir,
    executable: catalog.executable,
    arguments: configList(config, 'arguments'),
    requiredFiles: configList(config, 'requiredFiles'),
    configPath,
    configPatches,
    recommendations,
    safetyNotes: configList(config, 'safetyNotes'),
  }
}

// ---------------------------------------------------------------------------
// The table
// ---------------------------------------------------------------------------

/**
 * Build the request, ask the backend, read the answer as a checklist.
 *
 * A module whose recipe is missing the values its request needs (no
 * download URL, no proxy candidate) has no preview to read, and saying
 * so is the point: that is the card that used to install nothing.
 */
function verificationFor<D extends ModuleDialogName>(
  probe: VerificationProbe<D>,
  buildRequest: (context: ModuleRequestContext) => ModuleRequestByDialog[D] | null,
  preview: (request: ModuleRequestByDialog[D]) => Promise<ModulePreviewByDialog[D]>,
) {
  return async (context: ModuleRequestContext, t: Translate): Promise<VerificationOutcome> => {
    const request = buildRequest(context)
    if (!request) throw new Error(t('moduleNoAction', { module: context.module.id }))
    return probe(await preview(request), request, t)
  }
}

/**
 * The module's own uninstall command. A recipe that cannot be built has
 * nothing to uninstall, and saying so beats rolling back a transaction
 * recorded from a different install.
 */
function remover<Request>(
  buildRequest: (context: ModuleRequestContext) => Request | null,
  uninstall: (request: Request) => Promise<TransactionRecord>,
) {
  return async (context: ModuleRequestContext): Promise<void> => {
    const request = buildRequest(context)
    if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
    await uninstall(request)
  }
}

/**
 * Every module the library grid can act on itself, in card order.
 *
 * Each builder re-checks the module id, so an entry can never answer with
 * another module's request even if the table is asked for the wrong id.
 */
export const LIBRARY_MODULE_ENTRIES = [
  {
    id: 'vr-launch',
    dialog: 'vr-launch',
    previewCommand: 'preview_vr_launch',
    installCommand: 'launch_vr_game',
    // A VR launch profile is undone by rolling its last transaction back.
    uninstallCommand: null,
    transactionKind: 'vr-launch',
    buildRequest: (context: ModuleRequestContext) => buildVrLaunchRequest(context),
    preview: (request: VrLaunchRequest) => previewVrLaunch(request),
    openPreview: async (context) => {
      const request = buildVrLaunchRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'vr-launch', request, preview: await previewVrLaunch(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES['vr-launch'],
      (context: ModuleRequestContext) => buildVrLaunchRequest(context),
      (request: VrLaunchRequest) => previewVrLaunch(request),
    ),
    // Rolling the launch transaction back is the whole of the undo.
    remove: null,
  },
  {
    id: 'obs-vr',
    dialog: 'obs-vr',
    previewCommand: 'preview_obs_vr',
    installCommand: 'configure_obs_vr',
    uninstallCommand: 'uninstall_obs_vr',
    transactionKind: 'obs-vr',
    buildRequest: (context: ModuleRequestContext) => buildObsRequest(context),
    preview: (request: ObsVrRequest) => previewObsVr(request),
    openPreview: async (context) => {
      const request = buildObsRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'obs-vr', request, preview: await previewObsVr(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES['obs-vr'],
      (context: ModuleRequestContext) => buildObsRequest(context),
      (request: ObsVrRequest) => previewObsVr(request),
    ),
    remove: remover(
      (context: ModuleRequestContext) => buildObsRequest(context),
      (request: ObsVrRequest) => uninstallObsVr(request),
    ),
  },
  {
    id: 'optiscaler',
    dialog: 'optiscaler',
    previewCommand: 'preview_optiscaler',
    installCommand: 'install_optiscaler',
    uninstallCommand: 'uninstall_optiscaler',
    transactionKind: 'optiscaler',
    buildRequest: (context: ModuleRequestContext) => buildOptiScalerRequest(context),
    preview: (request: OptiScalerRequest) => previewOptiScaler(request),
    openPreview: async (context) => {
      const request = buildOptiScalerRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'optiscaler', request, preview: await previewOptiScaler(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES.optiscaler,
      (context: ModuleRequestContext) => buildOptiScalerRequest(context),
      (request: OptiScalerRequest) => previewOptiScaler(request),
    ),
    remove: remover(
      (context: ModuleRequestContext) => buildOptiScalerRequest(context),
      (request: OptiScalerRequest) => uninstallOptiScaler(request),
    ),
  },
  {
    id: 'ofxr-framegen',
    dialog: 'ofxr',
    previewCommand: 'preview_ofxr',
    installCommand: 'install_ofxr',
    uninstallCommand: 'uninstall_ofxr',
    transactionKind: 'ofxr-framegen',
    buildRequest: (context: ModuleRequestContext) => buildOfxrRequest(context),
    preview: (request: OfxrRequest) => previewOfxr(request),
    openPreview: async (context) => {
      const request = buildOfxrRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'ofxr', request, preview: await previewOfxr(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES.ofxr,
      (context: ModuleRequestContext) => buildOfxrRequest(context),
      (request: OfxrRequest) => previewOfxr(request),
    ),
    remove: remover(
      (context: ModuleRequestContext) => buildOfxrRequest(context),
      (request: OfxrRequest) => uninstallOfxr(request),
    ),
  },
  {
    id: 'cheeky-foveated-dlss',
    dialog: 'cheeky',
    previewCommand: 'preview_cheeky_foveated_dlss',
    installCommand: 'install_cheeky_foveated_dlss',
    uninstallCommand: 'uninstall_cheeky_foveated_dlss',
    transactionKind: 'cheeky-foveated-dlss',
    buildRequest: (context: ModuleRequestContext) => buildCheekyRequest(context),
    preview: (request: CheekyFoveatedDlssRequest) => previewCheekyFoveatedDlss(request),
    openPreview: async (context) => {
      const request = buildCheekyRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'cheeky', request, preview: await previewCheekyFoveatedDlss(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES.cheeky,
      (context: ModuleRequestContext) => buildCheekyRequest(context),
      (request: CheekyFoveatedDlssRequest) => previewCheekyFoveatedDlss(request),
    ),
    remove: remover(
      (context: ModuleRequestContext) => buildCheekyRequest(context),
      (request: CheekyFoveatedDlssRequest) => uninstallCheekyFoveatedDlss(request),
    ),
  },
  {
    id: 'uevr',
    dialog: 'uevr',
    previewCommand: 'preview_uevr',
    installCommand: 'install_uevr',
    uninstallCommand: 'uninstall_uevr',
    transactionKind: 'uevr',
    buildRequest: (context: ModuleRequestContext) => buildUevrRequest(context),
    preview: (request: UevrRequest) => previewUevr(request),
    openPreview: async (context) => {
      const request = buildUevrRequest(context)
      if (!request) throw new Error(context.t('moduleNoAction', { module: context.module.id }))
      return { dialog: 'uevr', request, preview: await previewUevr(request) }
    },
    verify: verificationFor(VERIFICATION_PROBES.uevr,
      (context: ModuleRequestContext) => buildUevrRequest(context),
      (request: UevrRequest) => previewUevr(request),
    ),
    remove: remover(
      (context: ModuleRequestContext) => buildUevrRequest(context),
      (request: UevrRequest) => uninstallUevr(request),
    ),
  },
] as const satisfies readonly ModuleEntry[]

/**
 * Modules the grid dispatches to another feature. `desktop-shortcut` is
 * installed and removed through `useDesktopShortcut`, which owns its
 * dialog; the row is here so the card has a declared, live path instead
 * of a button that quietly does nothing.
 */
export const DELEGATED_MODULE_ENTRIES = [
  {
    id: 'desktop-shortcut',
    delegate: 'desktop-shortcut',
    previewCommand: 'preview_desktop_shortcut',
    installCommand: 'create_desktop_shortcut',
    // Undoing a shortcut is rolling its transaction back, like a launch.
    uninstallCommand: null,
    transactionKind: 'desktop-shortcut',
  },
] as const satisfies readonly DelegatedModuleEntry[]

export const ALL_MODULE_ENTRIES: readonly AnyModuleEntry[] = [
  ...LIBRARY_MODULE_ENTRIES,
  ...DELEGATED_MODULE_ENTRIES,
]

/** The table row for a module id, or undefined when the grid does not own it. */
export function moduleEntry(id: string): AnyModuleEntry | undefined {
  return ALL_MODULE_ENTRIES.find((entry) => entry.id === id)
}

/**
 * The dialog-backed row for a module id, if the grid invokes it
 * directly. Callers that only have a `string` id get the whole union
 * back, which is the point: a `switch` on `entry.dialog` has to narrow
 * before anything can be sent to the backend.
 */
export function libraryModuleEntry(id: string): ModuleEntry | undefined {
  return LIBRARY_MODULE_ENTRIES.find((entry) => entry.id === id)
}

/** Transaction kind a module's changes are recorded under, if it has one. */
export function moduleTransactionKind(id: string): string | null {
  return moduleEntry(id)?.transactionKind ?? null
}
