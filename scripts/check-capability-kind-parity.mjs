#!/usr/bin/env node
// Capability kind parity guard.
//
// The step and check kinds a recipe may use are declared in five
// places, in three repositories:
//
//   * src-tauri/src/builtin_steps.rs  -> builtin_steps::known_kinds()
//   * src-tauri/src/builtin_checks.rs -> builtin_checks::known_kinds()
//   * src/types/capability.ts         -> StepKind / CheckKind unions
//   * moddin-agent/schema/capability.schema.json -> step.kind / check.kind enums
//   * moddin-agent/src/mcp-server.mjs -> STEP_KINDS / CHECK_KINDS
//   * moddin-community-capabilities/scripts/validate_capability.py
//                                    -> KNOWN_STEP_KINDS / KNOWN_CHECK_KINDS
//
// They drifted once, and silently: `download-file` and `exe-version`
// shipped in the Rust runner while the community validator still
// rejected them, and the TypeScript union — which is what the AI/MCP
// authoring path types recipes against — was missing six of the eleven
// step kinds. A community author could not express the recipe that
// CONTRIBUTING.md documents as the minimum, and the validator failed a
// capability whose checks were perfectly valid.
//
// A fifth source was found already stale when the first four were
// brought into line: mcp-server.mjs keeps its own STEP_KINDS map,
// feeds it to the agent through get_step_kinds, and was not covered
// by this check. It was missing two step kinds and one check kind, and
// its `registry-write` entry documented param names the Rust step
// does not read — so an agent authoring a recipe through MCP would
// have produced a step that failed at install time with "key is
// required". Adding the kind lists was not enough; the map itself has
// to be checked, which is what the field-level check below does.
//
// This check makes that drift a red build instead of a surprise.
//
// ## Engines (ROADMAP F-09)
//
// `supportedEngines` was a fourth list of the same kind: a vocabulary
// ("unreal5", "redengine", …) that four places had to agree on, with
// none of them checking. The backend now gates a capability on it, so
// an id added to one place and not the others is not a cosmetic
// difference — it decides which cards a player is offered. Two things
// are compared here:
//
//   * the engine ids: src/catalog/engines/ (the data),
//     capability.rs (KNOWN_ENGINES), capability.ts (EngineId), and the
//     agent JSON schema (supportedEngines.items.enum);
//   * the verdicts of the engine gate: capability.rs (EngineMatch) and
//     capability.ts (EngineMatch), so the card cannot render a verdict
//     the backend never sends.
//
// The community repo's `validate_capability.py` is the known un-guarded
// source: it does not look at `supportedEngines` at all today. Adding
// the enum there is a change in that repository, not this one.
//
// Zero dependencies on purpose, like check-frontend-architecture.mjs:
// it has to run in CI before `npm ci` has necessarily succeeded, and
// parsing four small source files is not worth a dependency.

import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join } from 'node:path'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

function read(relative) {
  return readFileSync(join(root, relative), 'utf8')
}

function fail(message) {
  console.error(`Capability kind parity check failed: ${message}`)
  process.exitCode = 1
}

/** String literals inside the `&[ ... ]` body of a Rust `known_kinds()`. */
function rustKinds(relative, functionName) {
  const source = read(relative)
  const start = source.indexOf(`fn ${functionName}()`)
  if (start < 0) {
    fail(`could not find ${functionName}() in ${relative}`)
    return []
  }
  // The return type is `-> &'static [&'static str]`, which contains both a
  // `[` and a `]`. Start after the function body's `{` so that pair is not
  // mistaken for the array.
  const body = source.indexOf('{', start)
  if (body < 0) {
    fail(`could not find the body of ${functionName}() in ${relative}`)
    return []
  }
  const open = source.indexOf('[', body)
  const close = source.indexOf(']', open)
  if (open < 0 || close < 0) {
    fail(`could not read the array of ${functionName}() in ${relative}`)
    return []
  }
  return [...source.slice(open, close).matchAll(/"([^"]+)"/g)].map((match) => match[1])
}

/** Quoted members of a TypeScript string-literal union. */
function tsUnion(relative, typeName) {
  const source = read(relative)
  const start = source.indexOf(`export type ${typeName} =`)
  if (start < 0) {
    fail(`could not find the ${typeName} union in ${relative}`)
    return []
  }
  const end = source.indexOf('\nexport', start)
  const body = source.slice(start, end < 0 ? undefined : end)
  return [...body.matchAll(/'([^']+)'/g)].map((match) => match[1])
}

/** Entries of a Python set literal assigned to `NAME = { ... }`. */
function pythonSet(relative, name) {
  const source = read(relative)
  const start = source.indexOf(`${name} = {`)
  if (start < 0) {
    fail(`could not find ${name} in ${relative}`)
    return []
  }
  const close = source.indexOf('}', start)
  return [...source.slice(start, close).matchAll(/"([^"]+)"/g)].map((match) => match[1])
}

/** Members of a JSON Schema `enum` under a named `$defs` entry. */
function jsonSchemaEnum(relative, defName, property = 'kind') {
  const source = read(relative)
  let schema
  try {
    schema = JSON.parse(source)
  } catch (error) {
    fail(`could not parse ${relative}: ${error.message}`)
    return []
  }
  const values = schema?.$defs?.[defName]?.properties?.[property]?.enum
  if (!Array.isArray(values)) {
    fail(`could not find $defs.${defName}.properties.${property}.enum in ${relative}`)
    return []
  }
  return values
}

/**
 * Top-level keys of a `const NAME = Object.freeze({ "kind": { ... } })` map
 * in mcp-server.mjs. The keys sit at exactly two spaces of indentation;
 * the nested `summary` / `fields` / `touches` properties sit at four, so
 * anchoring on the indentation is enough to tell them apart.
 *
 * The scan has to stop at the literal's own closing brace. Both maps sit
 * at the same indentation in one file, so a scan that runs to EOF reads
 * CHECK_KINDS' keys as if they were STEP_KINDS'.
 */
function jsObjectKeys(relative, constName) {
  const source = read(relative)
  const start = source.indexOf(`const ${constName} = Object.freeze({`)
  if (start < 0) {
    fail(`could not find ${constName} in ${relative}`)
    return []
  }
  const close = source.indexOf('\n})', start)
  if (close < 0) {
    fail(`could not find the end of ${constName} in ${relative}`)
    return []
  }
  const body = source.slice(start, close)
  const keys = new Set()
  for (const match of body.matchAll(/^ {2}"([^"]+)":/gm)) {
    keys.add(match[1])
  }
  return [...keys]
}

/**
 * The `fields` array declared for one kind in mcp-server.mjs's STEP_KINDS
 * map. This is what an agent is told a step accepts, so it is the one
 * place where a wrong name becomes a recipe that fails at install time.
 */
function jsEntryFields(relative, constName, kind) {
  const source = read(relative)
  const start = source.indexOf(`const ${constName} = Object.freeze({`)
  if (start < 0) return null
  const keyAt = source.indexOf(`\n  "${kind}": {`, start)
  if (keyAt < 0) return null
  const fieldsAt = source.indexOf('fields:', keyAt)
  if (fieldsAt < 0) return null
  const open = source.indexOf('[', fieldsAt)
  const close = source.indexOf(']', open)
  if (open < 0 || close < 0) return null
  return [...source.slice(open, close).matchAll(/"([^"]+)"/g)].map((match) => match[1])
}

/**
 * Every param name the Rust runner knows, across the whole of
 * builtin_steps.rs.
 *
 * This is deliberately file-global rather than per-function. A
 * per-function scan was tried first and reported five false positives,
 * because several steps resolve their path through a shared helper
 * (`path_param`, `build_working_directory`, `declared_outputs`) whose
 * literals live in another function's body. A guard that cries wolf is
 * worse than no guard, because it teaches people to ignore it.
 *
 * The trade is that this cannot catch a real field attached to the
 * wrong kind. It does catch invented names, which is the failure that
 * was actually live: an agent told `registry-write` accepts
 * `keyField` authors a step the runner rejects with "key is required".
 */
function rustKnownParamNames() {
  const relative = 'src-tauri/src/builtin_steps.rs'
  const source = read(relative)
  const names = new Set(
    [...source.matchAll(/"([a-zA-Z][a-zA-Z0-9]*)"/g)].map((match) => match[1]),
  )
  if (names.size === 0) fail(`found no param names in ${relative}`)
  return names
}

/**
 * `id:` of every preset file under `src/catalog/engines/`. This is the
 * data the other engine lists are checked against: a vocabulary the
 * engine presets do not contain describes engines the app cannot offer.
 */
function enginePresetIds() {
  const dir = join(root, 'src/catalog/engines')
  if (!existsSync(dir)) {
    fail('could not find src/catalog/engines/')
    return []
  }
  const ids = []
  for (const file of readdirSync(dir).filter((name) => name.endsWith('.yaml')).sort()) {
    const source = readFileSync(join(dir, file), 'utf8')
    const match = source.match(/^id:[ \t]*"?([A-Za-z0-9_-]+)"?[ \t]*$/m)
    if (!match) {
      fail(`could not read an id from src/catalog/engines/${file}`)
      continue
    }
    ids.push(match[1])
  }
  return ids
}

/**
 * String literals inside a Rust `const NAME: &[&str] = &[...]`.
 *
 * The search for the literal's `[` starts after the `=`, because the
 * type annotation's own `&[&str]` contains a pair of brackets and would
 * otherwise be mistaken for an empty array.
 */
function rustConstList(relative, constName) {
  const source = read(relative)
  const name = source.indexOf(constName)
  if (name < 0) {
    fail(`could not find ${constName} in ${relative}`)
    return []
  }
  const assign = source.indexOf('=', name)
  const open = source.indexOf('[', assign)
  const close = source.indexOf(']', open)
  if (assign < 0 || open < 0 || close < 0) {
    fail(`could not read the value of ${constName} in ${relative}`)
    return []
  }
  return [...source.slice(open, close).matchAll(/"([^"]+)"/g)].map((match) => match[1])
}

/**
 * Variant names of a Rust `enum Name { ... }`, in the form they travel
 * on the wire.
 *
 * `EngineMatch` is `#[serde(rename_all = "camelCase")]`, and the
 * TypeScript mirror has to match the wire name, not the Rust
 * identifier — so the first character is lowercased here rather than
 * comparing `Mismatch` with `mismatch` and failing forever. Braces of
 * struct variants are ignored, so `Mismatch { supported: ... }`
 * contributes one name.
 */
function rustEnumVariants(relative, enumName) {
  const source = read(relative)
  const start = source.indexOf(`enum ${enumName}`)
  if (start < 0) {
    fail(`could not find enum ${enumName} in ${relative}`)
    return []
  }
  const open = source.indexOf('{', start)
  const close = source.indexOf('\n}', open)
  if (open < 0 || close < 0) {
    fail(`could not read enum ${enumName} in ${relative}`)
    return []
  }
  return [...source.slice(open, close).matchAll(/^ {4}([A-Z][A-Za-z0-9]*)/gm)].map(
    (match) => match[1][0].toLowerCase() + match[1].slice(1),
  )
}

/** The `verdict: '...'` discriminators of the TypeScript `EngineMatch`. */
function tsVerdicts(relative, typeName) {
  const source = read(relative)
  const start = source.indexOf(`export type ${typeName} =`)
  if (start < 0) {
    fail(`could not find the ${typeName} union in ${relative}`)
    return []
  }
  const end = source.indexOf('\nexport', start)
  const body = source.slice(start, end < 0 ? undefined : end)
  return [...body.matchAll(/verdict: '([^']+)'/g)].map((match) => match[1])
}

/** Parse a JSON file, failing the run rather than throwing a stack. */
function readJson(relative) {
  let value
  try {
    value = JSON.parse(read(relative))
  } catch (error) {
    fail(`could not parse ${relative}: ${error.message}`)
    return null
  }
  return value
}

/**
 * The `enum` of a JSON Schema property's nested schema, e.g. the engine
 * ids under `properties.supportedEngines.items`.
 */
function jsonSchemaPropertyEnum(relative, property) {
  const schema = readJson(relative)
  const values = schema?.properties?.[property]?.items?.enum
  if (!Array.isArray(values)) {
    fail(`could not find properties.${property}.items.enum in ${relative}`)
    return []
  }
  return values
}

/** A JSON Schema property's `description`. */
function jsonSchemaPropertyDescription(relative, property) {
  return readJson(relative)?.properties?.[property]?.description ?? ''
}

function compare(label, lists) {
  const [[referenceName, reference], ...rest] = lists
  const expected = [...reference].sort()
  let failed = false

  for (const [name, kinds] of rest) {
    const actual = [...kinds].sort()
    const missing = expected.filter((kind) => !actual.includes(kind))
    const extra = actual.filter((kind) => !expected.includes(kind))
    if (missing.length === 0 && extra.length === 0) continue
    failed = true
    if (missing.length) {
      fail(`${name} is missing ${missing.join(', ')} (present in ${referenceName})`)
    }
    if (extra.length) {
      fail(`${name} declares ${extra.join(', ')} that ${referenceName} does not implement`)
    }
  }

  if (!failed) {
    console.log(`  ${label}: ${reference.length} entries agree across all sources`)
  }
}

const communityValidator = 'moddin-community-capabilities/scripts/validate_capability.py'
const agentSchema = 'moddin-agent/schema/capability.schema.json'
const agentServer = 'moddin-agent/src/mcp-server.mjs'

// The community repo is a git submodule. It is not initialised in a
// fresh `git clone` of this repo, and a guard that quietly skipped the
// Python set when it is missing would be a guard anyone could switch
// off by accident. Fail loudly, with the fix, instead.
const communityPath = join(root, communityValidator)
if (!existsSync(communityPath)) {
  console.error(
    'Capability kind parity check failed: ' +
      `${communityValidator} is missing.`,
  )
  console.error(
    'The community capabilities repo is a git submodule. Run:\n' +
      '  git submodule update --init --recursive',
  )
  process.exit(1)
}

console.log('Capability kind parity')

compare('step kinds', [
  ['builtin_steps.rs', rustKinds('src-tauri/src/builtin_steps.rs', 'known_kinds')],
  ['capability.ts', tsUnion('src/types/capability.ts', 'StepKind')],
  ['capability.schema.json', jsonSchemaEnum(agentSchema, 'step')],
  ['mcp-server.mjs', jsObjectKeys(agentServer, 'STEP_KINDS')],
  ['validate_capability.py', pythonSet(communityValidator, 'KNOWN_STEP_KINDS')],
])

compare('check kinds', [
  ['builtin_checks.rs', rustKinds('src-tauri/src/builtin_checks.rs', 'known_kinds')],
  ['capability.ts', tsUnion('src/types/capability.ts', 'CheckKind')],
  ['capability.schema.json', jsonSchemaEnum(agentSchema, 'check')],
  ['mcp-server.mjs', jsObjectKeys(agentServer, 'CHECK_KINDS')],
  ['validate_capability.py', pythonSet(communityValidator, 'KNOWN_CHECK_KINDS')],
])

compare('engine ids', [
  ['src/catalog/engines/', enginePresetIds()],
  ['capability.rs', rustConstList('src-tauri/src/capability.rs', 'KNOWN_ENGINES')],
  ['capability.ts', tsUnion('src/types/capability.ts', 'EngineId')],
  ['capability.schema.json', jsonSchemaPropertyEnum(agentSchema, 'supportedEngines')],
  // The community validator refuses an engine id the runner has never
  // heard of, because a recipe naming an unknown engine can never match
  // a game and is invisible on all of them. That makes the vocabulary
  // load-bearing there, not just descriptive — and it makes this the
  // fifth place the list lives, which is the shape that already drifted
  // once for step kinds.
  [
    'validate_capability.py',
    pythonSet(communityValidator, 'KNOWN_ENGINES'),
  ],
])

compare('engine match verdicts', [
  ['capability.rs', rustEnumVariants('src-tauri/src/capability.rs', 'EngineMatch')],
  ['capability.ts', tsVerdicts('src/types/capability.ts', 'EngineMatch')],
])

// The vocabulary above says which engines exist; it cannot say what an
// empty list means, and that is the convention the gate rests on (an
// empty list is every engine, not none). It is prose in every source,
// so this can only check that the prose is still there — and it was
// worth a check once: the schema's description used to be "Optional
// hint for which engines this capability is eligible for.", which told
// an agent author nothing about how the desktop uses the field.
if (!process.exitCode) {
  const description = jsonSchemaPropertyDescription(agentSchema, 'supportedEngines')
  if (!/every/i.test(description)) {
    fail(
      `${agentSchema} properties.supportedEngines.description no longer says what an ` +
        'empty list means. The backend reads an empty list as EVERY engine, and this ' +
        'description is what an agent author sees when writing a recipe.',
    )
  }
}

// Kind parity is necessary but not sufficient. mcp-server.mjs also tells
// the agent which param names each step accepts, and a wrong name there
// produces a recipe that passes every kind check and then fails at
// install time. That is exactly what happened with registry-write,
// which documented keyField/nameField/typeField/dataField while the
// Rust step reads key/value/type.
if (!process.exitCode) {
  const known = rustKnownParamNames()
  for (const kind of jsObjectKeys(agentServer, 'STEP_KINDS')) {
    const declared = jsEntryFields(agentServer, 'STEP_KINDS', kind)
    if (declared === null) continue
    const invented = declared.filter((field) => !known.has(field))
    if (invented.length) {
      fail(
        `mcp-server.mjs tells the agent that ${kind} accepts ` +
          `${invented.join(', ')}, which the Rust runner never reads. ` +
          `An agent authoring that step would produce a recipe that ` +
          `passes every kind check and then fails at install time.`,
      )
    }
  }
}

if (process.exitCode) {
  console.error(
    '\nA step kind, a check kind, an engine id or an engine-gate verdict exists in ' +
      'one place and not another. Update all of:',
  )
  console.error('  src-tauri/src/builtin_steps.rs / builtin_checks.rs')
  console.error('  src-tauri/src/capability.rs (KNOWN_ENGINES, enum EngineMatch)')
  console.error('  src/catalog/engines/*.yaml')
  console.error('  src/types/capability.ts')
  console.error(`  ${agentSchema}`)
  console.error(`  ${agentServer} (STEP_KINDS / CHECK_KINDS, and the ` +
    '`fields` array of every entry)')
  console.error(`  ${communityValidator}`)
  process.exit(process.exitCode)
}

console.log('Capability kind parity passed.')
