#!/usr/bin/env node
// Catalogue validation guard.
//
// The catalogue is data, and until this file existed nothing validated
// it: a wrong `executable`, a wrong `enginePreset` or a module the app
// cannot offer reached production, and only a user found out. The
// roadmap names `dead-island-2` as the proof — it shipped declaring RE
// Engine while offering UEVR, OFXR and Cheeky, and the RE Engine
// preset's own description says it does not target UEVR.
//
// So this checks the class, not the instance: every check below is a
// rule about a *shape* of mistake that any future catalogue entry can
// make, and each failure names the file and the key that carries it.
//
// Zero dependencies on purpose, like check-frontend-architecture.mjs
// and check-capability-kind-parity.mjs: it has to run in CI before
// anything is installed. That is also why there is a small strict YAML
// reader below instead of an import. It refuses any construct it does
// not understand (block scalars, anchors, flow mappings, multi-document
// files) instead of guessing, because a validator that mis-parses gives
// a confident wrong answer, which is the worst failure mode a guard has.
// The app itself parses these same files with the `yaml` package, in
// src/services/catalog.ts and src/services/preset.ts.
//
// ## Open data defects
//
// This guard found real defects in the catalogue that shipped before it
// existed. They are listed in KNOWN_DEFECTS below rather than deleted:
// the catalogue is not this change's to edit, and deleting a line from
// the baseline would hide the bug instead of fixing it. The baseline
// self-cleans — an entry that no longer reproduces fails the run and
// asks to be removed — and every new violation fails the build.

import { existsSync, readFileSync, readdirSync } from 'node:fs'
import { dirname, join, relative } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

function relativeTo(file) {
  return relative(root, file).replaceAll('\\', '/')
}

// ---------------------------------------------------------------------------
// Strict YAML reader
// ---------------------------------------------------------------------------

/** Raised for a construct the reader refuses to interpret. */
class YamlError extends Error {}

function unsupported(token, what) {
  throw new YamlError(`${relativeTo(token.file)}:${token.number}: ${what}`)
}

const SEQUENCE_ITEM = /^-(\s|$)/
const MAPPING_ENTRY = /^([^:#][^:]*):(?:[ \t]+(.*))?$/
const BLOCK_SCALAR = /^([|>])([-]?)$/

/** Collapse a folded (`>`) block: a newline becomes a space. */
function foldBlock(lines) {
  let text = ''
  lines.forEach((line, index) => {
    if (line === '') {
      text += '\n'
      return
    }
    text += index === 0 ? line : `${lines[index - 1] === '' ? '' : ' '}${line}`
  })
  return text
}

function tokenize(text, file) {
  const tokens = []
  const lines = text.replace(/^\uFEFF/, '').replaceAll('\r\n', '\n').split('\n')
  for (let at = 0; at < lines.length; at += 1) {
    const line = lines[at]
    const number = at + 1
    const where = { file, number }
    if (line.trim() === '' || /^\s*#/.test(line)) continue
    if (line.includes('\t')) {
      throw new YamlError(`${relativeTo(file)}:${number}: tab indentation is not valid YAML`)
    }
    if (/^\s*---+\s*$/.test(line)) {
      if (tokens.length) throw new YamlError(`${relativeTo(file)}:${number}: multi-document YAML`)
      continue
    }
    const body = line.trim()
    const token = { ...where, indent: line.length - line.trimStart().length, body }
    if (body.startsWith('&') || body.startsWith('*') || body.startsWith('!')) {
      unsupported(token, 'anchors, aliases and tags are not supported')
    }
    if (body.startsWith('<<:')) unsupported(token, 'merge keys are not supported')
    if (body.startsWith('{')) unsupported(token, 'flow mappings are not supported')
    if (!SEQUENCE_ITEM.test(body) && !MAPPING_ENTRY.test(body)) {
      unsupported(token, `cannot interpret line: ${JSON.stringify(body)}`)
    }

    // A block scalar is read here rather than in the parser: the body
    // lines belong to no other token, and this is the only place that
    // still has the raw text.
    const entry = MAPPING_ENTRY.exec(body)
    const value = entry ? (entry[2] ?? '').trim() : ''
    if (value.startsWith('>') || value.startsWith('|')) {
      const style = BLOCK_SCALAR.exec(value)
      if (!style) {
        unsupported(
          token,
          `block scalar style "${value}" is not supported (use |, |-, > or >- with no explicit indent)`,
        )
      }
      const body2 = []
      let contentIndent = null
      let cursor = at + 1
      for (; cursor < lines.length; cursor += 1) {
        const candidate = lines[cursor]
        if (candidate.trim() === '') {
          body2.push('')
          continue
        }
        const candidateIndent = candidate.length - candidate.trimStart().length
        if (candidateIndent <= token.indent) break
        if (contentIndent === null) contentIndent = candidateIndent
        body2.push(candidate.slice(contentIndent))
      }
      while (body2.length && body2[body2.length - 1] === '') body2.pop()
      const text = style[1] === '|' ? body2.join('\n') : foldBlock(body2)
      token.blockContent = style[2] === '-' ? text : `${text}\n`
      at = cursor - 1
    }

    tokens.push(token)
  }
  return tokens
}

function parseScalar(raw, token) {
  const value = raw.trim()
  if (value === '') return null
  if (value === '[]') return []
  if (value.startsWith('[') && value.endsWith(']')) {
    const inner = value.slice(1, -1).trim()
    if (inner === '') return []
    if (inner.includes('[') || inner.includes(']')) unsupported(token, 'nested flow sequences are not supported')
    return inner.split(',').map((item) => parseScalar(item, token))
  }
  if (value.length > 1 && value.startsWith('"') && value.endsWith('"')) {
    return value.slice(1, -1).replace(/\\"/g, '"').replace(/\\\\/g, '\\')
  }
  if (value.length > 1 && value.startsWith("'") && value.endsWith("'")) {
    return value.slice(1, -1).replaceAll("''", "'")
  }
  if (value === 'true' || value === 'false') return value === 'true'
  if (/^-?\d+$/.test(value)) return Number(value)
  return value
}

function parseDocument(text, file) {
  const tokens = tokenize(text, file)
  const state = { tokens, index: 0 }

  const readValue = (token) => {
    if (token === undefined) return null
    if (token.blockContent !== undefined) return token.blockContent
    const entry = MAPPING_ENTRY.exec(token.body)
    const [, key, rawValue] = entry
    if (rawValue !== undefined && rawValue.trim() !== '') return parseScalar(rawValue, token)
    const next = state.tokens[state.index]
    if (next && next.indent > token.indent) return readNode(next.indent)
    // A block sequence may sit at the same indent as its key.
    if (next && next.indent === token.indent && SEQUENCE_ITEM.test(next.body)) {
      return readNode(token.indent)
    }
    return null
  }

  const readMapping = (indent) => {
    const map = {}
    while (
      state.index < state.tokens.length &&
      state.tokens[state.index].indent === indent &&
      !SEQUENCE_ITEM.test(state.tokens[state.index].body)
    ) {
      const token = state.tokens[state.index]
      const entry = MAPPING_ENTRY.exec(token.body)
      // Advance first: `readValue` looks at the *next* token to decide
      // whether this key owns a nested block.
      state.index += 1
      map[entry[1].trim()] = readValue(token)
    }
    return map
  }

  const readSequence = (indent) => {
    const items = []
    while (
      state.index < state.tokens.length &&
      state.tokens[state.index].indent === indent &&
      SEQUENCE_ITEM.test(state.tokens[state.index].body)
    ) {
      const token = state.tokens[state.index]
      const rest = token.body.slice(1).trim()
      if (rest === '') {
        state.index += 1
        const next = state.tokens[state.index]
        items.push(next && next.indent > indent ? readNode(next.indent) : null)
        continue
      }
      if (MAPPING_ENTRY.test(rest) && !SEQUENCE_ITEM.test(rest)) {
        // `- key: value` opens a mapping whose remaining keys are
        // indented past the dash.
        const map = {}
        const key = MAPPING_ENTRY.exec(rest)[1].trim()
        state.index += 1
        map[key] = readValue({ ...token, body: rest, indent: token.indent + 2 })
        const childIndent = state.tokens[state.index]?.indent
        if (childIndent !== undefined && childIndent > token.indent) {
          Object.assign(map, readMapping(childIndent))
        }
        items.push(map)
        continue
      }
      items.push(parseScalar(rest, token))
      state.index += 1
    }
    return items
  }

  function readNode(indent) {
    return SEQUENCE_ITEM.test(state.tokens[state.index].body)
      ? readSequence(indent)
      : readMapping(indent)
  }

  if (state.tokens.length === 0) return {}
  const document = readNode(state.tokens[0].indent)
  if (state.index < state.tokens.length) {
    unsupported(state.tokens[state.index], 'unexpected indentation')
  }
  return document
}

function readYaml(relative) {
  const file = join(root, relative)
  if (!existsSync(file)) {
    throw new YamlError(`${relative} does not exist`)
  }
  return parseDocument(readFileSync(file, 'utf8'), file)
}

function yamlFilesIn(relativeDir) {
  const dir = join(root, relativeDir)
  if (!existsSync(dir)) throw new YamlError(`${relativeDir} does not exist`)
  return readdirSync(dir)
    .filter((name) => name.endsWith('.yaml') || name.endsWith('.yml'))
    .sort()
    .map((name) => `${relativeDir}/${name}`)
}

// ---------------------------------------------------------------------------
// Sources of truth
// ---------------------------------------------------------------------------

const games = yamlFilesIn('src/catalog/games').map((file) => ({
  file,
  data: readYaml(file),
}))
const engines = yamlFilesIn('src/catalog/engines').map((file) => ({
  file,
  data: readYaml(file),
}))
const recipes = yamlFilesIn('src-tauri/capabilities').map((file) => ({
  file,
  data: readYaml(file),
}))

const engineIds = new Set(engines.map((engine) => engine.data.id))
const recipeIds = new Set(recipes.map((recipe) => recipe.data.id))

/**
 * Recipes the binary actually loads. `insert_built_ins` panics on a
 * duplicate id and silently omits nothing, but a YAML file nobody
 * registered is a recipe the app cannot see — the kind of thing that
 * reads as "supported" in review and does nothing at runtime.
 */
const registeredRecipes = new Set(
  [...readFileSync(join(root, 'src-tauri/src/capability_runner.rs'), 'utf8').matchAll(
    /include_str!\("\.\.\/capabilities\/([^"]+)"\)/g,
  )].map((match) => match[1]),
)

/** Module ids the backend implements, from `Module::id`. */
const backendModuleIds = new Set(
  readdirSync(join(root, 'src-tauri/src'))
    .filter((name) => name.endsWith('.rs'))
    .flatMap((name) => {
      if (name === 'module.rs') return [] // the trait's own test stub
      const source = readFileSync(join(root, 'src-tauri/src', name), 'utf8')
      return [...source.matchAll(/fn id\(&self\) -> &'static str \{\s*"([^"]+)"/g)].map((m) => m[1])
    }),
)

/**
 * Every command the app registers, from the invoke_handler list.
 * The list is indented and CRLF, so the line end has to be allowed for.
 */
const registeredCommands = new Set(
  [...readFileSync(join(root, 'src-tauri/src/lib.rs'), 'utf8').matchAll(
    /^[ \t]{12}(?:[a-z][a-z0-9_]*::)*([a-z][a-z0-9_]+),/gm,
  )].map((match) => match[1]),
)

/**
 * Whether the app can act on a catalogue module id.
 *
 * Three independent artefacts, because no single one lists them: a
 * capability recipe id (`ofxr-bridge`), a `Module::id` implementation
 * (`ofxr-framegen`), or a registered command whose name carries every
 * token of the id (`create_desktop_shortcut` for `desktop-shortcut`).
 * The last one is deliberately loose for a single-token id — `launch_vr_game`
 * would satisfy an id called `game` — so it is the last resort, not the
 * first.
 */
function idTokens(id) {
  return id.toLowerCase().split('-').filter(Boolean)
}

function offeredByApp(id) {
  if (recipeIds.has(id) || backendModuleIds.has(id)) return true
  const tokens = idTokens(id)
  return [...registeredCommands].some((command) => {
    const words = new Set(command.toLowerCase().split('_'))
    return tokens.every((token) => words.has(token))
  })
}

/** Every module id the catalogue itself talks about, for prose matching. */
const knownModuleIds = new Set([
  ...recipeIds,
  ...backendModuleIds,
  ...games.flatMap((game) => (asList(game.data.modules) ?? []).map((module) => module?.id).filter(Boolean)),
  ...engines.flatMap((engine) => (asList(engine.data.modules) ?? []).map((module) => module?.id).filter(Boolean)),
])

// ---------------------------------------------------------------------------
// Findings
// ---------------------------------------------------------------------------

const findings = []

function find(rule, file, key, message) {
  findings.push({ rule, file, key, message })
}

function asString(value) {
  return typeof value === 'string' ? value : null
}

function asList(value) {
  if (value === null || value === undefined) return []
  if (!Array.isArray(value)) return null // wrong shape; reported elsewhere
  return value
}

// ---------------------------------------------------------------------------
// Rules
// ---------------------------------------------------------------------------

const APP_ID_KEYS = ['steamAppId', 'epicAppId', 'gogAppId']

/** Windows device names that cannot be a file or folder. */
const RESERVED_NAMES = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i
// `/` and `\` are separators, so they cannot survive the split below.
const INVALID_PATH_CHARS = /[<>:"\u007C\u003F\u002A]/
const CONTROL_CHARS = /[\u0000-\u001f\u007f]/
/**
 * Folder names every install shares, so they identify no one game.
 * `Binaries/Win64` is where every Unreal title ships; `Game/` is where
 * Elden Ring puts its executable and what half the catalogue could
 * plausibly be renamed to.
 */
const GENERIC_SEGMENTS = new Set([
  'bin',
  'binary',
  'binaries',
  'content',
  'data',
  'game',
  'games',
  'mods',
  'system',
  'win32',
  'win64',
  'x64',
  'x86',
])

/**
 * A relative Windows path below the game directory. Shared by
 * `executable` and by the `vr-launch` profile's in-game files, because
 * they are resolved the same way: relative to the install root.
 */
function checkRelativeWindowsPath(where, value) {
  const path = asString(value)
  if (path === null) {
    find('path-not-a-string', where.file, where.key, 'expected a path string')
    return
  }
  const segments = path.split(/[\\/]/)
  const problems = []
  if (path === '') problems.push('is empty')
  if (/^[A-Za-z]:/.test(path)) problems.push('is an absolute drive path')
  if (path.startsWith('/') || path.startsWith('\\')) problems.push('is an absolute path')
  if (segments.includes('..')) problems.push('traverses out of the game folder with ".."')
  if (segments.includes('')) problems.push('contains an empty path segment')
  for (const segment of segments) {
    if (INVALID_PATH_CHARS.test(segment)) problems.push(`segment "${segment}" has a character Windows forbids`)
    if (CONTROL_CHARS.test(segment)) problems.push(`segment "${segment}" contains a control character`)
    if (/[. ]$/.test(segment)) problems.push(`segment "${segment}" ends with a dot or a space`)
    if (RESERVED_NAMES.test(segment)) problems.push(`segment "${segment}" is a reserved Windows device name`)
  }
  for (const problem of problems) {
    find('implausible-path', where.file, where.key, `"${path}" ${problem}`)
  }
  return segments
}

/** Per-engine layout expectations, from how the engines actually ship. */
const ENGINE_LAYOUTS = {
  unreal5: (segments) =>
    segments.includes('Binaries') &&
    segments.includes('Win64') &&
    segments.lastIndexOf('Binaries') < segments.length - 1
      ? []
      : ['an Unreal Engine shipping build lives under <Project>/Binaries/Win64/'],
  redengine: (segments) =>
    segments.some((segment) => segment.toLowerCase() === 'x64')
      ? []
      : ['a REDengine build lives under bin/x64/'],
}

for (const game of games) {
  const data = game.data
  const id = asString(data.id) ?? '<no id>'
  const where = (key) => ({ file: game.file, key })

  if (asString(data.id) !== game.file.replace(/^src\/catalog\/games\//, '').replace(/\.ya?ml$/, '')) {
    find(
      'id-does-not-match-file',
      game.file,
      'id',
      `id "${asString(data.id)}" does not match the file name, which is what the loader and every reference use`,
    )
  }

  const appIds = APP_ID_KEYS.map((key) => [key, asString(data[key])]).filter(([, value]) => value)
  if (appIds.length === 0) {
    find(
      'no-store-app-id',
      game.file,
      'id',
      'a game with no steamAppId, epicAppId or gogAppId can never match an installed game',
    )
  }

  const engine = asString(data.enginePreset)
  if (engine !== null && !engineIds.has(engine)) {
    find(
      'unknown-engine-preset',
      game.file,
      'enginePreset',
      `"${engine}" is not a preset in src/catalog/engines/ (have: ${[...engineIds].join(', ')})`,
    )
  }

  // A duplicate id inside one list makes the loader throw and takes the
  // whole catalogue down with it. The engine preset merge cannot add
  // one — `mergePresetIntoGame` drops preset modules the game already
  // lists — so only the game's own list can.
  const ownModuleIds = (asList(data.modules) ?? []).map((module) => module?.id)
  const counted = new Map()
  for (const moduleId of ownModuleIds) {
    if (!moduleId) continue
    counted.set(moduleId, (counted.get(moduleId) ?? 0) + 1)
  }
  for (const [moduleId, times] of counted) {
    if (times < 2) continue
    find(
      'duplicate-id',
      game.file,
      `modules[${moduleId}]`,
      `listed ${times} times by the game — the loader throws on this and the whole catalogue fails to load`,
    )
  }

  const segments = checkRelativeWindowsPath(where('executable'), data.executable)
  const executable = asString(data.executable) ?? ''
  if (segments && !executable.toLowerCase().endsWith('.exe')) {
    find(
      'implausible-executable',
      game.file,
      'executable',
      `"${executable}" does not point at an .exe file`,
    )
  }
  if (segments && engine && ENGINE_LAYOUTS[engine]) {
    for (const problem of ENGINE_LAYOUTS[engine](segments)) {
      find('engine-executable-layout', game.file, 'executable', `${engine}: ${problem}`)
    }
  }

  // No game may point at another game's directory or executable.
  findCrossGamePath(game, 'executable', executable, segments)
}

/**
 * Report a path that names another game's install folder, executable or
 * id. Shared by `executable` and by the `vr-launch` profile's in-game
 * files, because a launch profile that requires a file from another
 * game's install is the same mistake one layer down.
 */
function findCrossGamePath(game, key, path, segments) {
  const ownName = (path.split(/[\\/]/).pop() ?? '').toLowerCase()
  const mine = [path, ...(segments ?? [])].join('/').toLowerCase()
  for (const other of games) {
    if (other.file === game.file) continue
    const otherSegments = (asString(other.data.executable) ?? '').split(/[\\/]/).map((s) => s.toLowerCase())
    const otherName = otherSegments[otherSegments.length - 1] ?? ''
    // What identifies the *other* game: its id, the folder its install
    // lives in, and its executable stem. Generic folder names say
    // nothing about which game owns them, so they cannot accuse a
    // neighbour — every Unreal title ships under `Binaries/Win64`.
    const distinctive = [
      ...idTokens(asString(other.data.id) ?? ''),
      otherSegments[0] ?? '',
      otherName.replace(/\.exe$/, ''),
    ].filter((token) => token.length >= 5 && !GENERIC_SEGMENTS.has(token))
    const hit = distinctive.find((token) => mine.includes(token))
    if (hit) {
      find(
        'cross-game-path',
        game.file,
        key,
        `"${path}" contains "${hit}", which is identifying for game "${other.data.id}"; one game cannot point at another game's directory`,
      )
    }
    if (otherName !== '' && otherName === ownName) {
      find(
        'cross-game-path',
        game.file,
        key,
        `"${path}" is the same file name as game "${other.data.id}"'s executable`,
      )
    }
  }
}

// --- duplicates ------------------------------------------------------------

function findDuplicates(entries, label, keyOf, fileOf) {
  const seen = new Map()
  for (const entry of entries) {
    const key = keyOf(entry)
    if (key === null || key === undefined) continue
    const previous = seen.get(key)
    if (previous) {
      find('duplicate-id', fileOf(entry), key, `"${key}" is already declared in ${previous}`)
    } else {
      seen.set(key, fileOf(entry))
    }
  }
  return seen
}

const gameIds = findDuplicates(
  games.map((game) => game.data.id),
  'game',
  (id) => id,
  (id) => games.find((game) => game.data.id === id)?.file ?? '<unknown>',
)
findDuplicates(engines.map((engine) => engine.data.id), 'engine', (id) => id, () => 'src/catalog/engines/')
findDuplicates(recipes.map((recipe) => recipe.data.id), 'recipe', (id) => id, () => 'src-tauri/capabilities/')

for (const key of APP_ID_KEYS) {
  const owners = new Map()
  for (const game of games) {
    const value = asString(game.data[key])
    if (!value) continue
    const previous = owners.get(value)
    if (previous) {
      find(
        'duplicate-app-id',
        game.file,
        key,
        `${key} "${value}" is already claimed by game "${previous}"`,
      )
    } else {
      owners.set(value, asString(game.data.id))
    }
  }
}

// --- engine presets vs the modules games offer -----------------------------

/**
 * Modules an engine preset's own description rules out.
 *
 * The preset descriptions are prose written for a human ("UEVR does not
 * target RE Engine"), and that prose is the only place the catalogue
 * states which module an engine is not for. Parsing it is what turns
 * `dead-island-2` from a curiosity into a rule: a game on that preset
 * offering the denied module is the exact shape that shipped.
 */
function deniedByPreset(preset) {
  const denied = new Set()
  const description = asString(preset.data.description) ?? ''
  for (const sentence of description.split(/[.;]/)) {
    if (!/\bdoes not\b|\bno\b/i.test(sentence)) continue
    for (const moduleId of knownModuleIds) {
      const stem = idTokens(moduleId)[0]
      if (stem.length < 4) continue
      if (new RegExp(`\\b${stem}\\b`, 'i').test(sentence)) denied.add(moduleId)
    }
  }
  return denied
}

for (const engine of engines) {
  const preset = engine.data
  const presetId = asString(preset.id)
  const presetModules = (asList(preset.modules) ?? []).map((module) => module?.id)
  const denied = deniedByPreset(engine)
  if (denied.size === 0) continue

  for (const game of games) {
    if (asString(game.data.enginePreset) !== presetId) continue
    for (const module of asList(game.data.modules) ?? []) {
      const moduleId = module?.id
      if (!denied.has(moduleId)) continue
      const covered = presetModules.includes(moduleId)
      find(
        'preset-denies-module',
        game.file,
        `modules[${moduleId}]`,
        `engine preset "${presetId}" says it does not offer ${moduleId}` +
          (covered ? '' : `, and ${moduleId} is not in the preset's module list either`) +
          `. Its own description: "${asString(preset.description)}"`,
      )
    }
  }
}

// --- modules a game lists must be offered by its engine or by the app ------

for (const game of games) {
  const data = game.data
  const gameId = asString(data.id)
  const preset = engines.find((engine) => asString(engine.data.id) === asString(data.enginePreset))
  const presetModules = new Set((asList(preset?.data.modules) ?? []).map((module) => module?.id))
  const modules = asList(data.modules) ?? []

  for (const module of modules) {
    const moduleId = module?.id
    if (!moduleId) {
      find('module-without-id', game.file, 'modules', 'a module entry has no id')
      continue
    }
    if (!presetModules.has(moduleId) && !offeredByApp(moduleId)) {
      find(
        'module-not-offered',
        game.file,
        `modules[${moduleId}]`,
        `nothing offers "${moduleId}": it is not in the "${asString(data.enginePreset) ?? 'no'}" preset, ` +
          'not a capability recipe in src-tauri/capabilities/, and no backend module or command implements it',
      )
    }

    for (const dependency of asList(module?.dependencies) ?? []) {
      if (!modules.some((other) => other?.id === dependency) && !presetModules.has(dependency)) {
        find(
          'unknown-module-dependency',
          game.file,
          `modules[${moduleId}].dependencies`,
          `"${dependency}" is not a module this game offers`,
        )
      }
    }

    // A recipe that names the engines it targets must not be offered to a
    // game on an engine it does not.
    const recipe = recipes.find((item) => asString(item.data.id) === moduleId)
    const recipeEngines = asList(recipe?.data.supportedEngines) ?? []
    const gameEngine = asString(data.enginePreset)
    if (recipe && gameEngine && recipeEngines.length > 0 && !recipeEngines.includes(gameEngine)) {
      find(
        'recipe-engine-mismatch',
        game.file,
        `modules[${moduleId}]`,
        `the "${moduleId}" recipe declares supportedEngines: [${recipeEngines.join(', ')}], ` +
          `and this game declares enginePreset: ${gameEngine}`,
      )
    }

    // A `flat` profile references modules; every one of them has to exist.
    const flat = module?.flat
    if (flat !== undefined && flat !== null) {
      const referenced = Array.isArray(flat) ? flat : typeof flat === 'object' ? Object.keys(flat) : null
      if (referenced === null) {
        find('flat-profile-shape', game.file, `modules[${moduleId}].flat`, 'expected a list of module ids or a mapping keyed by module id')
      } else {
        for (const reference of referenced) {
          if (reference !== moduleId && !modules.some((other) => other?.id === reference) && !presetModules.has(reference)) {
            find(
              'flat-profile-unknown-module',
              game.file,
              `modules[${moduleId}].flat`,
              `"${reference}" is not a module this game offers`,
            )
          }
        }
      }
    }

    // The `vr-launch` profile points at files inside the game folder.
    if (moduleId === 'vr-launch' && asString(module?.status) === 'available') {
      const config = module?.config
      if (!config || typeof config !== 'object') {
        find('vr-launch-without-profile', game.file, `modules[${moduleId}].config`, 'an available vr-launch module declares no config')
      } else {
        const requiredFiles = asList(config.requiredFiles)
        if (requiredFiles === null || requiredFiles.length === 0) {
          find('vr-launch-without-profile', game.file, `modules[${moduleId}].config.requiredFiles`, 'a launch profile with nothing to validate cannot fail early')
        } else {
          for (const file of requiredFiles) {
            const requiredWhere = {
              file: game.file,
              key: `modules[${moduleId}].config.requiredFiles`,
            }
            const segments = checkRelativeWindowsPath(requiredWhere, file)
            const required = asString(file) ?? ''
            if (segments && !/\.[A-Za-z0-9]+$/.test(segments[segments.length - 1] ?? '')) {
              find(
                'vr-launch-required-file',
                game.file,
                requiredWhere.key,
                `"${required}" is required by the launch profile but has no file extension, so it cannot be checked for presence`,
              )
            }
            findCrossGamePath(game, requiredWhere.key, required, segments)
          }
        }
        if (asString(config.configPath) !== null) {
          const configWhere = { file: game.file, key: `modules[${moduleId}].config.configPath` }
          const segments = checkRelativeWindowsPath(configWhere, config.configPath)
          findCrossGamePath(game, configWhere.key, asString(config.configPath) ?? '', segments)
        }
        for (const patch of asList(config.configPatches) ?? []) {
          const parts = asString(patch)?.split('|') ?? []
          if (parts.length !== 3 || parts.some((part) => part.trim() === '')) {
            find(
              'vr-launch-config-patch',
              game.file,
              `modules[${moduleId}].config.configPatches`,
              `"${patch}" is not a Section|Key|Value triple`,
            )
          }
        }
      }
    }

    // Any URL a recipe reaches for has to be https, and a *url config
    // field has to be one.
    for (const [key, value] of Object.entries(module?.config ?? {})) {
      const text = asString(value)
      if (text === null) continue
      if (text.startsWith('http://')) {
        find('insecure-url', game.file, `modules[${moduleId}].config.${key}`, `"${text}" is a plaintext http:// URL`)
      }
      if (/url$/i.test(key) && !/^https:\/\/\S+$/.test(text)) {
        find('invalid-url', game.file, `modules[${moduleId}].config.${key}`, `"${text}" is not an https URL`)
      }
    }
  }
}

// --- engine presets and recipes --------------------------------------------

for (const engine of engines) {
  const presetId = asString(engine.data.id)
  const seen = new Set()
  for (const module of asList(engine.data.modules) ?? []) {
    const moduleId = module?.id
    if (!moduleId) continue
    if (seen.has(moduleId)) {
      find('duplicate-id', engine.file, `modules[${moduleId}]`, `duplicate module id in preset "${presetId}"`)
    }
    seen.add(moduleId)
  }
}

for (const recipe of recipes) {
  const recipeId = asString(recipe.data.id)
  for (const dependency of asList(recipe.data.dependencies) ?? []) {
    if (!recipeIds.has(dependency)) {
      find(
        'unknown-capability-dependency',
        recipe.file,
        'dependencies',
        `"${dependency}" is not a capability recipe in src-tauri/capabilities/`,
      )
    }
  }
  for (const engine of asList(recipe.data.supportedEngines) ?? []) {
    if (!engineIds.has(engine)) {
      find(
        'unknown-supported-engine',
        recipe.file,
        'supportedEngines',
        `"${engine}" is not an engine in src/catalog/engines/ (have: ${[...engineIds].join(', ')})`,
      )
    }
  }
  const fileName = recipe.file.replace(/^src-tauri\/capabilities\//, '')
  if (!registeredRecipes.has(fileName)) {
    find(
      'unregistered-recipe',
      recipe.file,
      'id',
      `"${fileName}" is not in capability_runner.rs's include_str! list, so the binary never loads it`,
    )
  }
}

for (const fileName of registeredRecipes) {
  if (!existsSync(join(root, 'src-tauri/capabilities', fileName))) {
    find('missing-recipe-file', 'src-tauri/src/capability_runner.rs', 'include_str!', `${fileName} is registered but the file does not exist`)
  }
}

// ---------------------------------------------------------------------------
// Known defects
// ---------------------------------------------------------------------------

/**
 * Data defects that shipped before this guard existed. Each entry is
 * "does this exact problem still happen here?" — the catalogue may be
 * fixed, the baseline may not be allowed to forget it.
 *
 * A stale entry (the problem was fixed) fails the run, so the baseline
 * cannot outlive the bug it excuses. A new problem is never excused.
 */
const KNOWN_DEFECTS = [
  {
    rule: 'recipe-engine-mismatch',
    file: 'src/catalog/games/cyberpunk-2077.yaml',
    key: 'modules[uevr]',
    why: 'UEVR injects Unreal Engine only; the uevr recipe declares supportedEngines: [unreal5] and its own safetyNotes say it is only eligible for Unreal games, while this game declares enginePreset: redengine. Needs a product call: drop the module or extend the recipe.',
  },
  {
    rule: 'preset-denies-module',
    file: 'src/catalog/games/cyberpunk-2077.yaml',
    key: 'modules[uevr]',
    why: 'The redengine preset description states Moddin does not bundle UEVR-style VR injection for it, and this game offers UEVR. One decision closes both this and the recipe-engine-mismatch entry below: drop the module, or teach the recipe about REDengine.',
  },
  {
    rule: 'preset-denies-module',
    file: 'src/catalog/games/doom-2016.yaml',
    key: 'modules[kharvox-vr]',
    why: 'The idtech preset says the desktop shortcut is the only reusable mod that ships today and that KHARVOX-style launchers stay outside the app; the game lists a kharvox-vr card that nothing implements.',
  },
  {
    rule: 'module-not-offered',
    file: 'src/catalog/games/doom-2016.yaml',
    key: 'modules[kharvox-vr]',
    why: 'No preset, recipe, module or command offers kharvox-vr. The card renders and its primary action reports that it has no action.',
  },
  {
    rule: 'module-not-offered',
    file: 'src/catalog/games/dawnwalker.yaml',
    key: 'modules[graphics-profile]',
    why: 'A declared intention with nothing behind it: no preset, recipe, module or command. ROADMAP has no item for it.',
  },
  {
    rule: 'module-not-offered',
    file: 'src/catalog/games/dawnwalker.yaml',
    key: 'modules[vortex-migration]',
    why: 'A declared intention with nothing behind it: no preset, recipe, module or command. ROADMAP has no item for it.',
  },
  {
    rule: 'module-not-offered',
    file: 'src/catalog/games/dead-island-2.yaml',
    key: 'modules[cheeky-foveated-dlss-uevr]',
    why: 'A near-duplicate of the cheeky-foveated-dlss module the unreal5 preset already adds, under an id nothing implements.',
  },
  {
    rule: 'module-not-offered',
    file: 'src/catalog/games/stalker-2.yaml',
    key: 'modules[cheeky-foveated-dlss-uevr]',
    why: 'Same near-duplicate id as dead-island-2; nothing implements it.',
  },
]

const knownKeys = new Set(KNOWN_DEFECTS.map((defect) => `${defect.rule}|${defect.file}|${defect.key}`))
const reproduced = new Set()
const openDefects = []
const blocking = []

for (const finding of findings) {
  const key = `${finding.rule}|${finding.file}|${finding.key}`
  if (knownKeys.has(key)) {
    reproduced.add(key)
    openDefects.push({ ...finding, why: KNOWN_DEFECTS.find((defect) => `${defect.rule}|${defect.file}|${defect.key}` === key).why })
    continue
  }
  blocking.push(finding)
}

for (const defect of KNOWN_DEFECTS) {
  const key = `${defect.rule}|${defect.file}|${defect.key}`
  if (!reproduced.has(key)) {
    blocking.push({
      rule: 'stale-known-defect',
      file: defect.file,
      key: defect.key,
      message: `the known defect is listed here but no longer reproduces. Fix the catalogue entry and delete the baseline line in scripts/validate-catalog.mjs. (${defect.why})`,
    })
  }
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

const checked = [
  `${games.length} games`,
  `${engines.length} engine presets`,
  `${recipes.length} capability recipes`,
  `${registeredRecipes.size} registered recipes`,
  `${registeredCommands.size} registered commands`,
]

if (blocking.length) {
  console.error('\nCatalogue validation failed:\n')
  for (const finding of blocking) {
    console.error(`  - [${finding.rule}] ${finding.file} → ${finding.key}: ${finding.message}`)
  }
  console.error('\nThe catalogue is data with no other gate; this is the gate. Fix the entry,')
  console.error('or — if the data is right and the rule is wrong — fix the rule and say why in ROADMAP.md.\n')
  process.exit(1)
}

if (openDefects.length) {
  console.log(`Catalogue validation passed with ${openDefects.length} open data defect(s) (${checked.join(', ')}):\n`)
  for (const defect of openDefects) {
    console.log(`  - [${defect.rule}] ${defect.file} → ${defect.key}`)
    console.log(`      ${defect.why}`)
  }
  console.log('\nThese are catalogue bugs, not check tuning. Each one is excused by name in')
  console.log('scripts/validate-catalog.mjs and fails the build the moment the class recurs')
  console.log('somewhere else. Fixing one deletes its baseline line.\n')
} else {
  console.log(`Catalogue validation passed (${checked.join(', ')}).`)
}
