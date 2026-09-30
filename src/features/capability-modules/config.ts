import { z } from 'zod'
import { resolveCatalogConfig } from '../../services/catalog'
import type { ConfigFieldSpec } from '../../types/capability'
import type { CapabilityConfigValue } from './types'

/**
 * One resolver for "what values does this recipe need, for this game".
 *
 * Three callers install capability recipes — the game page's cards, the
 * Community panel and the collection run — and all three used to invent
 * their own answer. Two of them invented `{}`. A recipe's `configSchema`
 * says which fields it needs; the **catalogue** says what this game's
 * build of it needs, in the `config:` block beside the module the player
 * was shown. Those two facts together are the whole answer, and this
 * module is the only place they are joined.
 *
 * What this will not do is invent a value. A required field the catalogue
 * does not carry and the recipe does not default is reported in
 * `missingRequired`, because the alternative — sending a blank and
 * letting the backend fail inside a `download-file` step — is the
 * failure this was written to remove.
 */

/** The `type` tags this build renders a typed input for. */
const KNOWN_FIELD_TYPES = new Set<string>([
  'string',
  'number',
  'boolean',
  'url',
  'sha256',
  'path',
  'enum',
])

/**
 * Wire shape of a recipe's `configSchema`, as it arrives from Rust
 * (`capability_get` deserialises into it) and from the signed community
 * catalogue (`configSchema: unknown[]`, an unvalidated JSON array).
 * Mirrors `crate::capability::ConfigFieldSpec`.
 */
const configFieldSchema = z.object({
  name: z.string().min(1),
  // Rust treats `type` as a UI hint rather than an enum, so an id this
  // build has not seen is rendered as a text input instead of failing
  // the whole schema — losing every field because one tag is new would
  // be worse than showing it as a string.
  type: z
    .string()
    .transform((value) => (KNOWN_FIELD_TYPES.has(value) ? (value as ConfigFieldSpec['type']) : 'string')),
  required: z.boolean().optional(),
  default: z.union([z.string(), z.number(), z.boolean()]).optional(),
  enumValues: z.array(z.string()).optional(),
  description: z.string().optional(),
})

/**
 * A recipe's `configSchema` from an untrusted JSON value, or `[]` when
 * it is not the shape this build understands. The Community panel uses
 * it on the signed catalogue's own copy of the schema, which is the only
 * schema a community recipe ever has here: the YAML behind its
 * `downloadUrl` is fetched and parsed by the backend at install time, not
 * by the UI.
 *
 * Reading is lossy on purpose at the *field* level (an unknown `type`
 * becomes a string input) and at the *array* level too. A caller that
 * installs from this needs the strict answer instead — see
 * `readConfigSchema`.
 */
export function parseConfigSchema(raw: unknown): ConfigFieldSpec[] {
  const read = readConfigSchema(raw)
  return read.known ? read.schema : []
}

/**
 * A schema, or the reason there isn't a readable one.
 *
 * `known: false` is not an error case to be smoothed over: it is the
 * only honest answer for a `configSchema` this build cannot interpret,
 * and an installer that treats it as "no fields" sends a blank it
 * cannot justify. Callers that install refuse on it.
 */
export type ConfigSchemaRead =
  | { known: true; schema: ConfigFieldSpec[] }
  | { known: false; reason: string }

/**
 * Read a `configSchema` without guessing at a malformed one.
 *
 * An absent key is a statement, not a defect: Rust defaults
 * `CapabilitySpec::config_schema` to an empty `Vec` and the community
 * catalogue entry's `serde_json::Value` to `null`, so a recipe that
 * declares no config arrives as `null`, `[]` or missing — all of which
 * mean the same thing, "this recipe has no fields". Anything else that
 * fails to parse is a schema this build cannot vouch for, and comes back
 * as `known: false` rather than as an empty list.
 */
export function readConfigSchema(raw: unknown): ConfigSchemaRead {
  if (raw === undefined || raw === null) return { known: true, schema: [] }
  const parsed = z.array(configFieldSchema).safeParse(raw)
  if (parsed.success) return { known: true, schema: parsed.data }
  const issue = parsed.error.issues[0]
  const where = issue?.path.length ? issue.path.join('.') : 'configSchema'
  return { known: false, reason: `${where}: ${issue?.message ?? 'unreadable schema'}` }
}

/** The empty value a field of this type starts from. */
export function blankFor(field: ConfigFieldSpec): CapabilityConfigValue {
  if (field.default !== undefined) return field.default
  if (field.type === 'boolean') return false
  if (field.type === 'number') return 0
  return ''
}

/**
 * Seed values for a recipe's fields, in the order a person would expect
 * to find them filled in: what the user already has, then the
 * catalogue's answer for this game, then the recipe's own default, then
 * the type's blank.
 *
 * `current` wins so a form stays overridable — this seeds a form, it
 * does not overwrite one.
 */
export function seedConfigValues(
  schema: readonly ConfigFieldSpec[],
  catalogue: Record<string, string | string[]> = {},
  current: Record<string, CapabilityConfigValue> = {},
): Record<string, CapabilityConfigValue> {
  const values: Record<string, CapabilityConfigValue> = {}
  for (const field of schema) {
    const existing = current[field.name]
    if (existing !== undefined && existing !== '') {
      values[field.name] = existing
      continue
    }
    const fromCatalogue = catalogue[field.name]
    values[field.name] = fromCatalogue !== undefined ? fromCatalogue : blankFor(field)
  }
  return values
}

/**
 * Required fields that still have no value. An empty string counts as
 * missing, a `false` does not: a boolean nobody ticked is a real answer
 * to "do this", and treating it as absent would make an uncheckable
 * recipe un-installable.
 */
export function missingRequiredConfigFields(
  schema: readonly ConfigFieldSpec[],
  values: Record<string, CapabilityConfigValue>,
): string[] {
  return schema
    .filter((field) => {
      if (!field.required) return false
      const value = values[field.name]
      return value === undefined || value === '' || (Array.isArray(value) && value.length === 0)
    })
    .map((field) => field.name)
}

export interface CapabilityConfigResolution {
  /** What to send as `config.values`. */
  values: Record<string, CapabilityConfigValue>
  /** Required fields still empty. Non-empty means "needs input". */
  missingRequired: string[]
}

/**
 * The single entry point: the config a `(game, capability)` install
 * should carry, and what — if anything — the user still has to fill in.
 */
export function resolveCapabilityConfig(options: {
  gameId: string | null | undefined
  capabilityId: string
  schema: readonly ConfigFieldSpec[]
  current?: Record<string, CapabilityConfigValue>
}): CapabilityConfigResolution {
  const values = seedConfigValues(
    options.schema,
    resolveCatalogConfig(options.gameId, options.capabilityId),
    options.current,
  )
  return { values, missingRequired: missingRequiredConfigFields(options.schema, values) }
}
