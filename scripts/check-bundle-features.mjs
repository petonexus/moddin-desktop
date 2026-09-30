#!/usr/bin/env node
// Bundled-build feature guard.
//
// One invariant: every crate in this repository that depends on `tauri` must
// enable its `custom-protocol` Cargo feature.
//
// Without that feature the `tauri` crate's build script takes
// `let dev = !custom_protocol` (tauri/build.rs), `tauri-build` then runs
// `cfg_alias("dev", is_dev())` for the whole crate, and
// `Manager::get_app_url` resolves the app URL to `build.devUrl` —
// http://127.0.0.1:1420 — instead of the embedded `frontendDist`.
//
// The reason this needs a guard is that nothing else notices. `cargo check`
// passes. `cargo build --release` passes. `beforeBuildCommand` runs, `dist/`
// is produced, the installer builds, installs, and launches. The only
// symptom is a blank WebView2 window on a user's machine, with no log line
// and no error. A build-time test cannot catch it, because nothing is wrong
// until the artifact is in front of a user.
//
// The fix is one line in src-tauri/Cargo.toml. It is written the way it is so
// that the next person who sees `features = ["custom-protocol"]` and reads it
// as leftover scaffolding does not "tidy" it away: the manifest now carries a
// comment saying what breaks without it.
//
// ## What this check is not
//
// It reads the manifest, not the artifact. A manifest can carry the feature
// and a stale build directory still hold the old binary — cargo does not
// re-run a dependency's build script when a feature flips, which is why the
// original fix needed a one-time `cargo clean`. The artifact-level proof is
// the dry-run tag described in docs/UPDATER.md: a build whose EXE is checked
// for the embedded payload. This check is the cheap half, and it is the half
// that runs on every commit.
//
// ## Inventory
//
// The manifests are discovered by walking the repository rather than by
// hardcoding src-tauri/Cargo.toml, for the reason ROADMAP.md records twice:
// a guard that names four of five sources is right about the class and wrong
// about the inventory, and the omission is found by a human reading the
// source. There is one manifest today; if there are three tomorrow, this
// check covers all three.
//
// `tauri-build` is deliberately NOT checked. It is a different crate, it is a
// build dependency, and it does not take this feature.
//
// Zero dependencies, like the other guards here: it runs in CI, and parsing
// a handful of small TOML files is not worth a dependency.

import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { dirname, join, relative } from 'node:path'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

const SKIP = new Set(['target', 'node_modules', '.git', 'dist', 'moddin-runtime'])

let failures = 0

function fail(message) {
  console.error(`Bundle feature check failed: ${message}`)
  failures += 1
}

function read(absolute) {
  return readFileSync(absolute, 'utf8')
}

/** Every Cargo.toml in the tree, gitignored build output excluded. */
function manifests(directory = root, found = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) {
      if (SKIP.has(entry.name)) continue
      manifests(join(directory, entry.name), found)
    } else if (entry.name === 'Cargo.toml') {
      found.push(join(directory, entry.name))
    }
  }
  return found
}

/**
 * The `[dependencies]` tables of one manifest, as `{ name, body, line }`
 * entries. `tauri-build` under `[build-dependencies]` is a different crate
 * and must not be reported here.
 */
function dependencyTables(source) {
  const lines = source.split(/\r?\n/)
  const tables = []
  let current = null

  lines.forEach((line, index) => {
    const table = line.match(/^\s*\[([^\]]+)\]\s*$/)
    if (table) {
      const name = table[1].trim()
      current = /(^|\.)(dev-dependencies|build-dependencies)$/.test(name)
        ? null
        : name.endsWith('dependencies')
          ? { name, lines: [] }
          : null
      if (current) tables.push(current)
      return
    }
    if (current) current.lines.push({ line, lineNumber: index + 1 })
  })

  return tables
}

/**
 * The value of a dependency declaration, with its starting line. An inline
 * table may span lines, so accumulate until the braces balance.
 */
function declaration(lines, name) {
  const pattern = new RegExp(`^\\s*${name}\\s*=\\s*(.*)$`)
  for (let index = 0; index < lines.length; index += 1) {
    const first = lines[index].line.match(pattern)
    if (!first) continue

    let value = first[1]
    let end = index
    while (
      (value.match(/[{([]/g) || []).length > (value.match(/[}\])]/g) || []).length &&
      end + 1 < lines.length
    ) {
      end += 1
      value += `\n${lines[end].line}`
    }
    return { value: value.trim(), lineNumber: lines[index].lineNumber }
  }
  return null
}

/** The strings inside a Cargo `features = [ ... ]` list. */
function features(value) {
  const list = value.match(/features\s*=\s*\[([^\]]*)\]/)
  if (!list) return null
  return [...list[1].matchAll(/"([^"]+)"/g)].map((match) => match[1])
}

const found = manifests()

if (found.length === 0) {
  fail('found no Cargo.toml in the repository, so the check proved nothing')
}

for (const absolute of found) {
  const name = relative(root, absolute).replace(/\\/g, '/')
  let source
  try {
    source = read(absolute)
  } catch (error) {
    fail(`${name} could not be read: ${error.message}`)
    continue
  }

  for (const table of dependencyTables(source)) {
    const found_declaration = declaration(table.lines, 'tauri')
    if (!found_declaration) continue

    const { value, lineNumber } = found_declaration
    const list = features(value)

    if (list === null) {
      fail(
        `${name}:${lineNumber} depends on tauri without a features list. ` +
          'Without `custom-protocol` the bundled app serves build.devUrl and ' +
          'opens a blank window. Add features = ["custom-protocol"].',
      )
      continue
    }

    if (!list.includes('custom-protocol')) {
      fail(
        `${name}:${lineNumber} enables tauri features [${list.join(', ')}] but not ` +
          '`custom-protocol`. Without it the bundled app serves build.devUrl ' +
          '(http://127.0.0.1:1420) instead of the embedded frontendDist, and the ' +
          'only symptom is a blank window on a user machine.',
      )
    }
  }
}

if (failures === 0) {
  const checked = found
    .map((absolute) => relative(root, absolute).replace(/\\/g, '/'))
    .join(', ')
  console.log(`Bundle feature check passed: every tauri dependency in [${checked}] enables custom-protocol.`)
}

process.exitCode = failures === 0 ? 0 : 1
