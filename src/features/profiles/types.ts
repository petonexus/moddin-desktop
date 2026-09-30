import { z } from 'zod'
import type { CapabilitySpec, CapabilitySummary, EngineId } from '../../types/capability'
import type { TransactionRecord } from '../../types/transaction'
import type { CapabilityConfigValue } from '../capability-modules/types'

/**
 * The exported profile format.
 *
 * A profile is a file the user will email to themselves, so the shape
 * answers three questions and nothing else: which games, which
 * capabilities per game, and what each one was configured with. It
 * carries no binaries, no DLLs and no install instructions — the recipe
 * still comes from the registry on the receiving machine, exactly as an
 * import has to re-run the same checks a card install does.
 *
 * It is JSON, not the YAML the older `src/services/profile.ts` used,
 * because a profile is machine-written and machine-read while capability
 * recipes are hand-written by a person or an AI. YAML stays the recipe
 * format; JSON is the document format. The `kind` marker and the
 * `gameId`/module pairing are kept so the two are recognisably the same
 * family of file.
 */
export const PROFILE_KIND = 'moddin-profile'

/**
 * The shape of the document. Bump this when the format changes and keep
 * reading the versions below: a profile exported months ago has to keep
 * opening, and there is no way to un-ship a file the user already has.
 */
export const PROFILE_SCHEMA_VERSION = 1

/** Oldest schema this build can still read. Bump only when a change
 *  genuinely cannot be interpreted, not when one is merely optional. */
export const PROFILE_MIN_SCHEMA_VERSION = 1

const configValuesSchema = z.record(
  z.string(),
  z.union([z.string(), z.number(), z.boolean(), z.array(z.string())]),
)

export const profileCapabilitySchema = z.object({
  id: z.string().min(1),
  displayName: z.string().min(1),
  config: z.object({ values: configValuesSchema }),
  /**
   * Field names the secrets rule withheld on export. Carried in the file
   * so the import preview can say "these are not in here" instead of
   * silently installing a default the user never chose.
   */
  omittedSecrets: z.array(z.string()).default([]),
})

export const profileGameSchema = z.object({
  gameId: z.string().min(1),
  gameName: z.string().min(1),
  capabilities: z.array(profileCapabilitySchema),
})

export const profileDocumentSchema = z.object({
  kind: z.literal(PROFILE_KIND),
  schemaVersion: z.number().int().positive(),
  /** Informational. Never a compatibility gate: a profile exported by a
   *  newer Moddin still has to open if its schema is readable. */
  appVersion: z.string().min(1),
  /** Informational, ISO-8601. */
  exportedAt: z.string().min(1),
  games: z.array(profileGameSchema),
})

export type ProfileCapabilityEntry = z.infer<typeof profileCapabilitySchema>
export type ProfileGameEntry = z.infer<typeof profileGameSchema>
export type ModdinProfileDocument = z.infer<typeof profileDocumentSchema>

/**
 * Why a capability named by a profile cannot be installed here.
 *
 * Every one of these is decided during the preview. An import that
 * reports a problem only after it has started writing into a game folder
 * is the exact surprise this project keeps removing.
 */
export type ProfileBlockReason =
  /** The profile names a game this machine does not have installed. */
  | 'game-not-installed'
  /** No capability with this id exists in this build's registry. */
  | 'unknown-capability'
  /** A community capability. Never installed from a profile — see the
   *  note on `planProfileImport`. */
  | 'community-capability'
  /** On the community kill switch. */
  | 'revoked'
  /** The spec's `supportedEngines` excludes the detected engine. */
  | 'engine-mismatch'
  /** The engine could not be detected, so compatibility is unproven. */
  | 'engine-unknown'

/** A resolved install target, or `null` when the game is not installed. */
export interface ProfileGameTarget {
  gameId: string
  gameName: string
  installDir: string
  executableDir: string
  engine: string | null
  engineVersion: string | null
}

export interface ProfileImportContext {
  /** `capability_list`, by id. Its presence is what "exists" means. */
  capabilities: Map<string, CapabilitySummary>
  /** `capability_get` for the ids the file names. */
  specs: Map<string, CapabilitySpec>
  /** Applied capability installs per game, derived from the transaction log. */
  installedByGame: Map<string, Set<string>>
  /** `null` for a game that is not installed on this machine. */
  targets: Map<string, ProfileGameTarget | null>
  /** Community kill switch, id -> reason. */
  revoked: Map<string, string>
}

export interface ProfileCapabilityPlan {
  id: string
  displayName: string
  /** `install` when Moddin has no applied transaction for it, `reinstall`
   *  when it does — both go through the same install command. */
  action: 'install' | 'reinstall'
  blocked: boolean
  reasons: ProfileBlockReason[]
  /** Withheld by the secrets rule, from the file or from the live spec. */
  omittedSecrets: string[]
  /** What would actually be sent to `capability_install`. */
  config: Record<string, CapabilityConfigValue>
}

export interface ProfileGamePlan {
  gameId: string
  gameName: string
  installed: boolean
  capabilities: ProfileCapabilityPlan[]
}

export interface ProfileImportPlan {
  games: ProfileGamePlan[]
  /** Capabilities that would be installed by pressing Apply. */
  applyCount: number
  /** Capabilities named by the file that will not be applied. */
  blockedCount: number
  exportedAt: string
  schemaVersion: number
}

/**
 * Why a file was refused, as a code rather than a sentence.
 *
 * The message the user reads is translated in the view; a parse failure
 * that only exists as an English string is a string no other locale can
 * improve, and a zod error dump is not a message at all.
 */
export type ProfileParseErrorCode =
  | 'not-json'
  | 'not-profile'
  | 'future-version'
  | 'old-version'
  | 'invalid'

export type ProfileParseResult =
  | { ok: true; document: ModdinProfileDocument }
  | { ok: false; code: ProfileParseErrorCode; detail: string }

/**
 * Read a profile document, refusing anything this build cannot honestly
 * interpret.
 *
 * The schema version is checked *before* the shape, on purpose: a
 * document from a newer Moddin is well-formed JSON and would otherwise
 * be read as a v1 file with fields missing, which is a silent misread
 * rather than a refusal.
 */
export function parseProfileDocument(raw: string): ProfileParseResult {
  let parsed: unknown
  try {
    parsed = JSON.parse(raw)
  } catch (error) {
    return {
      ok: false,
      code: 'not-json',
      detail: error instanceof Error ? error.message : String(error),
    }
  }
  if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
    return { ok: false, code: 'not-profile', detail: '' }
  }

  const candidate = parsed as Record<string, unknown>
  if (candidate.kind !== PROFILE_KIND) {
    return { ok: false, code: 'not-profile', detail: String(candidate.kind ?? '') }
  }

  const version = candidate.schemaVersion
  if (typeof version !== 'number' || !Number.isInteger(version)) {
    return { ok: false, code: 'invalid', detail: String(version ?? '') }
  }
  if (version > PROFILE_SCHEMA_VERSION) {
    return { ok: false, code: 'future-version', detail: String(version) }
  }
  if (version < PROFILE_MIN_SCHEMA_VERSION) {
    return { ok: false, code: 'old-version', detail: String(version) }
  }

  const result = profileDocumentSchema.safeParse(candidate)
  if (!result.success) {
    const first = result.error.issues[0]
    return {
      ok: false,
      code: 'invalid',
      detail: first ? `${first.path.join('.') || '(root)'}: ${first.message}` : '',
    }
  }
  return { ok: true, document: result.data }
}

/**
 * A value that names somewhere on this machine rather than a setting.
 *
 * Checked for `string` fields only, which is where a recipe would store
 * a folder a user picked. `url` and `sha256` fields are public values by
 * construction and are never touched by this.
 */
export function looksLikeLocalPath(value: string): boolean {
  return (
    /^[a-zA-Z]:[\\/]/.test(value)
    || value.startsWith('\\\\')
    || value.startsWith('//')
    || /^~\//.test(value)
    || /\\Users\\/i.test(value)
  )
}

/**
 * The secrets rule, in one place, applied on both sides.
 *
 * A profile leaves this machine and comes back from an inbox, so it may
 * not carry anything that points at this user's disk. A capability
 * config field is therefore exported only when all three hold:
 *
 *  1. the capability's own `configSchema` declares it — a field Moddin
 *     does not know the meaning of is dropped rather than guessed at;
 *  2. its declared type is not `path` — that is the one type whose whole
 *     purpose is naming a location outside the game folder;
 *  3. a `string` value is not itself an absolute or home-relative path,
 *     because a recipe that stores a folder in a `string` field leaks the
 *     same thing.
 *
 * Withheld names are returned, not silently dropped, so the file and the
 * preview can both say which settings are missing. Re-running this on the
 * import side with the *live* spec means a hand-edited profile cannot
 * widen the rule: whatever it claims, only fields the local recipe
 * declares and permits are sent to the installer.
 */
export function screenConfigValues(
  spec: CapabilitySpec | null | undefined,
  values: Record<string, CapabilityConfigValue>,
): { values: Record<string, CapabilityConfigValue>; omitted: string[] } {
  const allowed: Record<string, CapabilityConfigValue> = {}
  const omitted: string[] = []
  const fields = new Map((spec?.configSchema ?? []).map((field) => [field.name, field]))

  for (const [name, value] of Object.entries(values)) {
    const field = fields.get(name)
    if (!field || field.type === 'path') {
      omitted.push(name)
      continue
    }
    if (typeof value === 'string' && looksLikeLocalPath(value)) {
      omitted.push(name)
      continue
    }
    allowed[name] = value
  }

  return { values: allowed, omitted }
}

export interface ProfileExportInput {
  appVersion: string
  exportedAt: string
  games: Array<{
    gameId: string
    gameName: string
    capabilities: Array<{
      id: string
      displayName: string
      spec: CapabilitySpec | null | undefined
      values: Record<string, CapabilityConfigValue>
    }>
  }>
}

/** Build the document. The only place a profile is ever written. */
export function buildProfileDocument(input: ProfileExportInput): ModdinProfileDocument {
  return {
    kind: PROFILE_KIND,
    schemaVersion: PROFILE_SCHEMA_VERSION,
    appVersion: input.appVersion,
    exportedAt: input.exportedAt,
    games: input.games.map((game) => ({
      gameId: game.gameId,
      gameName: game.gameName,
      capabilities: game.capabilities.map((capability) => {
        const screened = screenConfigValues(capability.spec, capability.values)
        return {
          id: capability.id,
          displayName: capability.displayName,
          config: { values: screened.values },
          omittedSecrets: screened.omitted,
        }
      }),
    })),
  }
}

/**
 * Map the engine `inspect_game_environment` reports onto the id a
 * `supportedEngines` entry uses (`EngineId`).
 *
 * The two vocabularies are different on purpose — one is written for a
 * player, the other is the stable machine-readable id a recipe declares
 * — and Unreal is the one that cannot be rounded. A game is only
 * `unreal5` when the executable actually says `UE5`; an `UE4` build, or
 * an Unreal game whose version marker was not found, stays `null`. That
 * is the same rule the capability card's engine gate uses: an engine
 * Moddin cannot identify is never treated as a supported one.
 */
export function engineSlug(engine: string | null, engineVersion: string | null): EngineId | null {
  if (!engine) return null
  switch (engine.trim().toLowerCase()) {
    case 'unity':
      return 'unity'
    case 'unreal engine':
      return engineVersion === 'UE5' ? 'unreal5' : null
    case 're engine':
      return 're-engine'
    case 'redengine':
      return 'redengine'
    default:
      return null
  }
}

/**
 * Decide, per game and per capability, what an import would do — before
 * anything is applied.
 *
 * Two rules are worth stating outright:
 *
 *  - A community capability is never installed from a profile. The
 *    community path re-verifies the catalog signature and the kill
 *    switch inside the install itself, and `capability_install` does
 *    neither. Sending a profile through `capability_install` would be a
 *    second, weaker install path, which is the one thing this app does
 *    not do.
 *  - Blocking is per capability, not per file. A profile naming one
 *    withdrawn mod still applies the rest, and says what it skipped.
 */
export function planProfileImport(
  document: ModdinProfileDocument,
  context: ProfileImportContext,
): ProfileImportPlan {
  const games: ProfileGamePlan[] = document.games.map((game) => {
    const target = context.targets.get(game.gameId) ?? null
    const installedHere = new Set(context.installedByGame.get(game.gameId) ?? [])
    const slug = engineSlug(target?.engine ?? null, target?.engineVersion ?? null)

    const capabilities = game.capabilities.map<ProfileCapabilityPlan>((entry) => {
      const summary = context.capabilities.get(entry.id)
      const spec = context.specs.get(entry.id) ?? null
      const reasons: ProfileBlockReason[] = []

      if (!target) reasons.push('game-not-installed')
      if (!summary) reasons.push('unknown-capability')
      if (summary?.origin === 'community') reasons.push('community-capability')
      if (context.revoked.has(entry.id)) reasons.push('revoked')
      if (summary && target && spec?.supportedEngines?.length) {
        // An undetected engine is never treated as a supported one —
        // the same stance the UEVR gate takes.
        if (!slug) reasons.push('engine-unknown')
        else if (!spec.supportedEngines.includes(slug)) reasons.push('engine-mismatch')
      }

      // The file's values go through the same rule as an export, against
      // the local recipe, so a hand-edited document cannot smuggle a
      // path past it.
      const screened = screenConfigValues(spec, entry.config.values)
      const omittedSecrets = [...new Set([...entry.omittedSecrets, ...screened.omitted])]

      return {
        id: entry.id,
        displayName: entry.displayName || summary?.displayName || entry.id,
        action: installedHere.has(entry.id) ? 'reinstall' : 'install',
        blocked: reasons.length > 0,
        reasons,
        omittedSecrets,
        config: screened.values,
      }
    })

    return {
      gameId: game.gameId,
      gameName: game.gameName || target?.gameName || game.gameId,
      installed: Boolean(target),
      capabilities,
    }
  })

  const plans = games.flatMap((game) => game.capabilities)
  return {
    games,
    applyCount: plans.filter((plan) => !plan.blocked).length,
    blockedCount: plans.filter((plan) => plan.blocked).length,
    exportedAt: document.exportedAt,
    schemaVersion: document.schemaVersion,
  }
}

/** The entries `applyProfileImport` would install, in file order. */
export function applicableEntries(
  plan: ProfileImportPlan,
): Array<{ game: ProfileGamePlan; capability: ProfileCapabilityPlan }> {
  return plan.games.flatMap((game) =>
    game.capabilities
      .filter((capability) => !capability.blocked)
      .map((capability) => ({ game, capability })),
  )
}

/**
 * Which capabilities are installed for which game, read off the
 * transaction log.
 *
 * `kind` is the capability id for every install that went through
 * `capability_install`, which is the same thing the capability cards and
 * the backend's own `is_capability_installed` compare against. A
 * transaction for a legacy module (OBS VR, an older OptiScaler install)
 * is therefore simply not a capability, and an export skips it: there
 * would be no recipe for an import to re-run.
 */
export function installedCapabilitiesByGame(
  transactions: TransactionRecord[],
): Map<string, Set<string>> {
  const installed = new Map<string, Set<string>>()
  for (const transaction of transactions) {
    if (transaction.status !== 'applied') continue
    const ids = installed.get(transaction.gameId) ?? new Set<string>()
    ids.add(transaction.kind)
    installed.set(transaction.gameId, ids)
  }
  return installed
}
