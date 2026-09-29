#!/usr/bin/env node
// Capability kind parity guard.
//
// The step and check kinds a recipe may use are declared in four
// places, in three repositories:
//
//   * src-tauri/src/builtin_steps.rs  -> builtin_steps::known_kinds()
//   * src-tauri/src/builtin_checks.rs -> builtin_checks::known_kinds()
//   * src/types/capability.ts         -> StepKind / CheckKind unions
//   * moddin-agent/schema/capability.schema.json -> step.kind / check.kind enums
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
// This check makes that drift a red build instead of a surprise.
//
// Zero dependencies on purpose, like check-frontend-architecture.mjs:
// it has to run in CI before `npm ci` has necessarily succeeded, and
// parsing four small source files is not worth a dependency.

import { existsSync, readFileSync } from 'node:fs'
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
    console.log(`  ${label}: ${reference.length} kinds agree across all sources`)
  }
}

const communityValidator = 'moddin-community-capabilities/scripts/validate_capability.py'
const agentSchema = 'moddin-agent/schema/capability.schema.json'

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
  ['validate_capability.py', pythonSet(communityValidator, 'KNOWN_STEP_KINDS')],
])

compare('check kinds', [
  ['builtin_checks.rs', rustKinds('src-tauri/src/builtin_checks.rs', 'known_kinds')],
  ['capability.ts', tsUnion('src/types/capability.ts', 'CheckKind')],
  ['capability.schema.json', jsonSchemaEnum(agentSchema, 'check')],
  ['validate_capability.py', pythonSet(communityValidator, 'KNOWN_CHECK_KINDS')],
])

if (process.exitCode) {
  console.error(
    '\nA step or check kind exists in one place and not another. Update all of:',
  )
  console.error('  src-tauri/src/builtin_steps.rs / builtin_checks.rs')
  console.error('  src/types/capability.ts')
  console.error(`  ${agentSchema}`)
  console.error(`  ${communityValidator}`)
  process.exit(process.exitCode)
}

console.log('Capability kind parity passed.')
