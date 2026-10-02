import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { fileURLToPath } from 'node:url'

export function validateReleaseVersion(root, tag) {
  const packageVersion = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version
  const config = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
  const cargoVersion = readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8')
    .match(/^version\s*=\s*"([^"]+)"/m)?.[1]
  if (packageVersion !== config.version || packageVersion !== cargoVersion || tag !== `v${packageVersion}`) {
    throw new Error(`Release version mismatch: tag=${tag}, package=${packageVersion}, Tauri=${config.version}, Cargo=${cargoVersion}`)
  }
  if (config.bundle.createUpdaterArtifacts !== true) throw new Error('Tauri v2 updater artifacts must be enabled')
  if (!readFileSync(join(root, `docs/release-notes/.release-notes-${tag}.md`), 'utf8').trim()) {
    throw new Error('Release notes are empty')
  }
  return packageVersion
}

export function createUpdaterManifest({ version, tag, repository, installer, signature, notes, now = new Date() }) {
  if (tag !== `v${version}`) throw new Error('Manifest tag does not match its version')
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error('Invalid GitHub repository')
  if (!installer.endsWith('-setup.exe') || basename(installer) !== installer) throw new Error('Expected an NSIS setup executable filename')
  if (!signature.trim()) throw new Error('Updater signature is empty')
  return {
    version, notes, pub_date: now.toISOString(),
    platforms: {
      'windows-x86_64': {
        url: `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(installer)}`,
        signature: signature.trim(),
      },
    },
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [mode, tag, repository] = process.argv.slice(2)
  const root = process.cwd()
  const version = validateReleaseVersion(root, tag)
  if (mode === 'manifest') {
    const bundle = join(root, 'src-tauri/target/release/bundle/nsis')
    const installers = readdirSync(bundle).filter((name) => name.endsWith('-setup.exe'))
    if (installers.length !== 1) throw new Error(`Expected exactly one NSIS installer, found ${installers.length}`)
    const installer = installers[0]
    const manifest = createUpdaterManifest({
      version, tag, repository, installer,
      signature: readFileSync(join(bundle, `${installer}.sig`), 'utf8'),
      notes: readFileSync(join(root, `docs/release-notes/.release-notes-${tag}.md`), 'utf8'),
    })
    writeFileSync(join(bundle, 'latest.json'), `${JSON.stringify(manifest, null, 2)}\n`)
  } else if (mode !== 'check') throw new Error('Usage: release-artifacts.mjs check|manifest <tag> [owner/repo]')
  console.log(`Release artifacts validated for ${tag}`)
}
