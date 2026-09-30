import type ptBR from './locales/pt-BR'

/**
 * The UX-21 boundary: backend-authored text that reaches the UI as an
 * identifier, mapped to a locale key **at the service layer**.
 *
 * Four backend fields are the subject — `entry.action`, `check.label`,
 * `state.warnings` and `agent.detail` — plus the capability card's
 * `description`, which is the player-facing sentence every other string
 * on that card is read alongside. Each one was an English sentence or a
 * snake_case command name written in Rust, handed to a Vue template and
 * rendered verbatim, so a pt-BR or es user read the backend's English
 * in the middle of an otherwise translated screen.
 *
 * ## The rule
 *
 * A mapping is a table, and the table is exhaustive over the ids this
 * repository's backend can emit. `src/i18n/__tests__/backendIds.test.ts`
 * reads the sources those ids come from — `PERSISTENT_ACTION_COMMANDS`,
 * `openxr.rs`, `ai_assistant_setup.rs`, `src-tauri/capabilities/*.yaml`
 * — and fails when an id appears there and not here. That is the whole
 * point of the item: an id with no entry must be a red test, not a
 * silent pass-through that leaves the string half-translated.
 *
 * ## The fallback, and why
 *
 * An id this table does not declare renders **the backend's own text**,
 * verbatim, exactly as before UX-21. Not the id alone, not a blank, not
 * a refusal to render.
 *
 * The tempting alternative — render nothing for an unknown id — is
 * worse in every one of the four places this boundary is applied. A log
 * entry with no action name is an entry the user cannot triage; a
 * capability card with no description is a card they cannot choose
 * between; an OpenXR warning that says nothing about a missing manifest
 * is the one line that would have told them why VR does not start. The
 * backend's sentence is bad English, and bad English is still evidence.
 *
 * So the fallback is a last resort, not a licence: the parity tests are
 * what keep it from being reached for an id the backend is known to
 * send.
 *
 * ## Where the key lands
 *
 * `key` travels beside the original string rather than replacing it,
 * because the original is still needed twice: as the documented
 * fallback, and as the text the activity log's search box matches on
 * (a user who remembers `install_optiscaler` must still find the entry
 * after the row is showing a Portuguese label).
 */

/**
 * Any key in the pt-BR payload. The three locales are held to identical
 * key sets by `src/i18n.ts`, so a table that names a key cannot name one
 * that does not exist — `npm run typecheck` is the check.
 */
export type BackendTextKey = keyof typeof ptBR & string

/**
 * A backend string that has been through this boundary.
 *
 * `key` is absent — not null, not a placeholder — when the string is not
 * a declared id, and that absence is the fallback signal.
 */
export interface LocalizedText {
  /** The backend's own string. Kept: the fallback, and the search text. */
  text: string
  /** Present only when the string is a declared backend id. */
  key?: BackendTextKey
  /** Values for a key that interpolates a path or a command. */
  params?: Record<string, string>
}

/**
 * Own-property lookup, so an id that happens to be `toString` or
 * `constructor` cannot pick a prototype member up as a translation.
 */
function lookup<K extends string, V>(table: Readonly<Record<K, V>>, id: string): V | undefined {
  return Object.hasOwn(table, id) ? table[id as K] : undefined
}

/**
 * `entry.action` — the Tauri command name `src/services/activity-log.ts`
 * writes into the action log, and the text the log row shows as its
 * title.
 *
 * Closed set: `PERSISTENT_ACTION_COMMANDS` is the only thing that calls
 * `record_ui_action_log`, and the log file is the only reader. The test
 * asserts the two lists agree, so adding a persistent command without
 * adding a label here is a red build.
 */
export const ACTION_IDS = [
  'configure_obs_vr',
  'uninstall_obs_vr',
  'install_optiscaler',
  'uninstall_optiscaler',
  'install_ofxr',
  'uninstall_ofxr',
  'install_cheeky_foveated_dlss',
  'uninstall_cheeky_foveated_dlss',
  'install_uevr',
  'uninstall_uevr',
  'rollback_latest_module_transaction',
  'rollback_transaction',
  'launch_vr_game',
  'create_desktop_shortcut',
  'set_game_openxr_runtime',
  'set_system_openxr_runtime',
] as const

export type ActionId = (typeof ACTION_IDS)[number]

const ACTION_LABELS: Readonly<Record<ActionId, BackendTextKey>> = {
  configure_obs_vr: 'activityActionConfigureObs',
  uninstall_obs_vr: 'activityActionUninstallObs',
  install_optiscaler: 'activityActionInstallOptiscaler',
  uninstall_optiscaler: 'activityActionUninstallOptiscaler',
  install_ofxr: 'activityActionInstallOfxr',
  uninstall_ofxr: 'activityActionUninstallOfxr',
  install_cheeky_foveated_dlss: 'activityActionInstallCheeky',
  uninstall_cheeky_foveated_dlss: 'activityActionUninstallCheeky',
  install_uevr: 'activityActionInstallUevr',
  uninstall_uevr: 'activityActionUninstallUevr',
  rollback_latest_module_transaction: 'activityActionRollbackLatest',
  rollback_transaction: 'activityActionRollback',
  launch_vr_game: 'activityActionLaunchVr',
  create_desktop_shortcut: 'activityActionCreateShortcut',
  set_game_openxr_runtime: 'activityActionSetGameRuntime',
  set_system_openxr_runtime: 'activityActionSetSystemRuntime',
}

/** Action-log id to locale key. Undeclared id: no key, see the module note. */
export function actionText(action: string): LocalizedText {
  return { text: action, key: lookup(ACTION_LABELS, action) }
}

/**
 * `state.warnings` — the four sentences `openxr::inspect_openxr` pushes
 * when the machine's OpenXR state is not the one the user assumes.
 *
 * These are diagnoses, not decoration: each one names a registry value
 * or a missing manifest, which is exactly what the user has to check
 * next. Translating them is the point of the item.
 */
export const OPENXR_WARNING_IDS = [
  'Windows does not currently define an active OpenXR runtime.',
  'The Windows ActiveRuntime registry value points to a missing manifest file.',
  'The saved per-game OpenXR override points to a missing manifest and will be ignored until changed.',
  'No installed OpenXR runtimes were discovered.',
] as const

export type OpenXrWarningId = (typeof OPENXR_WARNING_IDS)[number]

const OPENXR_WARNING_LABELS: Readonly<Record<OpenXrWarningId, BackendTextKey>> = {
  'Windows does not currently define an active OpenXR runtime.':
    'openxrWarningNoActiveRuntime',
  'The Windows ActiveRuntime registry value points to a missing manifest file.':
    'openxrWarningActiveRuntimeMissing',
  'The saved per-game OpenXR override points to a missing manifest and will be ignored until changed.':
    'openxrWarningGameOverrideMissing',
  'No installed OpenXR runtimes were discovered.': 'openxrWarningNoRuntimes',
}

/** OpenXR warning to locale key. Undeclared warning: the backend's own text. */
export function openXrWarningText(warning: string): LocalizedText {
  return { text: warning, key: lookup(OPENXR_WARNING_LABELS, warning) }
}

/**
 * `agent.detail` — the one line under each Local AI agent's badge.
 *
 * `inspect_agent` in `ai_assistant_setup.rs` writes these as whole
 * sentences, and two of them interpolate a path or a command, so the
 * table is patterns rather than exact strings: the pattern is the
 * template, and the value it captures becomes the key's parameter.
 */
const AGENT_DETAIL_PATTERNS: ReadonlyArray<readonly [RegExp, BackendTextKey]> = [
  [/^AI tool is not installed on this PC\.$/, 'localAiDetailNotInstalled'],
  [/^AI tool is installed but its config directory was not found\.$/, 'localAiDetailNoConfigDir'],
  [/^Moddin MCP entry points at a different binary: (.+)$/, 'localAiDetailForeignBinary'],
  [/^Moddin MCP entry has no command field\.$/, 'localAiDetailNoCommand'],
  [/^AI tool installed at (.+)\. Click Connect to register Moddin\.$/, 'localAiDetailInstalledAt'],
]

/**
 * The plain (non-interpolated) detail ids, kept separate so the parity
 * test has a list it can hold to the Rust source. The two templates that
 * interpolate a value are checked through `agentDetailText` with a probe
 * substituted, because that is the only way to reach them.
 */
export const AGENT_DETAIL_IDS = [
  'AI tool is not installed on this PC.',
  'AI tool is installed but its config directory was not found.',
  'Moddin MCP entry points at a different binary: {}',
  'Moddin MCP entry has no command field.',
  'AI tool installed at {}. Click Connect to register Moddin.',
] as const

/**
 * Agent detail to locale key, with the interpolated path or command
 * carried through as a parameter.
 *
 * A read failure — `detect_moddin_entry` returning an `Err`, which
 * `inspect_agent` passes straight through as the detail — has no
 * template and no translation. It keeps the backend's message: it is a
 * real I/O error with a real path in it, not prose.
 */
export function agentDetailText(detail: string): LocalizedText {
  for (const [pattern, key] of AGENT_DETAIL_PATTERNS) {
    const match = pattern.exec(detail)
    if (match) {
      return match[1] === undefined
        ? { text: detail, key }
        : { text: detail, key, params: { value: match[1] } }
    }
  }
  return { text: detail }
}

/**
 * `check.label` — a capability recipe's check id to the label the card
 * should show for it.
 *
 * Only the checks this repository ships are declared. A community or
 * AI-authored recipe names its own checks, and its own English: that
 * text is the author's, not the backend's, and the app does not get to
 * rewrite what a person wrote. An undeclared check id therefore keeps
 * the recipe's own label — which is also why the key is looked up by
 * check id and not by label text: the id is the identifier, and the
 * label is data that happens to be English.
 */
export const CAPABILITY_CHECK_IDS = [
  'archive-reachable',
  'archive-sha256',
  'core-present',
  'doorstop-present',
  'engine-recognised',
  'per-game-runtime-set',
  'preloader-present',
  'proxy-present',
  'registry-writable',
  'repository-reachable',
  'tray-running',
] as const

export type CapabilityCheckId = (typeof CAPABILITY_CHECK_IDS)[number]

const CAPABILITY_CHECK_LABELS: Readonly<Record<CapabilityCheckId, BackendTextKey>> = {
  'archive-reachable': 'capabilityCheckArchiveReachable',
  'archive-sha256': 'capabilityCheckArchiveSha256',
  'core-present': 'capabilityCheckCorePresent',
  'doorstop-present': 'capabilityCheckDoorstopPresent',
  'engine-recognised': 'capabilityCheckEngineRecognised',
  'per-game-runtime-set': 'capabilityCheckPerGameRuntimeSet',
  'preloader-present': 'capabilityCheckPreloaderPresent',
  'proxy-present': 'capabilityCheckProxyPresent',
  'registry-writable': 'capabilityCheckRegistryWritable',
  'repository-reachable': 'capabilityCheckRepositoryReachable',
  'tray-running': 'capabilityCheckTrayRunning',
}

/**
 * Capability check id to locale key. Undeclared id (a community recipe's
 * own check): no key, and the recipe's own label is rendered.
 */
export function capabilityCheckLabel(checkId: string | null | undefined): BackendTextKey | undefined {
  return checkId ? lookup(CAPABILITY_CHECK_LABELS, checkId) : undefined
}

/**
 * The capability card's `description` — the one sentence that says what
 * the mod does for the player.
 *
 * Keyed by capability id, because that is the identifier the gallery,
 * the install paths and the profile importer all already agree on. The
 * shipped recipes under `src-tauri/capabilities/` are this project's own
 * copy, so they are translated; a recipe written by the community or by
 * the AI keeps the sentence its author wrote, for the same reason as
 * above.
 */
export const CAPABILITY_IDS = [
  'bepinex',
  'cheeky-foveated-dlss',
  'ofxr-bridge',
  'openxr-helpers',
  'optiscaler',
  'reframework',
  'reshade',
  'ue4ss',
  'uevr',
] as const

export type CapabilityId = (typeof CAPABILITY_IDS)[number]

const CAPABILITY_DESCRIPTIONS: Readonly<Record<CapabilityId, BackendTextKey>> = {
  bepinex: 'capabilityDescriptionBepinex',
  'cheeky-foveated-dlss': 'capabilityDescriptionCheekyFoveatedDlss',
  'ofxr-bridge': 'capabilityDescriptionOfxrBridge',
  'openxr-helpers': 'capabilityDescriptionOpenxrHelpers',
  optiscaler: 'capabilityDescriptionOptiscaler',
  reframework: 'capabilityDescriptionReframework',
  reshade: 'capabilityDescriptionReshade',
  ue4ss: 'capabilityDescriptionUe4ss',
  uevr: 'capabilityDescriptionUevr',
}

/** Capability id to locale key for its description. Undeclared id: no key. */
export function capabilityDescriptionKey(
  capabilityId: string,
): BackendTextKey | undefined {
  return lookup(CAPABILITY_DESCRIPTIONS, capabilityId)
}
