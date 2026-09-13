import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs'
import path from 'node:path'

const root = process.cwd()
const failures = []

function relative(file) {
  return path.relative(root, file).replaceAll('\\', '/')
}

function walk(directory) {
  if (!existsSync(directory)) return []
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(directory, entry.name)
    return entry.isDirectory() ? walk(file) : [file]
  })
}

function fail(message) {
  failures.push(message)
}

const srcDir = path.join(root, 'src')
const srcFiles = walk(srcDir)
const vueFiles = srcFiles.filter((file) => file.endsWith('.vue'))

// App.vue is a known legacy hotspot. Freeze its growth while it is decomposed;
// all new/other Vue views must stay small enough to remain reviewable.
for (const file of vueFiles) {
  const rel = relative(file)
  const size = statSync(file).size
  const maxBytes = rel === 'src/App.vue' ? 115_000 : 30_000
  if (size > maxBytes) {
    fail(`${rel} is ${size} bytes; budget is ${maxBytes}. Split responsibilities before adding more.`)
  }
}

// Feature-owned views should not drift back into the generic component root.
const componentRoot = path.join(srcDir, 'components')
if (existsSync(componentRoot)) {
  for (const entry of readdirSync(componentRoot, { withFileTypes: true })) {
    if (entry.isFile() && entry.name.endsWith('.vue')) {
      fail(`src/components/${entry.name} is a feature view in the generic component root. Move it under src/features/<feature>/.`)
    }
  }
}

// The concatenated legacy stylesheet was deliberately removed. Reintroducing it
// would make cascade ownership ambiguous again.
if (existsSync(path.join(srcDir, 'style.css'))) {
  fail('src/style.css must not return. Add styles to an explicit src/styles/ layer or colocate them with a feature.')
}

// Keep the i18n bootstrap small; translation payloads belong in locale modules.
const i18nBootstrap = path.join(srcDir, 'i18n.ts')
if (existsSync(i18nBootstrap) && statSync(i18nBootstrap).size > 5_000) {
  fail('src/i18n.ts exceeded 5 KB. Translation payloads belong in src/i18n/locales/.')
}

const directTauriAllowed = new Set([
  'src/debug.ts',
  'src/services/activity-log.ts',
])

const invokeDebugImport = /import[\s\S]*?\binvokeDebug\b[\s\S]*?from\s+['"][^'"]*debug['"]/m

for (const file of srcFiles.filter((item) => /\.(ts|tsx|vue)$/.test(item))) {
  const rel = relative(file)
  const content = readFileSync(file, 'utf8')

  if (content.includes('@tauri-apps/api/core') && !directTauriAllowed.has(rel)) {
    fail(`${rel} imports the Tauri core API directly. Route native access through debug.ts or a dedicated infrastructure service.`)
  }

  if (invokeDebugImport.test(content)) {
    const isLegacyApp = rel === 'src/App.vue'
    const isFeatureService = /^src\/features\/[^/]+\/service\.ts$/.test(rel)
    if (!isLegacyApp && !isFeatureService) {
      fail(`${rel} calls invokeDebug outside a feature service. Keep native commands behind src/features/<feature>/service.ts.`)
    }
  }
}

if (failures.length) {
  console.error('\nFrontend architecture guard failed:\n')
  for (const failure of failures) console.error(`  - ${failure}`)
  console.error('\nSee docs/FRONTEND.md for the intended boundaries.\n')
  process.exit(1)
}

console.log(`Frontend architecture guard passed (${vueFiles.length} Vue files checked).`)
